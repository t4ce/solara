use crate::gpu_ui::geometry::{Rect, BLOCK_GAP, CONTROL_H, PAGE_PAD, TEXT_LINE};
use crate::gpu_ui::html::node::{inline_width, ElementKind, HtmlNode, InputType};

pub struct LayoutContext {
    pub page_width: f32,
    pub cursor_y: f32,
}

impl LayoutContext {
    pub fn new(page_width: f32) -> Self {
        Self {
            page_width,
            cursor_y: PAGE_PAD,
        }
    }

    pub fn content_width(&self) -> f32 {
        self.page_width - PAGE_PAD * 2.0
    }

    fn place_block(&mut self, height: f32) -> Rect {
        let rect = Rect::new(PAGE_PAD, self.cursor_y, self.content_width(), height);
        self.cursor_y += height + BLOCK_GAP;
        rect
    }
}

pub fn layout_document(nodes: &mut [HtmlNode], page_width: f32) {
    let mut ctx = LayoutContext::new(page_width);
    for node in nodes {
        layout_node(node, &mut ctx);
    }
}

pub fn document_height(nodes: &[HtmlNode]) -> f32 {
    nodes
        .iter()
        .map(|node| node.bounds.bottom() + BLOCK_GAP)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(PAGE_PAD)
        + PAGE_PAD
}

fn layout_node(node: &mut HtmlNode, ctx: &mut LayoutContext) {
    node.bounds = match &mut node.kind {
        ElementKind::Heading { level, .. } => {
            let h = if *level == 1 { 32.0 } else { 24.0 };
            ctx.place_block(h)
        }
        ElementKind::Paragraph { inlines } => {
            let lines = (inline_width(inlines) / ctx.content_width())
                .ceil()
                .max(1.0);
            ctx.place_block(lines * TEXT_LINE)
        }
        ElementKind::HorizontalRule => ctx.place_block(2.0),
        ElementKind::Link { .. } => ctx.place_block(TEXT_LINE),
        ElementKind::OrderedList { items } | ElementKind::UnorderedList { items } => {
            ctx.place_block(items.len() as f32 * TEXT_LINE)
        }
        ElementKind::Details {
            summary: _,
            summary_checkbox: _,
            children,
        } => layout_details(node.open, children, ctx),
        ElementKind::Div { children } | ElementKind::Form { children } => {
            layout_children(ctx, children)
        }
        ElementKind::Label { text: _, control } => {
            layout_node(control, ctx);
            let h = control.bounds.height.max(TEXT_LINE);
            let rect = ctx.place_block(h);
            control.bounds.x = rect.x + 120.0;
            control.bounds.y = rect.y + (h - control.bounds.height) * 0.5;
            rect
        }
        ElementKind::Input {
            input_type,
            label,
            ..
        } => match input_type {
            InputType::Checkbox | InputType::Radio => ctx.place_block(TEXT_LINE),
            _ => {
                if label.is_some() {
                    ctx.place_block(CONTROL_H + 4.0)
                } else {
                    ctx.place_block(CONTROL_H)
                }
            }
        },
        ElementKind::Select { .. } => ctx.place_block(CONTROL_H),
        ElementKind::Textarea { rows, .. } => ctx.place_block(*rows as f32 * TEXT_LINE + 8.0),
        ElementKind::Button { label, .. } => {
            let w = (label.len() as f32 * 4.5).clamp(80.0, ctx.content_width());
            let mut rect = ctx.place_block(CONTROL_H);
            rect.width = w;
            rect
        }
        ElementKind::Table { rows, .. } => {
            let row_h = 24.0;
            ctx.place_block(row_h * (rows.len() + 1) as f32 + 4.0)
        }
        ElementKind::Svg { height, .. } => ctx.place_block(*height + 8.0),
        ElementKind::Canvas { height, .. } => ctx.place_block(*height + 8.0),
        ElementKind::Iframe { children } => layout_children(ctx, children),
        ElementKind::Image { height, .. } => ctx.place_block(*height + 8.0),
        ElementKind::Dialog { children, floating } => {
            if *floating {
                Rect::new((ctx.page_width - 360.0) * 0.5, 120.0, 360.0, 180.0)
            } else {
                layout_children(ctx, children)
            }
        }
        ElementKind::Progress { .. } => ctx.place_block(24.0),
        ElementKind::Meter { .. } => ctx.place_block(24.0),
        ElementKind::Slider { .. } => ctx.place_block(28.0),
        ElementKind::Search { .. } => ctx.place_block(CONTROL_H + 14.0),
        ElementKind::Color => ctx.place_block(48.0),
        ElementKind::Footer { .. } => ctx.place_block(32.0),
        ElementKind::PlainText { text } => {
            let lines = ((text.len() as f32 * 4.5) / ctx.content_width()).ceil().max(1.0);
            ctx.place_block(lines * TEXT_LINE)
        }
    };
}

fn layout_details(open: bool, children: &mut [HtmlNode], ctx: &mut LayoutContext) -> Rect {
    let x = PAGE_PAD;
    let y = ctx.cursor_y;
    let w = ctx.content_width();
    let mut height = CONTROL_H;

    if open {
        let mut child_ctx = LayoutContext {
            page_width: ctx.page_width,
            cursor_y: y + CONTROL_H,
        };
        for child in children.iter_mut() {
            layout_node(child, &mut child_ctx);
        }
        height = child_ctx.cursor_y - y;
        ctx.cursor_y = child_ctx.cursor_y + BLOCK_GAP;
    } else {
        ctx.cursor_y = y + CONTROL_H + BLOCK_GAP;
    }

    Rect::new(x, y, w, height)
}

fn layout_children(ctx: &mut LayoutContext, children: &mut [HtmlNode]) -> Rect {
    let start_y = ctx.cursor_y;
    for child in children.iter_mut() {
        layout_node(child, ctx);
    }
    Rect::new(PAGE_PAD, start_y, ctx.content_width(), ctx.cursor_y - start_y)
}

pub fn hit_test_details_summary(node: &HtmlNode, x: f32, y: f32) -> bool {
    if !matches!(node.kind, ElementKind::Details { .. }) {
        return false;
    }
    let summary_rect = Rect::new(node.bounds.x, node.bounds.y, node.bounds.width, CONTROL_H);
    summary_rect.contains(x, y)
}
