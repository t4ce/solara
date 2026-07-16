#![allow(clippy::items_after_test_module, clippy::too_many_arguments)]

use rust_qjs_dom::StyleIndex;

use crate::gpu_ui::geometry::{CONTROL_H, Rect, TEXT_LINE, iframe_viewport};
use crate::gpu_ui::html::node::{ButtonType, ElementKind, HtmlNode, Inline, InputType, SvgChild};
use crate::gpu_ui::html::style::{self, ResolvedStyle};
use crate::gpu_ui::shapes::ShapeInstance;
use crate::gpu_ui::text::{self, TextBatch};

#[derive(Clone, Copy)]
struct PaintStyle {
    text: [f32; 4],
    background: Option<[f32; 4]>,
    font_scale: f32,
    border_color: [f32; 4],
    border_width: f32,
    border_visible: bool,
}

impl PaintStyle {
    fn from_theme(theme: &Theme, resolved: &ResolvedStyle, inherited: Option<PaintStyle>) -> Self {
        Self {
            text: resolved
                .color
                .or_else(|| inherited.map(|style| style.text))
                .unwrap_or(theme.text),
            background: resolved.background_color,
            font_scale: resolved
                .font_size
                .or_else(|| inherited.map(|style| style.font_scale))
                .unwrap_or(text::FONT_SCALE),
            border_color: resolved.border_color.unwrap_or(theme.border),
            border_width: resolved.border_width.unwrap_or(1.0),
            border_visible: resolved.border_color.is_some() || resolved.border_width.is_some(),
        }
    }

    fn fill_background(&self, shapes: &mut Vec<ShapeInstance>, bounds: Rect) {
        if let Some(bg) = self.background {
            fill_rect(shapes, bounds, bg);
        }
    }

    fn queue(
        &self,
        text_out: &mut TextBatch,
        x: f32,
        y: f32,
        value: &str,
        color: Option<[f32; 4]>,
    ) {
        text::queue_left_scaled(
            text_out,
            x,
            y,
            value,
            color.unwrap_or(self.text),
            self.font_scale,
        );
    }

    fn queue_wrapped(
        &self,
        text_out: &mut TextBatch,
        x: f32,
        y: f32,
        value: &str,
        max_width: f32,
        max_height: f32,
        color: Option<[f32; 4]>,
    ) {
        let section_color = color.unwrap_or(self.text);
        text::queue_wrapped_scaled(
            text_out,
            x,
            y,
            value,
            max_width,
            max_height,
            section_color,
            self.font_scale,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{PaintStyle, Theme, clip_shapes, clip_text};
    use crate::gpu_ui::geometry::Rect;
    use crate::gpu_ui::html::style::ResolvedStyle;
    use crate::gpu_ui::shapes::ShapeInstance;
    use crate::gpu_ui::text::{TextBatch, TextSection};

    #[test]
    fn text_properties_inherit_without_inheriting_background() {
        let theme = Theme::default();
        let parent = PaintStyle::from_theme(
            &theme,
            &ResolvedStyle {
                color: Some([0.1, 0.2, 0.3, 1.0]),
                background_color: Some([0.9, 0.9, 0.9, 1.0]),
                font_size: Some(22.0),
                ..ResolvedStyle::default()
            },
            None,
        );
        let child = PaintStyle::from_theme(&theme, &ResolvedStyle::default(), Some(parent));

        assert_eq!(child.text, parent.text);
        assert_eq!(child.font_scale, parent.font_scale);
        assert_eq!(child.background, None);
    }

    #[test]
    fn iframe_clip_trims_descendant_paint_to_its_viewport() {
        let clip = Rect::new(20.0, 30.0, 100.0, 80.0);
        let mut shapes = vec![ShapeInstance::rect(
            Rect::new(100.0, 90.0, 50.0, 50.0),
            [1.0; 4],
        )];
        clip_shapes(&mut shapes, clip);
        assert_eq!(shapes[0].pos_size, [100.0, 90.0, 20.0, 20.0]);

        let mut text = TextBatch {
            sections: vec![TextSection {
                x: 100.0,
                y: 90.0,
                width: 50.0,
                height: 30.0,
                text: "clipped".to_string(),
                color: [1.0; 4],
                scale: 14.0,
            }],
        };
        clip_text(&mut text, clip);
        assert_eq!(text.sections[0].width, 20.0);
        assert_eq!(text.sections[0].height, 20.0);
    }
}

#[allow(dead_code)]
pub struct Theme {
    pub page: [f32; 4],
    pub text: [f32; 4],
    pub link: [f32; 4],
    pub border: [f32; 4],
    pub control_bg: [f32; 4],
    pub button_bg: [f32; 4],
    pub table_header: [f32; 4],
    pub accent: [f32; 4],
    pub iframe_bg: [f32; 4],
    pub dialog_bg: [f32; 4],
    pub img_fill: [f32; 4],
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            page: [0.98, 0.98, 0.98, 1.0],
            text: [0.12, 0.12, 0.12, 1.0],
            link: [0.0, 0.0, 0.75, 1.0],
            border: [0.55, 0.55, 0.55, 1.0],
            control_bg: [1.0, 1.0, 1.0, 1.0],
            button_bg: [0.9, 0.9, 0.9, 1.0],
            table_header: [0.88, 0.88, 0.88, 1.0],
            accent: [0.18, 0.42, 0.86, 1.0],
            iframe_bg: [0.95, 0.97, 1.0, 1.0],
            dialog_bg: [1.0, 1.0, 0.95, 1.0],
            img_fill: [0.85, 0.15, 0.15, 1.0],
        }
    }
}

