#![cfg(feature = "spec-layout")]

use std::sync::{Arc, Mutex};

use blitz_dom::QualName;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use parley::PositionedLayoutItem;
use rust_qjs_dom::{DomArtifact, DomEngine, LoadedStylesheet};
use solara::spec_layout::{DocumentConfig, NodeId, SpecLayout, Viewport, bundled_font_context};

fn artifact(html: &str) -> DomArtifact {
    DomEngine::new()
        .unwrap()
        .parse(html, "trueos://solara/page.html")
        .unwrap()
}

fn viewport(width: u32) -> Viewport {
    Viewport {
        window_size: (width, 800),
        ..Default::default()
    }
}

fn config(width: u32) -> DocumentConfig {
    DocumentConfig {
        viewport: Some(viewport(width)),
        font_ctx: Some(bundled_font_context()),
        ..Default::default()
    }
}

fn node(layout: &SpecLayout, id: &str) -> NodeId {
    layout.document().get_element_by_id(id).unwrap()
}

fn width(layout: &SpecLayout, id: &str) -> f32 {
    layout
        .document()
        .get_node(node(layout, id))
        .unwrap()
        .final_layout()
        .size
        .width
}

fn height(layout: &SpecLayout, id: &str) -> f32 {
    layout
        .document()
        .get_node(node(layout, id))
        .unwrap()
        .final_layout()
        .size
        .height
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 1.0,
        "expected {expected}, got {actual}"
    );
}

fn font_ids(layout: &SpecLayout) -> Vec<(u64, u32)> {
    let mut fonts = Vec::new();
    for (_, n) in layout.document().tree().iter() {
        let Some(text) = n.element_data().and_then(|e| e.inline_layout_data.as_ref()) else {
            continue;
        };
        for line in text.layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    let font = run.run().font();
                    fonts.push((font.data.id(), font.index));
                    assert!(
                        run.positioned_glyphs()
                            .all(|g| g.x.is_finite() && g.y.is_finite())
                    );
                }
            }
        }
    }
    fonts.sort_unstable();
    fonts.dedup();
    fonts
}

#[test]
fn framework_grid_flex_layers_variables_and_media_queries_reflow_the_same_document() {
    let source = artifact(include_str!("../docs/FrameworkLayout.html"));
    let mut layout = SpecLayout::from_artifact(&source, config(1200)).unwrap();
    assert!(layout.resolve(0.0).unwrap());
    let card = node(&layout, "card-a");
    close(width(&layout, "sidebar"), 240.0);
    close(width(&layout, "content"), 888.0);
    close(width(&layout, "card-a"), (888.0 - 32.0) / 3.0);
    let toolbar = layout
        .document()
        .get_client_bounding_rect(node(&layout, "toolbar"))
        .unwrap();
    let status = layout
        .document()
        .get_client_bounding_rect(node(&layout, "status"))
        .unwrap();
    assert!((status.x + status.width - toolbar.x - toolbar.width).abs() <= 1.0);
    let fonts = font_ids(&layout);
    assert!(!fonts.is_empty());
    assert!(layout.summary().glyphs > 50);

    layout.set_viewport(viewport(640)).unwrap();
    assert!(layout.resolve(0.1).unwrap());
    assert_eq!(node(&layout, "card-a"), card);
    close(width(&layout, "card-a"), 592.0);
    close(width(&layout, "sidebar"), 0.0);
    assert_eq!(font_ids(&layout), fonts, "reflow reuses font resources");
    assert_eq!(layout.summary().generation, 2);
}

#[test]
fn live_class_and_text_mutations_restyle_and_remeasure_without_reimport() {
    let mut source = artifact(
        r#"<!doctype html><style>
        #box { width: 120px; font: 16px/24px monospace; }
        #box.wide { width: 360px; }
        </style><div id="box">a few words</div>"#,
    );
    // The old snapshot is deliberately inconsistent with the actual author CSS.
    // It must not become inline style and defeat selectors or later mutations.
    for style in &mut source.style_index.style_table {
        style
            .cascaded_declarations
            .insert("width".into(), "999px".into());
    }
    let mut layout = SpecLayout::from_artifact(&source, config(800)).unwrap();
    assert!(layout.resolve(0.0).unwrap());
    close(width(&layout, "box"), 120.0);
    let id = node(&layout, "box");
    let before = height(&layout, "box");
    let text_id = layout.document().get_node(id).unwrap().children[0];
    layout.mutate().set_node_text(text_id, "A longer paragraph whose measured glyph advances require multiple lines inside this narrow box.");
    assert!(layout.resolve(0.1).unwrap());
    assert!(height(&layout, "box") > before);
    let narrow = height(&layout, "box");
    layout
        .mutate()
        .set_attribute(id, QualName::new(None, "".into(), "class".into()), "wide");
    assert!(layout.resolve(0.2).unwrap());
    close(width(&layout, "box"), 360.0);
    assert!(height(&layout, "box") < narrow);
    assert_eq!(node(&layout, "box"), id);
}

