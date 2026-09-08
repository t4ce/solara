//! Offline HTML/CSS/font probe for captures from tools/capture_page.py.
//! The exported SVG contains the native solid and glyph geometry; page scripts
//! and image textures are reported separately, never presented as implemented.
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use rust_qjs_dom::{DomEngine, DomNode};
use solara::{
    native_paint::Painter,
    spec_layout::{DocumentConfig, SpecLayout, Viewport, bundled_font_context},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

struct Resources {
    root: PathBuf,
    files: BTreeMap<String, (String, String)>,
    missing: Mutex<BTreeSet<String>>,
    loaded: Mutex<BTreeSet<String>>,
}
impl NetProvider for Resources {
    fn fetch(&self, _: usize, request: Request, handler: Box<dyn NetHandler>) {
        let (bytes, resolved) = self
            .files
            .get(request.url.as_str())
            .and_then(|(file, url)| {
                std::fs::read(self.root.join(file))
                    .ok()
                    .map(|bytes| (bytes, url.clone()))
            })
            .unwrap_or_default();
        if bytes.is_empty() {
            self.missing.lock().unwrap().insert(request.url.to_string());
            handler.bytes(request.url.into(), Bytes::new());
        } else {
            self.loaded.lock().unwrap().insert(request.url.to_string());
            handler.bytes(resolved, Bytes::from(bytes));
        }
    }
}

fn scripts(node: &DomNode, counts: &mut [usize; 3]) {
    if node.tag_name.as_deref() == Some("script") {
        counts[0] += 1;
        counts[1] += usize::from(
            node.attrs
                .iter()
                .any(|a| a.name == "type" && a.value.eq_ignore_ascii_case("module")),
        );
        counts[2] += usize::from(node.attrs.iter().any(|a| a.name == "nomodule"));
    }
    for child in &node.children {
        scripts(child, counts);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        return Err("usage: page_probe CAPTURE_DIRECTORY OUTPUT.svg [WIDTH HEIGHT]".into());
    }
    let width = args.get(3).map_or(Ok(1280), |s| s.parse::<u32>())?;
    let height = args.get(4).map_or(Ok(800), |s| s.parse::<u32>())?;
    let root = PathBuf::from(&args[1]);
    let html = std::fs::read_to_string(root.join("page.html"))?;
    let url = std::fs::read_to_string(root.join("page.url"))?;
    let artifact = DomEngine::new()?.parse(&html, url.trim())?;
    let mut files = BTreeMap::new();
    for line in std::fs::read_to_string(root.join("resources.tsv"))?.lines() {
        let parts: Vec<_> = line.split('\t').collect();
        if parts.len() != 3
            || parts[1].len() != 64
            || !parts[1].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("invalid resource manifest entry".into());
        }
        files.insert(parts[0].into(), (parts[1].into(), parts[2].into()));
    }
    let resources = Arc::new(Resources {
        root,
        files,
        missing: Mutex::default(),
        loaded: Mutex::default(),
    });
    let mut layout = SpecLayout::from_artifact(
        &artifact,
        DocumentConfig {
            viewport: Some(Viewport {
                window_size: (width, height),
                ..Default::default()
            }),
            font_ctx: Some(bundled_font_context()),
            net_provider: Some(resources.clone()),
            ..Default::default()
        },
    )?;
    let mut ready = false;
    for _ in 0..32 {
        if layout.resolve(0.0)? {
            ready = true;
            break;
        }
    }
    if !ready {
        return Err("stylesheets did not settle in 32 resource rounds".into());
    }
    // Fonts may arrive during style resolution and trigger a second reflow.
    layout.resolve(0.0)?;
    let mesh = Painter::default().paint(layout.document())?;
    let view = mesh.viewport(width as f32, height as f32, 0.0);
    std::fs::write(&args[2], mesh.svg_preview(width, height, 0.0))?;
    let mut counts = [0; 3];
    scripts(&artifact.document, &mut counts);
    println!(
        "url={} viewport={width}x{height} boxes={} glyphs={} height={} triangles={} color_runs={} scripts={} modules={} nomodule={} scripts_executed=0 loaded_resources={} missing_resources={}",
        url.trim(),
        mesh.boxes,
        mesh.glyphs,
        mesh.height,
        view.triangles.len() / 3,
        view.color_runs().count(),
        counts[0],
        counts[1],
        counts[2],
        resources.loaded.lock().unwrap().len(),
        resources.missing.lock().unwrap().len()
    );
    for url in resources.missing.lock().unwrap().iter() {
        println!("missing: {url}");
    }
    Ok(())
}