pub fn paint_document(
    nodes: &[HtmlNode],
    scroll_y: f32,
    style_index: &StyleIndex,
    shapes: &mut Vec<ShapeInstance>,
    text_out: &mut TextBatch,
) {
    let theme = Theme::default();
    for node in nodes {
        paint_node(node, scroll_y, style_index, &theme, None, shapes, text_out);
    }
}

fn paint_node(
    node: &HtmlNode,
    scroll_y: f32,
    style_index: &StyleIndex,
    theme: &Theme,
    inherited: Option<PaintStyle>,
    shapes: &mut Vec<ShapeInstance>,
    text_out: &mut TextBatch,
) {
    let style = PaintStyle::from_theme(theme, &style::resolve(style_index, node), inherited);
    let bounds = offset_rect(node.bounds, 0.0, -scroll_y);
    if bounds.bottom() < 0.0 || bounds.y > 2000.0 {
        // coarse cull for off-screen blocks
    }

    match &node.kind {
        ElementKind::Element { children, .. } => {
            style.fill_background(shapes, bounds);
            if style.border_visible {
                stroke_rect(shapes, bounds, style.border_color, style.border_width);
            }
            for child in children {
                paint_node(
                    child,
                    scroll_y,
                    style_index,
                    theme,
                    Some(style),
                    shapes,
                    text_out,
                );
            }
        }
        ElementKind::Heading { level, text } => {
            style.queue(text_out, bounds.x, bounds.y + 4.0, text, None);
            let _ = level;
        }
        ElementKind::Paragraph { inlines } => paint_inlines(text_out, bounds, inlines, &style),
        ElementKind::HorizontalRule => {
            stroke_rect(shapes, bounds, theme.border, 1.0);
        }
        ElementKind::Link { text, .. } => {
            style.queue(text_out, bounds.x, bounds.y + 2.0, text, None);
            fill_rect(
                shapes,
                Rect::new(
                    bounds.x,
                    bounds.bottom() - 2.0,
                    text.chars().count() as f32 * text::char_width(style.font_scale),
                    1.0,
                ),
                style.text,
            );
        }
        ElementKind::OrderedList { items } => {
            for (i, item) in items.iter().enumerate() {
                let y = bounds.y + i as f32 * TEXT_LINE;
                text::queue_left(text_out, bounds.x, y, &format!("{}.", i + 1), theme.text);
                text::queue_left(text_out, bounds.x + 24.0, y, item, theme.text);
            }
        }
        ElementKind::UnorderedList { items } => {
            for (i, item) in items.iter().enumerate() {
                let y = bounds.y + i as f32 * TEXT_LINE;
                fill_rect(
                    shapes,
                    Rect::new(bounds.x + 4.0, y + 6.0, 4.0, 4.0),
                    theme.text,
                );
                text::queue_left(text_out, bounds.x + 16.0, y, item, theme.text);
            }
        }
        ElementKind::Details {
            summary,
            summary_checkbox,
            children,
        } => {
            style.fill_background(shapes, bounds);
            paint_details_summary(
                shapes,
                text_out,
                bounds,
                summary,
                *summary_checkbox,
                node.open,
                &style,
                theme,
            );
            if node.open {
                let inner = Rect::new(
                    bounds.x + 12.0,
                    bounds.y + CONTROL_H,
                    bounds.width - 12.0,
                    bounds.height - CONTROL_H,
                );
                stroke_rect(shapes, inner, style.border_color, style.border_width);
                for child in children {
                    paint_node(
                        child,
                        scroll_y,
                        style_index,
                        theme,
                        Some(style),
                        shapes,
                        text_out,
                    );
                }
            }
        }
        ElementKind::Div { children } | ElementKind::Form { children } => {
            for child in children {
                paint_node(
                    child,
                    scroll_y,
                    style_index,
                    theme,
                    Some(style),
                    shapes,
                    text_out,
                );
            }
        }
        ElementKind::Label { text, control } => {
            text::queue_left(text_out, bounds.x, bounds.y + 4.0, text, theme.text);
            paint_node(
                control,
                scroll_y,
                style_index,
                theme,
                Some(style),
                shapes,
                text_out,
            );
        }
        ElementKind::Input {
            input_type,
            value,
            checked,
            label,
            ..
        } => paint_input(
            shapes,
            text_out,
            bounds,
            input_type,
            value,
            *checked,
            label.as_deref(),
            theme,
        ),
        ElementKind::Select { options, selected } => {
            paint_select(shapes, text_out, bounds, options, *selected, theme);
        }
        ElementKind::Textarea { value, rows, .. } => {
            paint_textarea(shapes, text_out, bounds, value, *rows, theme);
        }
        ElementKind::Button { label, button_type } => {
            paint_button(shapes, text_out, bounds, label, button_type, &style);
        }
        ElementKind::Table { headers, rows } => {
            paint_table(shapes, text_out, bounds, headers, rows, theme);
        }
        ElementKind::Svg {
            width,
            height,
            children,
        } => {
            paint_svg(shapes, bounds, *width, *height, children, theme);
        }
        ElementKind::Canvas { width, height } => {
            stroke_rect(shapes, bounds, theme.border, 1.0);
            fill_rect(shapes, bounds.inset(1.0), [0.92, 0.92, 0.92, 1.0]);
            text::queue_left(
                text_out,
                bounds.x + 8.0,
                bounds.y + 8.0,
                &format!("canvas {}x{}", *width as u32, *height as u32),
                theme.text,
            );
        }
        ElementKind::Iframe { children, .. } => {
            fill_rect(shapes, bounds, theme.iframe_bg);
            stroke_rect(shapes, bounds, theme.border, 2.0);
            text::queue_left(
                text_out,
                bounds.x + 8.0,
                bounds.y + 4.0,
                "iframe",
                theme.text,
            );
            let viewport = iframe_viewport(bounds);
            let mut child_shapes = Vec::new();
            let mut child_text = TextBatch::default();
            for child in children {
                paint_node(
                    child,
                    scroll_y,
                    style_index,
                    theme,
                    Some(style),
                    &mut child_shapes,
                    &mut child_text,
                );
            }
            clip_shapes(&mut child_shapes, viewport);
            clip_text(&mut child_text, viewport);
            shapes.extend(child_shapes);
            text_out.sections.extend(child_text.sections);
        }
        ElementKind::Image { alt, .. } => {
            fill_rect(shapes, bounds, theme.img_fill);
            stroke_rect(shapes, bounds, theme.border, 1.0);
            text::queue_left(
                text_out,
                bounds.x + 8.0,
                bounds.y + bounds.height * 0.5 - 4.0,
                alt,
                [1.0, 1.0, 1.0, 1.0],
            );
        }
        ElementKind::Dialog { children, floating } => {
            fill_rect(shapes, bounds, theme.dialog_bg);
            stroke_rect(
                shapes,
                bounds,
                theme.border,
                if *floating { 2.0 } else { 1.0 },
            );
            if *floating {
                text::queue_left(
                    text_out,
                    bounds.x + 8.0,
                    bounds.y + 4.0,
                    "dialog",
                    theme.text,
                );
            }
            for child in children {
                paint_node(
                    child,
                    scroll_y,
                    style_index,
                    theme,
                    Some(style),
                    shapes,
                    text_out,
                );
            }
        }
        ElementKind::Progress { value, max } => {
            text::queue_left(text_out, bounds.x, bounds.y, "progress", theme.text);
            let bar = Rect::new(bounds.x, bounds.y + 14.0, bounds.width, 10.0);
            fill_rect(shapes, bar, [0.85, 0.85, 0.85, 1.0]);
            let fill_w = bar.width * (value / max).clamp(0.0, 1.0);
            fill_rect(
                shapes,
                Rect::new(bar.x, bar.y, fill_w, bar.height),
                theme.accent,
            );
        }
        ElementKind::Meter { value, label } => {
            text::queue_left(text_out, bounds.x, bounds.y, "meter", theme.text);
            let bar = Rect::new(bounds.x, bounds.y + 14.0, bounds.width * 0.6, 10.0);
            fill_rect(shapes, bar, [0.85, 0.85, 0.85, 1.0]);
            fill_rect(
                shapes,
                Rect::new(bar.x, bar.y, bar.width * value.clamp(0.0, 1.0), bar.height),
                [0.2, 0.7, 0.3, 1.0],
            );
            text::queue_left(
                text_out,
                bar.x + bar.width + 8.0,
                bar.y - 2.0,
                label,
                theme.text,
            );
        }
        ElementKind::Slider { value, label } => {
            text::queue_left(text_out, bounds.x, bounds.y, "slider", theme.text);
            let track = Rect::new(bounds.x, bounds.y + 16.0, bounds.width * 0.6, 4.0);
            fill_rect(shapes, track, [0.75, 0.75, 0.75, 1.0]);
            let knob_x = track.x + track.width * value.clamp(0.0, 1.0) - 6.0;
            fill_rect(
                shapes,
                Rect::new(knob_x, track.y - 4.0, 12.0, 12.0),
                theme.accent,
            );
            text::queue_left(
                text_out,
                track.x + track.width + 8.0,
                track.y - 4.0,
                label,
                theme.text,
            );
        }
        ElementKind::Search { value, width } => {
            text::queue_left(text_out, bounds.x, bounds.y, "search", theme.text);
            let w = width.min(bounds.width);
            let rect = Rect::new(bounds.x, bounds.y + 14.0, w, CONTROL_H);
            paint_text_field(shapes, text_out, rect, value, theme);
        }
        ElementKind::Color => {
            stroke_rect(shapes, bounds, theme.border, 1.0);
            fill_rect(
                shapes,
                Rect::new(bounds.x, bounds.y, bounds.width, bounds.height - 14.0).inset(1.0),
                [0.8, 0.2, 0.2, 1.0],
            );
            text::queue_left(
                text_out,
                bounds.x,
                bounds.y + bounds.height - 12.0,
                "color",
                theme.text,
            );
        }
        ElementKind::Footer { text } => {
            style.fill_background(shapes, bounds);
            style.queue(text_out, bounds.x + 8.0, bounds.y + 8.0, text, None);
        }
        ElementKind::PlainText { text } => {
            style.queue_wrapped(
                text_out,
                bounds.x,
                bounds.y + 2.0,
                text,
                bounds.width,
                TEXT_LINE,
                None,
            );
        }
    }
}

