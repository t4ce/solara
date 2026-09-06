//! Retained standards-based layout for TRUEOS/Solara.
//!
//! Parse5 supplies tree construction. Blitz owns the live document, Stylo's
//! cascade, Taffy's box layout and Parley's shaped text. The parser's
//! `style_index` is diagnostic data, never a replacement for author rules.
//! This module does not paint or publish a Picasso scene.

use std::collections::BTreeMap;
use std::sync::Arc;

use blitz_dom::{Attribute, BaseDocument, DocumentMutator, QualName};
pub use blitz_dom::{DocumentConfig, NodeId};
pub use blitz_traits::shell::Viewport;
use parley::{FontContext, PositionedLayoutItem};
use rust_qjs_dom::{DomArtifact, DomNode};

mod import;

/// Own this alongside the page runtime; resizing and mutations reuse its nodes,
/// stylesheets, fonts and layout caches. NodeIds remain stable across reflow.
pub struct SpecLayout {
    document: BaseDocument,
    source_url: url::Url,
    source_nodes: BTreeMap<String, NodeId>,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutSummary {
    pub generation: u64,
    pub boxes: usize,
    pub text_layouts: usize,
    pub glyphs: usize,
}

impl SpecLayout {
    /// Import the artifact tree once, preserving namespaces, inline CSS, style
    /// elements and stylesheet links. `config.net_provider` resolves resources;
    /// no desktop shell or network implementation is selected here.
    pub fn from_artifact(
        artifact: &DomArtifact,
        mut config: DocumentConfig,
    ) -> Result<Self, String> {
        artifact.validate_contract()?;
        if artifact.document.node_name != "#document" {
            return Err("layout requires a complete Parse5 document".into());
        }
        validate_viewport(
            config
                .viewport
                .as_ref()
                .ok_or("layout requires a viewport")?,
        )?;
        let source_url = url::Url::parse(&artifact.source.url)
            .map_err(|error| format!("invalid layout document URL: {error}"))?;
        // Blitz has one URL for resolving resources. Preserve the page identity
        // separately while using Parse5's first <base href> for relative URLs.
        let resource_base = artifact
            .asset_index
            .base_href
            .as_deref()
            .and_then(|href| source_url.join(href).ok())
            .unwrap_or_else(|| source_url.clone());
        config.base_url = Some(resource_base.into());
        // Sequential style traversal avoids depending on a host thread pool.
        config.style_threading = blitz_dom::StyleThreading::Sequential;
        let mut document = BaseDocument::new(config);
        document.add_user_agent_stylesheet(include_str!("button-defaults.css"));
        let mut source_nodes = BTreeMap::new();
        let root = document.root_node().id;
        source_nodes.insert("root".into(), root);
        {
            let mut mutator = document.mutate();
            import::children(
                &mut mutator,
                root,
                &artifact.document,
                "root",
                &mut source_nodes,
            )?;
        }
        if document.try_root_element().is_none() {
            return Err("layout document has no root element".into());
        }
        Ok(Self {
            document,
            source_url,
            source_nodes,
            generation: 0,
        })
    }