#[derive(Default)]
struct DeferredCss(Mutex<Vec<(String, Box<dyn NetHandler>)>>);

impl NetProvider for DeferredCss {
    fn fetch(&self, _: usize, request: Request, handler: Box<dyn NetHandler>) {
        self.0.lock().unwrap().push((request.url.into(), handler));
    }
}

#[test]
fn linked_css_keeps_source_order_and_waits_for_the_host_provider() {
    let html = r#"<!doctype html><base href="/assets/"><link rel="stylesheet" href="first.css">
        <style>#box { width: 180px; }</style><div id="box">Measured</div>"#;
    let mut engine = DomEngine::with_stylesheet_loader(|_, _, _| {
        Ok(LoadedStylesheet::new(
            "trueos://solara/assets/first.css",
            "#box { width: 80px; }",
        ))
    })
    .unwrap();
    let source = engine.parse(html, "trueos://solara/page.html").unwrap();
    let net = Arc::new(DeferredCss::default());
    let mut cfg = config(800);
    cfg.net_provider = Some(net.clone());
    let mut layout = SpecLayout::from_artifact(&source, cfg).unwrap();
    assert_eq!(layout.source_url().as_str(), "trueos://solara/page.html");
    assert!(!layout.resolve(0.0).unwrap());
    assert_eq!(layout.summary().generation, 0);
    let requests = std::mem::take(&mut *net.0.lock().unwrap());
    assert_eq!(requests.len(), 1);
    for (url, handler) in requests {
        assert_eq!(url, "trueos://solara/assets/first.css");
        handler.bytes(url, Bytes::from_static(b"#box { width: 80px; }"));
    }
    assert!(layout.resolve(0.1).unwrap());
    close(width(&layout, "box"), 180.0);
}

#[test]
fn namespaces_templates_and_artifact_node_references_survive_import() {
    let source = artifact(
        r##"<!doctype html><div id="live">live</div>
        <template id="template"><style>#live { width: 999px }</style><div id="inert">hidden</div></template>
        <svg id="vector"><use xmlns:xlink="http://www.w3.org/1999/xlink" xlink:href="#shape"/></svg>"##,
    );
    let mut layout = SpecLayout::from_artifact(&source, config(800)).unwrap();
    assert!(layout.resolve(0.0).unwrap());
    assert!(layout.document().get_element_by_id("inert").is_none());
    let svg = layout
        .document()
        .get_node(node(&layout, "vector"))
        .unwrap()
        .element_data()
        .unwrap();
    assert_eq!(svg.name.ns.as_ref(), "http://www.w3.org/2000/svg");
    let source_ref = source
        .style_index
        .node_style_refs
        .iter()
        .find(|r| layout.source_node(&r.path) == Some(node(&layout, "live")));
    assert!(source_ref.is_some());
    assert!(width(&layout, "live") < 999.0, "template CSS is inert");
    let template = node(&layout, "template");
    let body = layout.document().find_body_node().unwrap().id;
    {
        let mut mutations = layout.mutate();
        let content = mutations.template_contents(template);
        let children = mutations.child_ids(content);
        mutations.append_children(body, &children);
    }
    assert!(layout.resolve(0.1).unwrap());
    assert!(layout.document().get_element_by_id("inert").is_some());
    close(width(&layout, "live"), 999.0);
}

#[test]
fn css_animation_sampling_is_driven_by_the_host_clock() {
    let source = artifact(
        r#"<!doctype html><style>
        @keyframes grow { from { width: 100px } to { width: 200px } }
        #box { height: 20px; animation: grow 1s linear forwards; }
        </style><div id="box"></div>"#,
    );
    let mut layout = SpecLayout::from_artifact(&source, config(800)).unwrap();
    assert!(layout.resolve(0.0).unwrap());
    close(width(&layout, "box"), 100.0);
    assert!(layout.resolve(0.5).unwrap());
    close(width(&layout, "box"), 150.0);
    assert!(layout.resolve(1.0).unwrap());
    close(width(&layout, "box"), 200.0);
}

#[test]
fn invalid_viewports_and_clocks_do_not_enter_the_layout_engine() {
    let source = artifact("<!doctype html><p>Text</p>");
    assert!(SpecLayout::from_artifact(&source, config(0)).is_err());
    let mut layout = SpecLayout::from_artifact(&source, config(800)).unwrap();
    assert!(layout.resolve(f64::NAN).is_err());
    let mut invalid = viewport(800);
    invalid.zoom = f32::INFINITY;
    assert!(layout.set_viewport(invalid).is_err());
    assert_eq!(layout.summary().generation, 0);
}