fn paint_inlines(
    text_out: &mut text::TextBatch,
    bounds: Rect,
    inlines: &[Inline],
    style: &PaintStyle,
) {
    let mut x = bounds.x;
    let mut y = bounds.y + 2.0;
    let right = bounds.x + bounds.width;
    for inline in inlines {
        let (text, color) = match inline {
            Inline::Text(t) => (t.as_str(), style.text),
            Inline::Bold(t) => (t.as_str(), style.text),
            Inline::Italic(t) => (t.as_str(), [0.35, 0.35, 0.35, 1.0]),
        };
        (x, y) = paint_inline_run(
            text_out,
            x,
            y,
            bounds.x,
            right,
            text,
            color,
            style.font_scale,
        );
    }
}

fn paint_inline_run(
    text_out: &mut text::TextBatch,
    mut x: f32,
    mut y: f32,
    left: f32,
    right: f32,
    text: &str,
    color: [f32; 4],
    font_scale: f32,
) -> (f32, f32) {
    let char_w = text::char_width(font_scale);
    let mut line_start = x;
    let mut line = String::new();
    for ch in text.chars() {
        if x + char_w > right && x > left {
            if !line.is_empty() {
                text::queue_left_scaled(text_out, line_start, y, &line, color, font_scale);
                line.clear();
            }
            x = left;
            line_start = x;
            y += TEXT_LINE;
        }
        line.push(ch);
        x += char_w;
    }
    if !line.is_empty() {
        text::queue_left_scaled(text_out, line_start, y, &line, color, font_scale);
    }
    (x, y)
}

