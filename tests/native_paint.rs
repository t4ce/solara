#![cfg(feature = "spec-layout")]
use rust_qjs_dom::{DomEngine, LoadedStylesheet};
use solara::{
    native_paint::Painter,
    spec_layout::{DocumentConfig, SpecLayout, Viewport, bundled_font_context},
};

struct Resources;
impl blitz_traits::net::NetProvider for Resources {
    fn fetch(
        &self,
        _: usize,
        req: blitz_traits::net::Request,
        handler: Box<dyn blitz_traits::net::NetHandler>,
    ) {
        if solara::native_paint::raster_image_url(&req.url) {
            return; // Tests deliver decoded images through SpecLayout::load_image.
        }
        let css = match req.url.path().rsplit('/').next().unwrap_or("") {
            "TextAndBorders.css" => include_str!("../docs/TextAndBorders.css"),
            "demoui.css" => include_str!("../docs/demoui.css"),
            _ => "",
        };
        handler.bytes(req.url.to_string(), blitz_traits::net::Bytes::from(css));
    }
}

fn document(html: &str) -> SpecLayout {
    let mut engine = DomEngine::with_stylesheet_loader(|_, _, href| {
        let css = match href.rsplit('/').next().unwrap_or(href) {
            "TextAndBorders.css" => include_str!("../docs/TextAndBorders.css"),
            "demoui.css" => include_str!("../docs/demoui.css"),
            _ => return Err(format!("unavailable {href}")),
        };
        Ok(LoadedStylesheet::new(
            format!("trueos://solara/docs/{href}"),
            css,
        ))
    })
    .unwrap();
    let artifact = engine
        .parse(html, "trueos://solara/docs/test.html")
        .unwrap();
    let mut layout = SpecLayout::from_artifact(
        &artifact,
        DocumentConfig {
            viewport: Some(Viewport {
                window_size: (960, 640),
                ..Default::default()
            }),
            font_ctx: Some(bundled_font_context()),
            net_provider: Some(std::sync::Arc::new(Resources)),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(layout.resolve(0.0).unwrap());
    layout
}

#[test]
fn all_four_documents_have_native_text_and_css_solids() {
    for (name, html) in [
        ("framework", include_str!("../docs/FrameworkLayout.html")),
        ("text", include_str!("../docs/TextAndBorders.html")),
        ("panels", include_str!("../docs/DivsAndPanels.html")),
        ("forms", include_str!("../docs/FlowAndForms.html")),
    ] {
        let layout = document(html);
        let mut painter = Painter::default();
        let mesh = painter.paint(layout.document()).unwrap();
        assert!(mesh.glyphs > 0, "{name}");
        assert!(!mesh.triangles.is_empty(), "{name}");
        assert!(mesh.boxes > 0, "{name}");
        assert_eq!(mesh.triangle_colors.len() * 3, mesh.triangles.len());
        assert_eq!(mesh.triangles.len() % 3, 0);
        assert!(
            mesh.triangles
                .iter()
                .all(|i| (*i as usize) < mesh.vertices.len())
        );
        let cached = painter.cached_glyphs();
        let again = painter.paint(layout.document()).unwrap();
        assert_eq!(cached, painter.cached_glyphs());
        assert_eq!(mesh.vertices, again.vertices);
        if let Ok(dir) = std::env::var("SOLARA_MESH_PREVIEW_DIR") {
            let svg = mesh.svg_preview(960, 640, 0.0);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(format!("{dir}/{name}.svg"), svg).unwrap();
        }
    }
}

#[test]
fn glyph_counter_is_empty_and_reflow_reuses_outlines() {
    let mut layout = document("<style>body{margin:0;font:64px monospace}</style><div>O</div>");
    let mut painter = Painter::default();
    let mesh = painter.paint(layout.document()).unwrap();
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for i in &mesh.triangles {
        let p = mesh.vertices[*i as usize];
        for a in 0..2 {
            min[a] = min[a].min(p[a]);
            max[a] = max[a].max(p[a]);
        }
    }
    let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let cross = |a: [f32; 2], b: [f32; 2], p: [f32; 2]| {
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    };
    assert!(!mesh.triangles.as_chunks::<3>().0.iter().any(|t| {
        let [a, b, c] = [
            mesh.vertices[t[0] as usize],
            mesh.vertices[t[1] as usize],
            mesh.vertices[t[2] as usize],
        ];
        let v = [
            cross(a, b, center),
            cross(b, c, center),
            cross(c, a, center),
        ];
        v.iter().all(|v| *v > 0.0) || v.iter().all(|v| *v < 0.0)
    }));
    let cached = painter.cached_glyphs();
    layout
        .set_viewport(Viewport {
            window_size: (480, 640),
            ..Default::default()
        })
        .unwrap();
    assert!(layout.resolve(0.0).unwrap());
    painter.paint(layout.document()).unwrap();
    assert_eq!(cached, painter.cached_glyphs());
}

#[test]
fn viewport_compacts_visible_primitives_and_scroll_reveals_the_rest() {
    let layout = document(
        "<style>body{margin:0;font:32px monospace}div{height:1000px}</style><div>TOP</div><div>BOTTOM</div>",
    );
    let mesh = Painter::default().paint(layout.document()).unwrap();
    let first = mesh.viewport(960.0, 640.0, 0.0);
    let second = mesh.viewport(960.0, 640.0, 1000.0);
    assert!(!first.triangles.is_empty());
    assert!(!second.triangles.is_empty());
    assert!(first.triangles.len() < mesh.triangles.len());
    assert!(second.triangles.len() < mesh.triangles.len());
    for view in [first, second] {
        assert_eq!(view.triangle_colors.len() * 3, view.triangles.len());
        assert!(
            view.triangles
                .iter()
                .all(|index| (*index as usize) < view.vertices.len())
        );
    }
}

#[test]
fn raster_image_urls_use_path_extensions() {
    for url in [
        "https://example.test/cat.JPG?v=2#image",
        "trueos://solara/assets/cat.jpeg",
        "https://example.test/cat.PNG?v=2#image",
    ] {
        assert!(solara::native_paint::raster_image_url(
            &url::Url::parse(url).unwrap()
        ));
    }
    for url in [
        "https://example.test/cat.svg",
        "https://example.test/image?name=cat.jpg",
    ] {
        assert!(!solara::native_paint::raster_image_url(
            &url::Url::parse(url).unwrap()
        ));
    }
}

#[test]
fn decoded_images_reflow_and_crop_without_rebuilding_glyphs_on_scroll() {
    struct Images;
    impl blitz_traits::net::NetProvider for Images {
        fn fetch(
            &self,
            _: usize,
            _: blitz_traits::net::Request,
            _: Box<dyn blitz_traits::net::NetHandler>,
        ) {
        }
    }
    let mut engine = DomEngine::new().unwrap();
    let artifact = engine
        .parse(
            r#"<html><head><base href="https://example.test/pictures/"><style>
        body { margin:0 } img { display:block }
        #cover { width:200px;height:60px;object-fit:cover }
        #contain { width:200px;height:60px;object-fit:contain; transform:translateX(10px) }
        </style></head><body><img src="cat.PNG?v=1"><img id="cover" src="cat.jpeg">
        <div style="height:800px"></div><img id="contain" src="cat.jpeg"></body></html>"#,
            "https://example.test/page.html",
        )
        .unwrap();
    let mut layout = SpecLayout::from_artifact(
        &artifact,
        DocumentConfig {
            viewport: Some(Viewport {
                window_size: (400, 300),
                ..Default::default()
            }),
            font_ctx: Some(bundled_font_context()),
            net_provider: Some(std::sync::Arc::new(Images)),
            ..Default::default()
        },
    )
    .unwrap();
    layout.resolve(0.0).unwrap();
    layout.load_image(
        "https://example.test/pictures/cat.jpeg".into(),
        80,
        40,
        std::sync::Arc::new(vec![255; 80 * 40 * 4]),
    );
    layout.resolve(0.0).unwrap();
    layout.load_image(
        "https://example.test/pictures/cat.PNG?v=1".into(),
        80,
        40,
        std::sync::Arc::new(vec![127; 80 * 40 * 4]),
    );
    layout.resolve(0.0).unwrap();
    let mut painter = Painter::default();
    let mesh = painter.paint(layout.document()).unwrap();
    assert_eq!(mesh.images.len(), 3);
    let natural = &mesh.images[0];
    assert_eq!(natural.corners[2], [80.0, 40.0]);
    let cover = &mesh.images[1];
    assert!((cover.uv[0][1] - 0.2).abs() < 0.001);
    assert!((cover.uv[2][1] - 0.8).abs() < 0.001);
    let contain = &mesh.images[2];
    assert!((contain.corners[0][0] - 50.0).abs() < 0.01);
    assert!(!contain.visible(400.0, 300.0, 0.0));
    assert!(contain.visible(400.0, 300.0, 800.0));
    assert_eq!(contain.uv, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
}

#[test]
fn homepage_keeps_the_complete_logo_centered_across_window_sizes() {
    let mut layout = document(include_str!("../docs/home.html"));
    layout.load_image(
        "trueos://solara/assets/logo.jpg".into(),
        640,
        360,
        std::sync::Arc::new(vec![255; 640 * 360 * 4]),
    );
    for (width, height) in [(800, 512), (480, 320), (2480, 1340)] {
        layout
            .set_viewport(Viewport {
                window_size: (width, height),
                ..Default::default()
            })
            .unwrap();
        assert!(layout.resolve(0.0).unwrap());
        let mesh = Painter::default().paint(layout.document()).unwrap();
        assert_eq!(mesh.images.len(), 1);
        let image = &mesh.images[0];
        assert_eq!(image.uv, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        let [left, top] = image.corners[0];
        let [right, bottom] = image.corners[2];
        assert!((left + right - width as f32).abs() <= 1.0);
        assert!(
            (top + bottom - height as f32).abs() <= 1.0,
            "viewport={width}x{height} corners={:?}",
            image.corners
        );
        assert!(left >= 0.0 && top >= 0.0);
        assert!(right <= width as f32 && bottom <= height as f32);
        assert!(((right - left) / (bottom - top) - 640.0 / 360.0).abs() < 0.001);
    }
}

#[test]
fn button_hover_uses_css_cascade_and_retained_glyphs() {
    let mut layout = document(
        r#"<style>
        body{margin:0} button{position:absolute;left:20px;top:20px;width:140px;height:40px;
        padding:0;border-style:solid;border-width:2px}
        #disabled{top:80px} #author{top:140px;border-color:#123456}
        #author:hover{border-color:#abcdef;transform:translateX(8px)}
        #below{top:800px}
        </style><button id="plain"><span>Hover me</span></button>
        <button id="disabled" disabled>Disabled</button>
        <button id="author">Author CSS</button><button id="below">Scrolled</button>"#,
    );
    let mut painter = Painter::default();
    let initial = painter.paint(layout.document()).unwrap();
    let cached = painter.cached_glyphs();
    let border = |layout: &SpecLayout, id| {
        let id = layout.document().get_element_by_id(id).unwrap();
        layout
            .document()
            .resolved_style_value(id, "border-top-color")
    };
    assert_eq!(border(&layout, "plain"), "rgb(65, 151, 174)");
    assert!(layout.pointer_move(Some([80.0, 40.0]), 0.0));
    layout.resolve(0.0).unwrap();
    assert_eq!(border(&layout, "plain"), "rgb(229, 237, 248)");
    let hovered = painter.paint(layout.document()).unwrap();
    assert_eq!(initial.vertices, hovered.vertices);
    assert_eq!(cached, painter.cached_glyphs());
    assert_ne!(initial.triangle_colors, hovered.triangle_colors);
    let clipped = hovered.viewport(960.0, 640.0, 0.0);
    assert_eq!(clipped.triangle_colors.len() * 3, clipped.triangles.len());
    assert!(
        clipped
            .triangle_colors
            .contains(&u32::from_le_bytes([229, 237, 248, 255]))
    );
    assert!(!layout.pointer_move(Some([81.0, 40.0]), 0.0));

    layout.pointer_move(Some([80.0, 100.0]), 0.0);
    layout.resolve(0.0).unwrap();
    assert_eq!(border(&layout, "disabled"), "rgb(65, 151, 174)");
    assert_eq!(border(&layout, "plain"), "rgb(65, 151, 174)");
    layout.pointer_move(Some([80.0, 160.0]), 0.0);
    layout.resolve(0.0).unwrap();
    assert_eq!(border(&layout, "author"), "rgb(171, 205, 239)");

    layout.pointer_move(Some([80.0, 40.0]), 780.0);
    layout.resolve(0.0).unwrap();
    assert_eq!(border(&layout, "below"), "rgb(229, 237, 248)");
    assert_eq!(border(&layout, "author"), "rgb(18, 52, 86)");
    assert!(layout.pointer_move(None, 780.0));
    layout.resolve(0.0).unwrap();
    assert_eq!(border(&layout, "below"), "rgb(65, 151, 174)");
}

#[test]
fn disclosures_collapse_reflow_and_activate_after_scrolling() {
    let mut layout = document(
        r#"<style>body{margin:0} summary{min-height:30px} p{height:80px}</style>
        <details id="root" open><summary id="root-label">disc001 <button id="upload" type="button"><span>upload</span></button></summary>
        <div><details id="folder"><summary id="folder-label"><b>apps</b></summary>
        <p>Hidden folder contents</p><summary id="second">Extra summary is ordinary content</summary></details></div></details>
        <div style="height:800px"></div><details id="below"><summary id="below-label">Scrolled folder</summary><p>Below</p></details>"#,
    );
    let id = |layout: &SpecLayout, name| layout.document().get_element_by_id(name).unwrap();
    let open = |layout: &SpecLayout, name| {
        layout
            .document()
            .get_node(id(layout, name))
            .unwrap()
            .element_data()
            .unwrap()
            .attr(blitz_dom::local_name!("open"))
            .is_some()
    };
    let point = |layout: &SpecLayout, name, scroll| {
        let rect = layout
            .document()
            .get_client_bounding_rect(id(layout, name))
            .unwrap();
        [rect.x as f32 + 6.0, rect.y as f32 + 10.0 - scroll]
    };
    let click = |layout: &mut SpecLayout, p, scroll| {
        assert!(!layout.pointer_button(Some(p), scroll, true));
        let changed = layout.pointer_button(Some(p), scroll, false);
        layout.resolve(0.0).unwrap();
        changed
    };
    let mut painter = Painter::default();
    let closed = painter.paint(layout.document()).unwrap();
    assert!(!open(&layout, "folder"));
    let p = point(&layout, "upload", 0.0);
    assert!(!click(&mut layout, p, 0.0));
    assert!(open(&layout, "root"));
    let p = point(&layout, "folder-label", 0.0);
    assert!(click(&mut layout, p, 0.0));
    assert!(open(&layout, "folder"));
    let expanded = painter.paint(layout.document()).unwrap();
    assert!(expanded.glyphs > closed.glyphs);
    assert!(expanded.height > closed.height);
    let p = point(&layout, "second", 0.0);
    assert!(!click(&mut layout, p, 0.0));
    let p = point(&layout, "folder-label", 0.0);
    assert!(click(&mut layout, p, 0.0));
    let collapsed = painter.paint(layout.document()).unwrap();
    assert_eq!(closed.glyphs, collapsed.glyphs);
    assert_eq!(closed.height, collapsed.height);
    let p = point(&layout, "below-label", 800.0);
    assert!(click(&mut layout, p, 800.0));
    assert!(open(&layout, "below"));
    // Release alone and losing the pointer route must not activate anything.
    assert!(!layout.pointer_button(Some(p), 800.0, false));
    layout.pointer_button(Some(p), 800.0, true);
    layout.pointer_button(None, 800.0, true);
    assert!(!layout.pointer_button(Some(p), 800.0, false));
    // The actual shaped disclosure marker must have a font glyph, not .notdef.
    let summary = layout
        .document()
        .get_node(id(&layout, "below-label"))
        .unwrap();
    let text = summary
        .element_data()
        .unwrap()
        .inline_layout_data
        .as_ref()
        .unwrap();
    let first = text.layout.lines().next().unwrap().items().next().unwrap();
    let parley::PositionedLayoutItem::GlyphRun(run) = first else {
        panic!("missing disclosure glyph")
    };
    assert_ne!(run.positioned_glyphs().next().unwrap().id, 0);
}

fn pixel(mesh: &solara::native_paint::PageMesh, p: [f32; 2]) -> u32 {
    let mut color = mesh.canvas_color;
    for (triangle, &fill) in mesh
        .triangles
        .as_chunks::<3>()
        .0
        .iter()
        .zip(&mesh.triangle_colors)
    {
        let points = triangle
            .iter()
            .map(|&i| mesh.vertices[i as usize])
            .collect::<Vec<_>>();
        let cross = |a: [f32; 2], b: [f32; 2]| {
            (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
        };
        let signs = [
            cross(points[0], points[1]),
            cross(points[1], points[2]),
            cross(points[2], points[0]),
        ];
        if (signs.iter().all(|v| *v >= 0.0) || signs.iter().all(|v| *v <= 0.0))
            && signs.iter().any(|v| v.abs() > 0.001)
        {
            color = fill;
        }
    }
    color
}

#[test]
fn css_panels_borders_and_nested_overflow_keep_paint_order() {
    let layout = document(
        r#"<style>
        body { margin:0;background:#fff }
        #panel { position:relative; width:100px;height:100px;background:#f00;overflow:hidden }
        #child { position:absolute;left:20px;top:20px;width:50px;height:150px;background:#00f }
        #overlay { position:absolute;left:30px;top:30px;width:30px;height:30px;background:#0f0 }
        #border { position:absolute;left:150px;top:0;width:60px;height:60px;box-sizing:border-box;
            background:#f00;border:8px solid #00f;border-radius:20px }
        .sr { position:absolute;clip:rect(0,0,0,0);width:1px;height:1px;overflow:hidden;white-space:nowrap }
        </style><div id='panel'><div id='child'></div></div><div id='overlay'></div><div id='border'></div>
        <div class='sr'>This accessibility label must not leak into the page.</div>"#,
    );
    let mesh = Painter::default().paint(layout.document()).unwrap();
    assert_eq!(pixel(&mesh, [10.0, 12.0]), 0xff0000ff);
    assert_eq!(pixel(&mesh, [25.0, 27.0]), 0xffff0000);
    assert_eq!(pixel(&mesh, [35.0, 37.0]), 0xff00ff00);
    assert_eq!(pixel(&mesh, [25.0, 110.0]), 0xffffffff);
    assert_eq!(pixel(&mesh, [151.0, 1.0]), 0xffffffff);
    assert_eq!(pixel(&mesh, [180.0, 3.0]), 0xffff0000);
    assert_eq!(pixel(&mesh, [180.0, 30.0]), 0xff0000ff);
    assert!(
        mesh.triangle_colors
            .iter()
            .all(|c| [0xff0000ff, 0xffff0000, 0xff00ff00].contains(c))
    );
    let view = mesh.viewport(300.0, 200.0, 0.0);
    assert_eq!(pixel(&view, [35.0, 37.0]), pixel(&mesh, [35.0, 37.0]));
    assert_eq!(
        view.color_runs()
            .map(|(indices, _)| indices.len())
            .sum::<usize>(),
        view.triangles.len()
    );
}

#[test]
fn paint_order_uses_stacking_contexts_and_live_dom_order() {
    let mut layout = document(
        r#"<style>
        body{margin:0} .box{position:absolute;left:10px;top:10px;width:100px;height:100px}
        #front{background:#f00;z-index:3} #back{background:#00f;z-index:1}
        </style><div class='box' id='front'></div><div class='box' id='back'></div>"#,
    );
    let mut painter = Painter::default();
    assert_eq!(
        pixel(&painter.paint(layout.document()).unwrap(), [40.0, 45.0]),
        0xff0000ff
    );
    let front = layout.document().get_element_by_id("front").unwrap();
    let back = layout.document().get_element_by_id("back").unwrap();
    let style = blitz_dom::QualName::new(None, "".into(), "style".into());
    layout
        .mutate()
        .set_attribute(front, style.clone(), "z-index:auto");
    layout.mutate().set_attribute(back, style, "z-index:auto");
    layout.resolve(0.0).unwrap();
    assert_eq!(
        pixel(&painter.paint(layout.document()).unwrap(), [40.0, 45.0]),
        0xffff0000
    );
    layout.mutate().insert_nodes_before(front, &[back]);
    layout.resolve(0.0).unwrap();
    assert_eq!(
        pixel(&painter.paint(layout.document()).unwrap(), [40.0, 45.0]),
        0xff0000ff
    );
}

#[test]
fn inline_colors_and_input_values_use_their_resolved_text_runs() {
    let layout = document(
        r#"<style>body{margin:0;color:#f00} span{color:#00f} input{color:#008000}</style>
        <p>Red <span>Blue</span></p><input value='Typed value'>"#,
    );
    let mesh = Painter::default().paint(layout.document()).unwrap();
    for color in [0xff0000ff, 0xffff0000, 0xff008000] {
        assert!(
            mesh.triangle_colors.contains(&color),
            "missing glyph color {color:08x}"
        );
    }
    let empty = document("<input value=''>");
    let filled = document("<input value='Typed value'>");
    assert!(
        Painter::default().paint(filled.document()).unwrap().glyphs
            > Painter::default().paint(empty.document()).unwrap().glyphs
    );
}

#[test]
fn negative_z_children_paint_between_parent_background_and_text() {
    let layout = document(
        r#"<style>
        body{margin:0} #parent{position:relative;z-index:0;width:200px;height:100px;background:#0f0;color:#f00}
        #negative{position:absolute;z-index:-1;inset:0;background:#00f}
        </style><div id='parent'>Visible text<div id='negative'></div></div>"#,
    );
    let mesh = Painter::default().paint(layout.document()).unwrap();
    let first_red = mesh
        .triangle_colors
        .iter()
        .position(|c| *c == 0xff0000ff)
        .unwrap();
    let last_blue = mesh
        .triangle_colors
        .iter()
        .rposition(|c| *c == 0xffff0000)
        .unwrap();
    let last_green = mesh
        .triangle_colors
        .iter()
        .rposition(|c| *c == 0xff00ff00)
        .unwrap();
    assert!(last_green < last_blue && last_blue < first_red);
}
