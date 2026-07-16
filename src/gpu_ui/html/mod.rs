mod layout;
mod node;
mod paint;
mod parser;
mod style;

pub use node::HtmlNode;

use layout::{document_height, hit_test_details_summary, layout_document};
use paint::paint_document;
use parser::parse_html;
use rust_qjs_dom::{DomArtifact, DomEngine, JsEngine};

#[derive(Default)]
pub struct RenderBatch {
    pub shapes: Vec<crate::gpu_ui::shapes::ShapeInstance>,
    pub text: crate::gpu_ui::text::TextBatch,
}

impl RenderBatch {
    pub fn clear(&mut self) {
        self.shapes.clear();
        self.text.clear();
    }
}

pub struct Document {
    pub nodes: Vec<HtmlNode>,
    pub scroll_y: f32,
    pub content_height: f32,
    dom: DomArtifact,
    dom_engine: DomEngine,
    page_width: f32,
}

impl Document {
    pub fn from_dom(
        dom: DomArtifact,
        mut dom_engine: DomEngine,
        page_width: f32,
    ) -> Result<Self, String> {
        let mut nodes = parse_html(&dom, &mut dom_engine)?;
        layout_document(&mut nodes, page_width, &dom.style_index);
        let content_height = document_height(&nodes);
        let mut document = Self {
            nodes,
            scroll_y: 0.0,
            content_height,
            dom,
            dom_engine,
            page_width,
        };
        let bootstrap = format!(
            "globalThis.__solara = Object.freeze({{ domArtifactVersion: {} }});",
            document.dom().schema_version
        );
        document
            .js_mut()
            .eval_void(&bootstrap, "<solara-bootstrap>")
            .map_err(|error| format!("failed to initialize Solara's JavaScript host: {error}"))?;
        Ok(document)
    }

    pub fn dom(&self) -> &DomArtifact {
        &self.dom
    }

    /// Returns the same QuickJS runtime that produced this document's Parse5 DOM.
    pub fn js_mut(&mut self) -> &mut JsEngine {
        self.dom_engine.js_mut()
    }

    pub fn relayout(&mut self, page_width: f32) {
        self.page_width = page_width;
        layout_document(&mut self.nodes, page_width, &self.dom.style_index);
        self.content_height = document_height(&self.nodes);
    }

    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll_y = (self.scroll_y - delta).max(0.0);
    }

    pub fn clamp_scroll_to(&mut self, viewport_height: f32) {
        let max_scroll = (self.content_height - viewport_height).max(0.0);
        if self.scroll_y > max_scroll {
            self.scroll_y = max_scroll;
        }
    }

    pub fn toggle_details_at(&mut self, x: f32, y: f32) -> bool {
        if !toggle_details_recursive(&mut self.nodes, x, y + self.scroll_y) {
            return false;
        }
        layout_document(&mut self.nodes, self.page_width, &self.dom.style_index);
        self.content_height = document_height(&self.nodes);
        true
    }
}

fn toggle_details_recursive(nodes: &mut [HtmlNode], x: f32, y: f32) -> bool {
    for node in nodes.iter_mut() {
        if hit_test_details_summary(node, x, y) {
            node.open = !node.open;
            return true;
        }
        if toggle_details_in_children(node, x, y) {
            return true;
        }
    }
    false
}

fn toggle_details_in_children(node: &mut HtmlNode, x: f32, y: f32) -> bool {
    match &mut node.kind {
        node::ElementKind::Details { children, .. } if node.open => {
            toggle_details_recursive(children, x, y)
        }
        node::ElementKind::Element { children, .. }
        | node::ElementKind::Div { children }
        | node::ElementKind::Form { children }
        | node::ElementKind::Iframe { children, .. }
        | node::ElementKind::Dialog { children, .. } => toggle_details_recursive(children, x, y),
        node::ElementKind::Label { control, .. } => {
            toggle_details_recursive(std::slice::from_mut(control), x, y)
        }
        _ => false,
    }
}

pub fn collect_batch(document: &Document, scale: f32, batch: &mut RenderBatch) {
    batch.clear();
    paint_document(
        &document.nodes,
        document.scroll_y,
        &document.dom.style_index,
        &mut batch.shapes,
        &mut batch.text,
    );
    crate::gpu_ui::shapes::scale_shape_instances(&mut batch.shapes, scale);
    crate::gpu_ui::text::scale_text_batch(&mut batch.text, scale);
}

#[cfg(test)]
mod parity_baseline {
    use rust_qjs_dom::DomEngine;

    use super::{Document, HtmlNode, RenderBatch, collect_batch};
    use crate::gpu_ui::html::node::ElementKind;

    fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x100000001b3);
        }
    }

    fn find_by_html_id<'a>(nodes: &'a [HtmlNode], id: &str) -> Option<&'a HtmlNode> {
        for node in nodes {
            if node.id_attr.as_deref() == Some(id) {
                return Some(node);
            }
            let found = match &node.kind {
                ElementKind::Element { children, .. }
                | ElementKind::Details { children, .. }
                | ElementKind::Div { children }
                | ElementKind::Form { children }
                | ElementKind::Iframe { children, .. }
                | ElementKind::Dialog { children, .. } => find_by_html_id(children, id),
                ElementKind::Label { control, .. } => {
                    find_by_html_id(std::slice::from_ref(control), id)
                }
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }

    #[test]
    fn current_demo_keeps_its_nested_frame_render_digest() {
        let page = crate::gpu_ui::loader::load_page(None).expect("load current demo");
        assert_eq!(page.title, "HTML Only Visual Elements");
        let mut document = Document::from_dom(page.artifact, page.dom_engine, 960.0)
            .expect("adapt current demo DOM");
        assert_eq!(document.dom().schema_version, 2);
        assert_eq!(
            document
                .js_mut()
                .eval_json("21 * 2", "<solara-parity>")
                .expect("retained QuickJS runtime evaluates JavaScript")
                .as_i64(),
            Some(42)
        );
        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        let mut hash = 0xcbf29ce484222325_u64;
        for shape in &batch.shapes {
            for value in shape.pos_size.into_iter().chain(shape.color) {
                hash_bytes(&mut hash, &value.to_bits().to_le_bytes());
            }
            hash_bytes(&mut hash, &shape.shape_type.to_le_bytes());
        }
        for section in &batch.text.sections {
            for value in [
                section.x,
                section.y,
                section.width,
                section.height,
                section.font_size,
            ] {
                hash_bytes(&mut hash, &value.to_bits().to_le_bytes());
            }
            for value in section.color {
                hash_bytes(&mut hash, &value.to_bits().to_le_bytes());
            }
            hash_bytes(&mut hash, section.text.as_bytes());
        }
        assert_eq!(batch.shapes.len(), 141);
        assert_eq!(batch.text.sections.len(), 83);
        assert_eq!(document.content_height.to_bits(), 0x455e8000);
        assert_eq!(hash, 0x233ebbe76e0dc804);
    }

    #[test]
    fn authored_lightning_css_reaches_the_active_paint_batch() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                "<style>main { color: #123456 }</style><main>Styled by Lightning CSS</main>",
                "https://solara.test/",
            )
            .expect("document parses");
        let document = Document::from_dom(artifact, engine, 960.0).expect("document adapts");
        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        let styled = batch
            .text
            .sections
            .iter()
            .find(|section| section.text.contains("Styled by Lightning CSS"))
            .expect("styled text section");
        assert_eq!(
            styled.color,
            [
                0x12 as f32 / 255.0,
                0x34 as f32 / 255.0,
                0x56 as f32 / 255.0,
                1.0
            ]
        );
    }

    #[test]
    fn mixed_native_css_font_sizes_reach_layout_scene_and_glyph_scale() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                r#"
                    <style>
                      #small { font-size: 12px; }
                      #large { font-size: 30px; line-height: 42px; }
                      #wrapped { font-size: 20px; line-height: 36px; }
                    </style>
                    <main>
                      <p id="small">small scene text</p>
                      <p id="large">large scene text</p>
                      <p id="wrapped">abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789</p>
                      <h2 id="native-heading">native heading text</h2>
                    </main>
                "#,
                "https://solara.test/typography",
            )
            .expect("document parses");
        let document = Document::from_dom(artifact, engine, 320.0).expect("document adapts");
        let small_node = find_by_html_id(&document.nodes, "small").expect("small layout node");
        let large_node = find_by_html_id(&document.nodes, "large").expect("large layout node");
        let wrapped_node =
            find_by_html_id(&document.nodes, "wrapped").expect("wrapped layout node");
        assert!(large_node.bounds.height > small_node.bounds.height);
        assert!(wrapped_node.bounds.height >= 72.0);

        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        let section = |needle: &str| {
            batch
                .text
                .sections
                .iter()
                .find(|section| section.text == needle)
                .unwrap_or_else(|| panic!("text section for {needle}"))
        };
        let small = section("small scene text");
        let large = section("large scene text");
        let heading = section("native heading text");

        assert_eq!(small.font_size, 12.0);
        assert_eq!(large.font_size, 30.0);
        assert_eq!(heading.font_size, 22.0);
        assert!(large.height >= 42.0);
        assert_eq!(
            solara_wgpu_shim::TextRun::scale(large),
            solara_wgpu_shim::font_metrics(30.0).glyph_scale
        );
        assert!(solara_wgpu_shim::TextRun::scale(large) > solara_wgpu_shim::TextRun::scale(small));

        let wrapped = batch
            .text
            .sections
            .iter()
            .filter(|section| section.font_size == 20.0)
            .collect::<Vec<_>>();
        assert!(wrapped.len() >= 2);
        assert!((wrapped[1].y - wrapped[0].y - 36.0).abs() < 0.001);
    }
}