fn paint_details_summary(
    shapes: &mut Vec<ShapeInstance>,
    text_out: &mut text::TextBatch,
    bounds: Rect,
    summary: &str,
    checkbox: bool,
    open: bool,
    style: &PaintStyle,
    theme: &Theme,
) {
    let row = Rect::new(bounds.x, bounds.y, bounds.width, CONTROL_H);
    if style.background.is_none() {
        fill_rect(shapes, row, [0.93, 0.93, 0.93, 1.0]);
    }
    stroke_rect(shapes, row, style.border_color, style.border_width);
    style.queue(
        text_out,
        bounds.x + 8.0,
        bounds.y + 6.0,
        if open { "v" } else { ">" },
        None,
    );
    let mut tx = bounds.x + 24.0;
    if checkbox {
        paint_checkbox(
            shapes,
            Rect::new(tx, bounds.y + 6.0, 16.0, 16.0),
            open,
            theme,
        );
        tx += 24.0;
    }
    style.queue(text_out, tx, bounds.y + 6.0, summary, None);
}

fn paint_input(
    shapes: &mut Vec<ShapeInstance>,
    text_out: &mut text::TextBatch,
    bounds: Rect,
    input_type: &InputType,
    value: &str,
    checked: bool,
    label: Option<&str>,
    theme: &Theme,
) {
    match input_type {
        InputType::Checkbox => {
            if let Some(label) = label {
                paint_checkbox(
                    shapes,
                    Rect::new(bounds.x, bounds.y + 2.0, 16.0, 16.0),
                    checked,
                    theme,
                );
                text::queue_left(text_out, bounds.x + 24.0, bounds.y + 2.0, label, theme.text);
            }
        }
        InputType::Radio => {
            if let Some(label) = label {
                let c = Rect::new(bounds.x, bounds.y + 2.0, 16.0, 16.0);
                shapes.push(ShapeInstance::circle(
                    c.x + 8.0,
                    c.y + 8.0,
                    16.0,
                    theme.control_bg,
                ));
                stroke_rect(shapes, c, theme.border, 1.0);
                if checked {
                    fill_rect(
                        shapes,
                        Rect::new(c.x + 4.0, c.y + 4.0, 8.0, 8.0),
                        theme.accent,
                    );
                }
                text::queue_left(text_out, bounds.x + 24.0, bounds.y + 2.0, label, theme.text);
            }
        }
        _ => {
            if let Some(label) = label {
                text::queue_left(text_out, bounds.x, bounds.y, label, theme.text);
            }
            let field = if label.is_some() {
                Rect::new(
                    bounds.x,
                    bounds.y + 18.0,
                    bounds.width.min(280.0),
                    CONTROL_H,
                )
            } else {
                Rect::new(bounds.x, bounds.y, bounds.width.min(280.0), CONTROL_H)
            };
            let display = if matches!(input_type, InputType::Password) {
                "*".repeat(value.len())
            } else {
                value.to_string()
            };
            paint_text_field(shapes, text_out, field, &display, theme);
        }
    }
}

