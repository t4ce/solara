use rust_qjs_dom::StyleIndex;

use crate::gpu_ui::geometry::{BLOCK_GAP, CONTROL_H, PAGE_PAD, Rect, iframe_viewport};
use crate::gpu_ui::html::node::{
    ButtonType, ElementKind, HtmlNode, HtmlTag, InputType, inline_width,
};
use crate::gpu_ui::html::style;
use crate::gpu_ui::text;

#[derive(Clone, Copy)]
struct LayoutStyle {
    font_size: f32,
    line_height: f32,
}

impl LayoutStyle {
    fn for_node(style_index: &StyleIndex, node: &HtmlNode, inherited: Option<Self>) -> Self {
        let resolved = style::resolve(style_index, node);
        Self {
            font_size: resolved
                .font_size
                .or_else(|| inherited.map(|style| style.font_size))
                .unwrap_or(text::DEFAULT_FONT_SIZE),
            line_height: resolved
                .line_height
                .or_else(|| inherited.map(|style| style.line_height))
                .unwrap_or_else(|| text::metrics(text::DEFAULT_FONT_SIZE).natural_line_height()),
        }
    }
}

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

pub fn layout_document(nodes: &mut [HtmlNode], page_width: f32, style_index: &StyleIndex) {
    let mut ctx = LayoutContext::new(page_width);
    for node in nodes {
        layout_node(node, &mut ctx, style_index, None);
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

fn layout_node(
    node: &mut HtmlNode,
    ctx: &mut LayoutContext,
    style_index: &StyleIndex,
    inherited: Option<LayoutStyle>,
) {
    let text_style = LayoutStyle::for_node(style_index, node, inherited);
    node.bounds = match &mut node.kind {
        ElementKind::Element { tag, children } => {
            if tag.is_metadata() || matches!(tag, HtmlTag::Head) {
                Rect::new(PAGE_PAD, ctx.cursor_y, ctx.content_width(), 0.0)
            } else {
                layout_children(ctx, children, style_index, Some(text_style))
            }
        }
        ElementKind::Heading { .. } => ctx.place_block(text_style.line_height),
        ElementKind::Paragraph { inlines } => {
            let lines = (inline_width(inlines, text_style.font_size) / ctx.content_width())
                .ceil()
                .max(1.0);
            ctx.place_block(lines * text_style.line_height)
        }
        ElementKind::HorizontalRule => ctx.place_block(2.0),
        ElementKind::Link { .. } => ctx.place_block(text_style.line_height),
        ElementKind::OrderedList { items } | ElementKind::UnorderedList { items } => {
            ctx.place_block(items.len() as f32 * text_style.line_height)
        }
        ElementKind::Details {
            summary: _,
            summary_checkbox: _,
            children,
        } => layout_details(node.open, children, ctx, style_index, Some(text_style)),
        ElementKind::Div { children } | ElementKind::Form { children } => {
            layout_children(ctx, children, style_index, Some(text_style))
        }
        ElementKind::Label { text: _, control } => {
            layout_label(control, ctx, style_index, text_style)
        }
        ElementKind::Input {
            input_type, label, ..
        } => match input_type {
            InputType::Checkbox | InputType::Radio => ctx.place_block(text_style.line_height),
            _ => {
                if label.is_some() {
                    ctx.place_block(CONTROL_H + 18.0)
                } else {
                    ctx.place_block(CONTROL_H)
                }
            }
        },
        ElementKind::Select { .. } => ctx.place_block(CONTROL_H),
        ElementKind::Textarea { rows, .. } => {
            ctx.place_block(*rows as f32 * text_style.line_height + 8.0)
        }
        ElementKind::Button { label, button_type } => {
            let prefix = match button_type {
                ButtonType::Submit => "[submit] ",
                ButtonType::Reset => "[reset] ",
                ButtonType::Button => "",
            };
            let text = format!("{prefix}{label}");
            let w = (text.chars().count() as f32 * text::char_width(text_style.font_size) + 24.0)
                .clamp(80.0, ctx.content_width());
            let lines =
                text::wrapped_line_count_sized(&text, w - 16.0, text_style.font_size) as f32;
            let h = (lines * text_style.line_height + 12.0).max(CONTROL_H);
            let mut rect = ctx.place_block(h);
            rect.width = w;
            rect
        }
        ElementKind::Table { rows, .. } => {
            let row_h = 24.0;
            ctx.place_block(row_h * (rows.len() + 1) as f32 + 4.0)
        }
        ElementKind::Svg { height, .. } => ctx.place_block(*height + 8.0),
        ElementKind::Canvas { height, .. } => ctx.place_block(*height + 8.0),
        ElementKind::Iframe {
            width,
            height,
            children,
        } => layout_iframe(
            *width,
            *height,
            children,
            ctx,
            style_index,
            Some(text_style),
        ),
        ElementKind::Image { height, .. } => ctx.place_block(*height + 8.0),
        ElementKind::Dialog { children, floating } => {
            if *floating {
                layout_floating_dialog(children, ctx, style_index, Some(text_style))
            } else {
                layout_children(ctx, children, style_index, Some(text_style))
            }
        }
        ElementKind::Progress { .. } => ctx.place_block(24.0),
        ElementKind::Meter { .. } => ctx.place_block(24.0),
        ElementKind::Slider { .. } => ctx.place_block(28.0),
        ElementKind::Search { .. } => ctx.place_block(CONTROL_H + 18.0),
        ElementKind::Color => ctx.place_block(48.0),
        ElementKind::Footer { .. } => ctx.place_block(text_style.line_height + 14.0),
        ElementKind::PlainText { text } => {
            let lines =
                text::wrapped_line_count_sized(text, ctx.content_width(), text_style.font_size)
                    as f32;
            ctx.place_block(lines * text_style.line_height)
        }
    };
}

fn layout_label(
    control: &mut HtmlNode,
    ctx: &mut LayoutContext,
    style_index: &StyleIndex,
    text_style: LayoutStyle,
) -> Rect {
    let row_y = ctx.cursor_y;
    let row_h = CONTROL_H.max(text_style.line_height);
    let control_x = PAGE_PAD + 120.0;
    layout_control_in_row(
        control,
        control_x,
        row_y,
        row_h,
        ctx.content_width(),
        style_index,
        text_style,
    );
    let rect = Rect::new(PAGE_PAD, row_y, ctx.content_width(), row_h);
    ctx.cursor_y = row_y + row_h + BLOCK_GAP;
    rect
}

fn layout_control_in_row(
    node: &mut HtmlNode,
    x: f32,
    y: f32,
    row_h: f32,
    content_width: f32,
    style_index: &StyleIndex,
    inherited: LayoutStyle,
) {
    let text_style = LayoutStyle::for_node(style_index, node, Some(inherited));
    let max_w = (content_width - (x - PAGE_PAD)).max(80.0);
    match &mut node.kind {
        ElementKind::Input { input_type, .. } => match input_type {
            InputType::Checkbox | InputType::Radio => {
                node.bounds = Rect::new(x, y + 2.0, max_w, text_style.line_height);
            }
            _ => {
                node.bounds = Rect::new(
                    x,
                    y + (row_h - CONTROL_H) * 0.5,
                    max_w.min(280.0),
                    CONTROL_H,
                );
            }
        },
        ElementKind::Button { label, .. } => {
            let text_w =
                label.chars().count() as f32 * text::char_width(text_style.font_size) + 24.0;
            let w = text_w.clamp(80.0, max_w);
            node.bounds = Rect::new(x, y + (row_h - CONTROL_H) * 0.5, w, CONTROL_H);
        }
        ElementKind::Select { .. } => {
            node.bounds = Rect::new(
                x,
                y + (row_h - CONTROL_H) * 0.5,
                max_w.min(200.0),
                CONTROL_H,
            );
        }
        _ => {
            node.bounds = Rect::new(x, y, max_w, row_h);
        }
    }
}

fn layout_floating_dialog(
    children: &mut [HtmlNode],
    ctx: &mut LayoutContext,
    style_index: &StyleIndex,
    inherited: Option<LayoutStyle>,
) -> Rect {
    let w = 360.0_f32.min(ctx.content_width());
    let x = (ctx.page_width - w) * 0.5;
    let y = ctx.cursor_y;
    let h = 180.0;
    let rect = Rect::new(x, y, w, h);

    let mut child_ctx = LayoutContext {
        page_width: w - 16.0 + PAGE_PAD * 2.0,
        cursor_y: y + 28.0,
    };
    for child in children.iter_mut() {
        layout_node(child, &mut child_ctx, style_index, inherited);
        shift_bounds_tree(child, x + 8.0 - PAGE_PAD, 0.0);
    }

    ctx.cursor_y = rect.bottom() + BLOCK_GAP;
    rect
}

fn layout_iframe(
    requested_width: f32,
    requested_height: f32,
    children: &mut [HtmlNode],
    ctx: &mut LayoutContext,
    style_index: &StyleIndex,
    inherited: Option<LayoutStyle>,
) -> Rect {
    let width = requested_width.max(1.0).min(ctx.content_width());
    let height = requested_height.max(1.0);
    let mut frame = ctx.place_block(height);
    frame.width = width;
    let viewport = iframe_viewport(frame);

    let mut child_ctx = LayoutContext::new(viewport.width + PAGE_PAD * 2.0);
    for child in children.iter_mut() {
        layout_node(child, &mut child_ctx, style_index, inherited);
        shift_bounds_tree(child, viewport.x - PAGE_PAD, viewport.y - PAGE_PAD);
    }
    frame
}

fn shift_bounds_tree(node: &mut HtmlNode, dx: f32, dy: f32) {
    node.bounds.x += dx;
    node.bounds.y += dy;
    match &mut node.kind {
        ElementKind::Element { children, .. }
        | ElementKind::Details { children, .. }
        | ElementKind::Div { children }
        | ElementKind::Form { children }
        | ElementKind::Iframe { children, .. }
        | ElementKind::Dialog { children, .. } => {
            for child in children.iter_mut() {
                shift_bounds_tree(child, dx, dy);
            }
        }
        ElementKind::Label { control, .. } => shift_bounds_tree(control, dx, dy),
        _ => {}
    }
}

fn layout_details(
    open: bool,
    children: &mut [HtmlNode],
    ctx: &mut LayoutContext,
    style_index: &StyleIndex,
    inherited: Option<LayoutStyle>,
) -> Rect {
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
            layout_node(child, &mut child_ctx, style_index, inherited);
        }
        height = child_ctx.cursor_y - y;
        ctx.cursor_y = child_ctx.cursor_y + BLOCK_GAP;
    } else {
        ctx.cursor_y = y + CONTROL_H + BLOCK_GAP;
    }

    Rect::new(x, y, w, height)
}