    /// Resolve at a host-provided animation time in seconds. A false result
    /// means critical stylesheets are pending, so the host must retry after
    /// resource completion rather than publish the previous layout as current.
    pub fn resolve(&mut self, time_seconds: f64) -> Result<bool, String> {
        if !time_seconds.is_finite() || time_seconds < 0.0 {
            return Err("layout animation time must be finite and non-negative".into());
        }
        self.document.resolve(time_seconds);
        if self.document.has_pending_critical_resources() {
            return Ok(false);
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or("layout generation exhausted")?;
        Ok(true)
    }

    pub fn set_viewport(&mut self, viewport: Viewport) -> Result<(), String> {
        validate_viewport(&viewport)?;
        self.document.set_viewport(viewport);
        Ok(())
    }

    /// UI4 supplies frame-local pixels; native scrolling is a document projection.
    /// Blitz owns hit testing, ancestor :hover state and Stylo invalidation.
    pub fn pointer_move(&mut self, position: Option<[f32; 2]>, scroll_y: f32) -> bool {
        let (width, height) = self.document.viewport().logical_size();
        match position {
            Some([x, y])
                if x.is_finite()
                    && y.is_finite()
                    && x >= 0.0
                    && y >= 0.0
                    && x < width
                    && y < height =>
            {
                self.document.set_hover_to(x, y + scroll_y)
            }
            _ => self.document.clear_hover(),
        }
    }

    /// Artifact paths are import-time references (`root.1.0`, etc.), not live
    /// DOM addresses. Keep the returned versioned NodeId for subsequent edits.
    pub fn source_node(&self, path: &str) -> Option<NodeId> {
        self.source_nodes.get(path).copied()
    }

    pub fn source_url(&self) -> &url::Url {
        &self.source_url
    }

    /// Read the actual resolved document, including inline fragments and
    /// Parley glyph runs. The future Picasso painter consumes this same state.
    pub fn document(&self) -> &BaseDocument {
        &self.document
    }

    /// Native browser API seam. All changes pass through Blitz's invalidating
    /// mutation API; no parallel Solara cascade or layout implementation exists.
    pub fn mutate(&mut self) -> DocumentMutator<'_> {
        self.document.mutate()
    }

    /// Deliver a kernel-decoded image through Blitz's normal resource completion.
    pub fn load_image(&mut self, url: String, width: u32, height: u32, rgba: Arc<Vec<u8>>) {
        self.document
            .load_resource(blitz_dom::net::ResourceLoadResponse {
                request_id: usize::MAX,
                node_id: None,
                resolved_url: Some(url),
                result: Ok(blitz_dom::net::Resource::Image(
                    blitz_dom::util::ImageType::Image,
                    width,
                    height,
                    rgba,
                )),
            });
    }

    pub fn summary(&self) -> LayoutSummary {
        let mut summary = LayoutSummary {
            generation: self.generation,
            ..Default::default()
        };
        for (_, node) in self.document.tree().iter() {
            let Some(element) = node.element_data() else {
                continue;
            };
            if !node.flags.is_in_document() {
                continue;
            }
            let size = node.final_layout().size;
            if size.width > 0.0 && size.height > 0.0 {
                summary.boxes += 1;
            }
            if let Some(text) = &element.inline_layout_data {
                summary.text_layouts += 1;
                for line in text.layout.lines() {
                    for item in line.items() {
                        if let PositionedLayoutItem::GlyphRun(run) = item {
                            summary.glyphs += run.positioned_glyphs().count();
                        }
                    }
                }
            }
        }
        summary
    }
}

fn validate_viewport(viewport: &Viewport) -> Result<(), String> {
    let logical = viewport.logical_size();
    if viewport.window_size.0 == 0
        || viewport.window_size.1 == 0
        || !viewport.hidpi_scale.is_finite()
        || viewport.hidpi_scale <= 0.0
        || !viewport.zoom.is_finite()
        || viewport.zoom <= 0.0
        || !viewport.scale().is_finite()
        || viewport.scale() <= 0.0
        || !logical.0.is_finite()
        || !logical.1.is_finite()
    {
        return Err(
            "layout viewport requires non-zero dimensions and finite positive scales".into(),
        );
    }
    Ok(())
}

/// Deterministic font setup for the first layout proof, without system-font
/// discovery. Production pages should supply their own resolved font context.
pub fn bundled_font_context() -> FontContext {
    use linebender_resource_handle::Blob;
    use parley::fontique::{Collection, CollectionOptions, GenericFamily, SourceCache};
    let mut context = FontContext {
        source_cache: SourceCache::new_shared(),
        collection: Collection::new(CollectionOptions {
            shared: false,
            system_fonts: false,
        }),
    };
    let bytes: &'static [u8] = include_bytes!("../../assets/fonts/Inconsolata-Regular.ttf");
    let families = context
        .collection
        .register_fonts(Blob::new(Arc::new(bytes) as _), None);
    for generic in [
        GenericFamily::SansSerif,
        GenericFamily::Serif,
        GenericFamily::Monospace,
        GenericFamily::SystemUi,
    ] {
        context
            .collection
            .append_generic_families(generic, families.iter().map(|(id, _)| *id));
    }
    context
}