fn paint_checkbox(shapes: &mut Vec<ShapeInstance>, rect: Rect, checked: bool, theme: &Theme) {
    fill_rect(shapes, rect, theme.control_bg);
    stroke_rect(shapes, rect, theme.border, 1.0);
    if checked {
        fill_rect(
            shapes,
            Rect::new(rect.x + 3.0, rect.y + 3.0, 10.0, 10.0),
            theme.accent,
        );
    }
}

fn paint_text_field(
    shapes: &mut Vec<ShapeInstance>,
    text_out: &mut text::TextBatch,
    rect: Rect,
    value: &str,
    theme: &Theme,
) {
    fill_rect(shapes, rect, theme.control_bg);
    stroke_rect(shapes, rect, theme.border, 1.0);
    text::queue_left(text_out, rect.x + 6.0, rect.y + 6.0, value, theme.text);
}

fn paint_select(
    shapes: &mut Vec<ShapeInstance>,
    text_out: &mut text::TextBatch,
    bounds: Rect,
    options: &[String],
    selected: usize,
    theme: &Theme,
) {
    let rect = Rect::new(bounds.x, bounds.y, bounds.width.min(200.0), CONTROL_H);
    fill_rect(shapes, rect, theme.control_bg);
    stroke_rect(shapes, rect, theme.border, 1.0);
    let label = options.get(selected).map(String::as_str).unwrap_or("");
    text::queue_left(text_out, rect.x + 6.0, rect.y + 6.0, label, theme.text);
    fill_rect(
        shapes,
        Rect::new(rect.x + rect.width - 18.0, rect.y + 6.0, 12.0, 12.0),
        theme.button_bg,
    );
}

