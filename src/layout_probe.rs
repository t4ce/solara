//! Corpus-only host adapter. The reusable layout boundary accepts any host
//! NetProvider; this one deliberately serves only the existing embedded CSS.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use rust_qjs_dom::DomArtifact;
use solara::spec_layout::{
    DocumentConfig, LayoutSummary, SpecLayout, Viewport, bundled_font_context,
};

#[derive(Default)]
struct CorpusResources {
    unavailable: Mutex<BTreeSet<String>>,
}

impl NetProvider for CorpusResources {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url.as_str().to_owned();
        match crate::parser_probe::load_embedded_stylesheet("", None, request.url.path()) {
            Ok(loaded) => handler.bytes(loaded.resolved_url, Bytes::from(loaded.css)),
            Err(_) => {
                if let Ok(mut unavailable) = self.unavailable.lock() {
                    unavailable.insert(url.clone());
                }
                // NetHandler has no error callback. Complete with empty bytes
                // so omitted corpus assets cannot leave critical loads pending.
                // The omission is reported separately; it is not a loaded asset.
                handler.bytes(url, Bytes::new());
            }
        }
    }
}

pub(crate) fn layout_artifact(artifact: &DomArtifact) -> Result<(SpecLayout, usize), String> {
    let resources = Arc::new(CorpusResources::default());
    let mut layout = SpecLayout::from_artifact(
        artifact,
        DocumentConfig {
            viewport: Some(Viewport {
                window_size: (1280, 800),
                ..Default::default()
            }),
            font_ctx: Some(bundled_font_context()),
            net_provider: Some(resources.clone()),
            ..Default::default()
        },
    )?;
    // Synchronous corpus callbacks enqueue their responses in Blitz. Resolve
    // drains those responses before style/layout and may request more assets.
    if !layout.resolve(0.0)? {
        return Err("embedded layout is waiting for critical stylesheets".into());
    }
    let missing = resources
        .unavailable
        .lock()
        .map_err(|_| "corpus resource lock poisoned")?
        .len();
    Ok((layout, missing))
}

pub(crate) fn report(page: &str, summary: LayoutSummary, missing: usize, elapsed_us: u64) {
    crate::parser_probe::report_info(format_args!(
        "solara: layout-ready page={} engine=blitz-stylo-taffy-parley viewport=1280x800 generation={} boxes={} text_layouts={} glyphs={} unavailable_resources={} layout_us={} renderer=none",
        page,
        summary.generation,
        summary.boxes,
        summary.text_layouts,
        summary.glyphs,
        missing,
        elapsed_us,
    ));
}