fn layout_children(
    ctx: &mut LayoutContext,
    children: &mut [HtmlNode],
    style_index: &StyleIndex,
    inherited: Option<LayoutStyle>,
) -> Rect {
    let start_y = ctx.cursor_y;
    for child in children.iter_mut() {
        layout_node(child, ctx, style_index, inherited);
    }
    Rect::new(
        PAGE_PAD,
        start_y,
        ctx.content_width(),
        ctx.cursor_y - start_y,
    )
}

pub fn hit_test_details_summary(node: &HtmlNode, x: f32, y: f32) -> bool {
    if !matches!(node.kind, ElementKind::Details { .. }) {
        return false;
    }
    let summary_rect = Rect::new(node.bounds.x, node.bounds.y, node.bounds.width, CONTROL_H);
    summary_rect.contains(x, y)
}

#[cfg(test)]
mod tests {
    use super::layout_document;
    use crate::gpu_ui::geometry::iframe_viewport;
    use crate::gpu_ui::html::node::{ElementKind, HtmlNode};

    #[test]
    fn iframe_establishes_a_fixed_containing_viewport() {
        let mut engine = rust_qjs_dom::DomEngine::new().expect("DOM engine starts");
        let artifact = engine
            .parse("<main></main>", "https://solara.test/")
            .expect("style index is generated");
        let dialog = HtmlNode::new(
            2,
            ElementKind::Dialog {
                children: Vec::new(),
                floating: true,
            },
        );
        let iframe = HtmlNode::new(
            1,
            ElementKind::Iframe {
                width: 300.0,
                height: 220.0,
                children: vec![dialog],
            },
        );
        let mut nodes = vec![iframe];

        layout_document(&mut nodes, 960.0, &artifact.style_index);

        let frame = nodes[0].bounds;
        assert_eq!((frame.width, frame.height), (300.0, 220.0));
        let viewport = iframe_viewport(frame);
        let ElementKind::Iframe { children, .. } = &nodes[0].kind else {
            panic!("expected iframe");
        };
        let dialog = children[0].bounds;
        assert!(dialog.x >= viewport.x);
        assert!(dialog.y >= viewport.y);
        assert!(dialog.right() <= viewport.right());
        assert!(dialog.bottom() <= viewport.bottom());
    }
}