fn paint_textarea(
    shapes: &mut Vec<ShapeInstance>,
    text_out: &mut text::TextBatch,
    bounds: Rect,
    value: &str,
    rows: u32,
    theme: &Theme,
) {
    let rect = Rect::new(
        bounds.x,
        bounds.y,
        bounds.width.min(320.0),
        rows as f32 * TEXT_LINE + 8.0,
    );
    fill_rect(shapes, rect, theme.control_bg);
    stroke_rect(shapes, rect, theme.border, 1.0);
    text::queue_left(text_out, rect.x + 6.0, rect.y + 6.0, value, theme.text);
}

fn paint_button(
    shapes: &mut Vec<ShapeInstance>,
    text_out: &mut text::TextBatch,
    bounds: Rect,
    label: &str,
    button_type: &ButtonType,
    style: &PaintStyle,
) {
    if let Some(bg) = style.background {
        fill_rect(shapes, bounds, bg);
    } else {
        fill_rect(shapes, bounds, [0.9, 0.9, 0.9, 1.0]);
    }
    stroke_rect(shapes, bounds, style.border_color, style.border_width);
    let prefix = match button_type {
        ButtonType::Submit => "[submit] ",
        ButtonType::Reset => "[reset] ",
        ButtonType::Button => "",
    };
    let text = format!("{prefix}{label}");
    style.queue_wrapped(
        text_out,
        bounds.x + 8.0,
        bounds.y + 6.0,
        &text,
        bounds.width - 16.0,
        bounds.height - 12.0,
        None,
    );
}

fn paint_table(
    shapes: &mut Vec<ShapeInstance>,
    text_out: &mut text::TextBatch,
    bounds: Rect,
    headers: &[String],
    rows: &[Vec<String>],
    theme: &Theme,
) {
    let cols = headers.len().max(1);
    let col_w = bounds.width / cols as f32;
    let row_h = 24.0;
    stroke_rect(shapes, bounds, theme.border, 1.0);
    for (c, header) in headers.iter().enumerate() {
        let cell = Rect::new(bounds.x + c as f32 * col_w, bounds.y, col_w, row_h);
        fill_rect(shapes, cell, theme.table_header);
        stroke_rect(shapes, cell, theme.border, 1.0);
        text::queue_left(text_out, cell.x + 4.0, cell.y + 4.0, header, theme.text);
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            let cell = Rect::new(
                bounds.x + c as f32 * col_w,
                bounds.y + (r + 1) as f32 * row_h,
                col_w,
                row_h,
            );
            fill_rect(shapes, cell, theme.control_bg);
            stroke_rect(shapes, cell, theme.border, 1.0);
            text::queue_left(text_out, cell.x + 4.0, cell.y + 4.0, value, theme.text);
        }
    }
}

