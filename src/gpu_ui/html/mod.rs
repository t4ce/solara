mod demoui;
mod layout;
mod node;
mod paint;

use demoui::build_demoui_document;
use layout::{document_height, hit_test_details_summary, layout_document};
use node::HtmlNode;
use paint::paint_document;

pub struct Document {
    pub nodes: Vec<HtmlNode>,
    pub scroll_y: f32,
    pub content_height: f32,
    page_width: f32,
}

impl Document {
    pub fn new(page_width: f32) -> Self {
        let mut nodes = build_demoui_document();
        layout_document(&mut nodes, page_width);
        let content_height = document_height(&nodes);
        Self {
            nodes,
            scroll_y: 0.0,
            content_height,
            page_width,
        }
    }

    pub fn relayout(&mut self, page_width: f32) {
        self.page_width = page_width;
        layout_document(&mut self.nodes, page_width);
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
        layout_document(&mut self.nodes, self.page_width);
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
        node::ElementKind::Div { children }
        | node::ElementKind::Form { children }
        | node::ElementKind::Iframe { children }
        | node::ElementKind::Dialog { children, .. } => toggle_details_recursive(children, x, y),
        node::ElementKind::Label { control, .. } => {
            toggle_details_recursive(std::slice::from_mut(control), x, y)
        }
        _ => false,
    }
}

pub fn collect_instances(
    document: &Document,
    scale: f32,
    out: &mut Vec<crate::gpu_ui::shapes::ShapeInstance>,
) {
    out.clear();
    paint_document(&document.nodes, document.scroll_y, out);
    crate::gpu_ui::shapes::scale_shape_instances(out, scale);
}
