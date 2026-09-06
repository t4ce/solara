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
fn all_four_documents_have_native_text_and_closed_box_edges() {
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
        assert_eq!(mesh.lines.len(), mesh.boxes * 8);
        assert_eq!(mesh.triangles.len() % 3, 0);
        assert!(
            mesh.triangles
                .iter()
                .chain(&mesh.lines)
                .all(|i| (*i as usize) < mesh.vertices.len())
        );
        let cached = painter.cached_glyphs();
        let again = painter.paint(layout.document()).unwrap();
        assert_eq!(cached, painter.cached_glyphs());
        assert_eq!(mesh.vertices, again.vertices);
        if let Ok(dir) = std::env::var("SOLARA_MESH_PREVIEW_DIR") {
            use std::fmt::Write;
            let mut svg = String::from(
                "<svg xmlns='http://www.w3.org/2000/svg' width='960' height='640' viewBox='0 0 960 640'><rect width='960' height='640' fill='#0d121b'/><g stroke='#4197ae' stroke-width='3'>",
            );
            for e in mesh.lines.as_chunks::<2>().0.iter() {
                let a = mesh.vertices[e[0] as usize];
                let b = mesh.vertices[e[1] as usize];
                write!(svg, "<path d='M{},{} L{},{}'/>", a[0], a[1], b[0], b[1]).unwrap();
            }
            svg.push_str("</g><path fill='#e5edf8' d='");
            for t in mesh.triangles.as_chunks::<3>().0.iter() {
                let a = mesh.vertices[t[0] as usize];
                let b = mesh.vertices[t[1] as usize];
                let c = mesh.vertices[t[2] as usize];
                write!(
                    svg,
                    "M{},{} L{},{} {},{} Z ",
                    a[0], a[1], b[0], b[1], c[0], c[1]
                )
                .unwrap();
            }
            svg.push_str("'/></svg>");
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
        assert!(view.lines.iter().all(|index| *index < 100));
        assert!(
            view.triangles
                .iter()
                .chain(&view.lines)
                .all(|index| (*index as usize) < view.vertices.len())
        );
    }
}

#[test]
fn jpeg_urls_use_path_extensions() {
    for url in [
        "https://example.test/cat.JPG?v=2#image",
        "trueos://solara/assets/cat.jpeg",
    ] {
        assert!(solara::native_paint::jpeg_url(
            &url::Url::parse(url).unwrap()
        ));
    }
    for url in [
        "https://example.test/cat.png",
        "https://example.test/image?name=cat.jpg",
    ] {
        assert!(!solara::native_paint::jpeg_url(
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
        </style></head><body><img src="cat.jpeg"><img id="cover" src="cat.jpeg">
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