fn paint_svg(
    shapes: &mut Vec<ShapeInstance>,
    bounds: Rect,
    width: f32,
    height: f32,
    children: &[SvgChild],
    theme: &Theme,
) {
    stroke_rect(shapes, bounds, theme.border, 1.0);
    let sx = bounds.width / width;
    let sy = bounds.height / height;
    for child in children {
        match child {
            SvgChild::Rect {
                x,
                y,
                width,
                height,
                fill,
                stroke,
            } => {
                let r = Rect::new(
                    bounds.x + x * sx,
                    bounds.y + y * sy,
                    width * sx,
                    height * sy,
                );
                fill_rect(shapes, r, *fill);
                stroke_rect(shapes, r, *stroke, 2.0);
            }
            SvgChild::Circle { cx, cy, r, fill } => {
                shapes.push(ShapeInstance::circle(
                    bounds.x + cx * sx,
                    bounds.y + cy * sy,
                    r * 2.0 * sx,
                    *fill,
                ));
            }
            SvgChild::Path { points, stroke } => {
                for window in points.windows(2) {
                    let (x0, y0) = window[0];
                    let (x1, y1) = window[1];
                    let dx = x1 - x0;
                    let dy = y1 - y0;
                    let len = (dx * dx + dy * dy).sqrt().max(2.0);
                    fill_rect(
                        shapes,
                        Rect::new(bounds.x + x0 * sx, bounds.y + y0 * sy, len * sx, 3.0),
                        *stroke,
                    );
                }
            }
        }
    }
}

fn fill_rect(shapes: &mut Vec<ShapeInstance>, rect: Rect, color: [f32; 4]) {
    if rect.width > 0.0 && rect.height > 0.0 {
        shapes.push(ShapeInstance::rect(rect, color));
    }
}

fn stroke_rect(shapes: &mut Vec<ShapeInstance>, rect: Rect, color: [f32; 4], thickness: f32) {
    fill_rect(
        shapes,
        Rect::new(rect.x, rect.y, rect.width, thickness),
        color,
    );
    fill_rect(
        shapes,
        Rect::new(rect.x, rect.bottom() - thickness, rect.width, thickness),
        color,
    );
    fill_rect(
        shapes,
        Rect::new(rect.x, rect.y, thickness, rect.height),
        color,
    );
    fill_rect(
        shapes,
        Rect::new(
            rect.x + rect.width - thickness,
            rect.y,
            thickness,
            rect.height,
        ),
        color,
    );
}

fn offset_rect(rect: Rect, dx: f32, dy: f32) -> Rect {
    Rect::new(rect.x + dx, rect.y + dy, rect.width, rect.height)
}

fn clip_shapes(shapes: &mut Vec<ShapeInstance>, clip: Rect) {
    shapes.retain_mut(|shape| {
        let bounds = Rect::new(
            shape.pos_size[0],
            shape.pos_size[1],
            shape.pos_size[2],
            shape.pos_size[3],
        );
        let Some(clipped) = bounds.intersection(clip) else {
            return false;
        };
        shape.pos_size = [clipped.x, clipped.y, clipped.width, clipped.height];
        true
    });
}

fn clip_text(text: &mut TextBatch, clip: Rect) {
    text.sections.retain_mut(|section| {
        if section.x < clip.x || section.y < clip.y {
            return false;
        }
        let bounds = Rect::new(section.x, section.y, section.width, section.height);
        let Some(clipped) = bounds.intersection(clip) else {
            return false;
        };
        section.width = clipped.width;
        section.height = clipped.height;
        true
    });
}
