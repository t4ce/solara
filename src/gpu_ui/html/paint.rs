use crate::gpu_ui::geometry::{Rect, CONTROL_H, TEXT_LINE};
use crate::gpu_ui::html::node::{ButtonType, ElementKind, HtmlNode, Inline, InputType, SvgChild};
use crate::gpu_ui::shapes::ShapeInstance;
use crate::gpu_ui::text;

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
    out: &mut Vec<ShapeInstance>,
) {
    let theme = Theme::default();
    for node in nodes {
        paint_node(node, scroll_y, &theme, out);
    }
}

fn paint_node(node: &HtmlNode, scroll_y: f32, theme: &Theme, out: &mut Vec<ShapeInstance>) {
    let bounds = offset_rect(node.bounds, 0.0, -scroll_y);
    if bounds.bottom() < 0.0 || bounds.y > 2000.0 {
        // coarse cull for off-screen blocks
    }

    match &node.kind {
        ElementKind::Heading { level, text } => {
            let color = theme.text;
            text::draw_text_left(out, bounds.x, bounds.y + 4.0, text, color);
            let _ = level;
        }
        ElementKind::Paragraph { inlines } => paint_inlines(out, bounds, inlines, theme.text),
        ElementKind::HorizontalRule => {
            stroke_rect(out, bounds, theme.border, 1.0);
        }
        ElementKind::Link { text, .. } => {
            text::draw_text_left(out, bounds.x, bounds.y + 2.0, text, theme.link);
            fill_rect(out, Rect::new(bounds.x, bounds.bottom() - 2.0, text.chars().count() as f32 * 8.0, 1.0), theme.link);
        }
        ElementKind::OrderedList { items } => {
            for (i, item) in items.iter().enumerate() {
                let y = bounds.y + i as f32 * TEXT_LINE;
                text::draw_text_left(out, bounds.x, y, &format!("{}.", i + 1), theme.text);
                text::draw_text_left(out, bounds.x + 24.0, y, item, theme.text);
            }
        }
        ElementKind::UnorderedList { items } => {
            for (i, item) in items.iter().enumerate() {
                let y = bounds.y + i as f32 * TEXT_LINE;
                fill_rect(out, Rect::new(bounds.x + 4.0, y + 6.0, 4.0, 4.0), theme.text);
                text::draw_text_left(out, bounds.x + 16.0, y, item, theme.text);
            }
        }
        ElementKind::Details {
            summary,
            summary_checkbox,
            children,
        } => {
            paint_details_summary(out, bounds, summary, *summary_checkbox, node.open, theme);
            if node.open {
                let inner = Rect::new(bounds.x + 12.0, bounds.y + CONTROL_H, bounds.width - 12.0, bounds.height - CONTROL_H);
                stroke_rect(out, inner, theme.border, 1.0);
                for child in children {
                    paint_node(child, scroll_y, theme, out);
                }
            }
        }
        ElementKind::Div { children } | ElementKind::Form { children } => {
            for child in children {
                paint_node(child, scroll_y, theme, out);
            }
        }
        ElementKind::Label { text, control } => {
            text::draw_text_left(out, bounds.x, bounds.y + 4.0, text, theme.text);
            paint_node(control, scroll_y, theme, out);
        }
        ElementKind::Input {
            input_type,
            value,
            checked,
            label,
            ..
        } => paint_input(out, bounds, input_type, value, *checked, label.as_deref(), theme),
        ElementKind::Select { options, selected } => {
            paint_select(out, bounds, options, *selected, theme);
        }
        ElementKind::Textarea { value, rows, .. } => {
            paint_textarea(out, bounds, value, *rows, theme);
        }
        ElementKind::Button { label, button_type } => {
            paint_button(out, bounds, label, button_type, theme);
        }
        ElementKind::Table { headers, rows } => {
            paint_table(out, bounds, headers, rows, theme);
        }
        ElementKind::Svg { width, height, children } => {
            paint_svg(out, bounds, *width, *height, children, theme);
        }
        ElementKind::Canvas { width, height } => {
            stroke_rect(out, bounds, theme.border, 1.0);
            fill_rect(out, bounds.inset(1.0), [0.92, 0.92, 0.92, 1.0]);
            text::draw_text_left(out, bounds.x + 8.0, bounds.y + 8.0, &format!("canvas {}x{}", *width as u32, *height as u32), theme.text);
        }
        ElementKind::Iframe { children } => {
            fill_rect(out, bounds, theme.iframe_bg);
            stroke_rect(out, bounds, theme.border, 2.0);
            text::draw_text_left(out, bounds.x + 8.0, bounds.y + 4.0, "iframe", theme.text);
            let inner = bounds.inset(8.0);
            for child in children {
                let mut cloned = child.clone();
                let dy = inner.y - cloned.bounds.y;
                shift_bounds(&mut cloned, 0.0, dy);
                paint_node(&cloned, scroll_y, theme, out);
            }
        }
        ElementKind::Image { alt, .. } => {
            fill_rect(out, bounds, theme.img_fill);
            stroke_rect(out, bounds, theme.border, 1.0);
            text::draw_text_left(out, bounds.x + 8.0, bounds.y + bounds.height * 0.5 - 4.0, alt, [1.0, 1.0, 1.0, 1.0]);
        }
        ElementKind::Dialog { children, floating } => {
            fill_rect(out, bounds, theme.dialog_bg);
            stroke_rect(out, bounds, theme.border, if *floating { 2.0 } else { 1.0 });
            if *floating {
                text::draw_text_left(out, bounds.x + 8.0, bounds.y + 4.0, "dialog", theme.text);
            }
            for child in children {
                paint_node(child, scroll_y, theme, out);
            }
        }
        ElementKind::Progress { value, max } => {
            text::draw_text_left(out, bounds.x, bounds.y, "progress", theme.text);
            let bar = Rect::new(bounds.x, bounds.y + 14.0, bounds.width, 10.0);
            fill_rect(out, bar, [0.85, 0.85, 0.85, 1.0]);
            let fill_w = bar.width * (value / max).clamp(0.0, 1.0);
            fill_rect(out, Rect::new(bar.x, bar.y, fill_w, bar.height), theme.accent);
        }
        ElementKind::Meter { value, label } => {
            text::draw_text_left(out, bounds.x, bounds.y, "meter", theme.text);
            let bar = Rect::new(bounds.x, bounds.y + 14.0, bounds.width * 0.6, 10.0);
            fill_rect(out, bar, [0.85, 0.85, 0.85, 1.0]);
            fill_rect(out, Rect::new(bar.x, bar.y, bar.width * value.clamp(0.0, 1.0), bar.height), [0.2, 0.7, 0.3, 1.0]);
            text::draw_text_left(out, bar.x + bar.width + 8.0, bar.y - 2.0, label, theme.text);
        }
        ElementKind::Slider { value, label } => {
            text::draw_text_left(out, bounds.x, bounds.y, "slider", theme.text);
            let track = Rect::new(bounds.x, bounds.y + 16.0, bounds.width * 0.6, 4.0);
            fill_rect(out, track, [0.75, 0.75, 0.75, 1.0]);
            let knob_x = track.x + track.width * value.clamp(0.0, 1.0) - 6.0;
            fill_rect(out, Rect::new(knob_x, track.y - 4.0, 12.0, 12.0), theme.accent);
            text::draw_text_left(out, track.x + track.width + 8.0, track.y - 4.0, label, theme.text);
        }
        ElementKind::Search { value, width } => {
            text::draw_text_left(out, bounds.x, bounds.y, "search", theme.text);
            let w = width.min(bounds.width);
            let rect = Rect::new(bounds.x, bounds.y + 14.0, w, CONTROL_H);
            paint_text_field(out, rect, value, theme);
        }
        ElementKind::Color => {
            stroke_rect(out, bounds, theme.border, 1.0);
            fill_rect(
                out,
                Rect::new(bounds.x, bounds.y, bounds.width, bounds.height - 14.0).inset(1.0),
                [0.8, 0.2, 0.2, 1.0],
            );
            text::draw_text_left(
                out,
                bounds.x,
                bounds.y + bounds.height - 12.0,
                "color",
                theme.text,
            );
        }
        ElementKind::Footer { text } => {
            fill_rect(out, bounds, [0.92, 0.92, 0.92, 1.0]);
            text::draw_text_left(out, bounds.x + 8.0, bounds.y + 8.0, text, theme.text);
        }
        ElementKind::PlainText { text } => {
            text::draw_text_wrapped(
                out,
                bounds.x,
                bounds.y + 2.0,
                text,
                bounds.width,
                TEXT_LINE,
                theme.text,
            );
        }
    }
}

fn paint_inlines(out: &mut Vec<ShapeInstance>, bounds: Rect, inlines: &[Inline], default: [f32; 4]) {
    let mut x = bounds.x;
    let mut y = bounds.y + 2.0;
    let right = bounds.x + bounds.width;
    for inline in inlines {
        let (text, color) = match inline {
            Inline::Text(t) => (t.as_str(), default),
            Inline::Bold(t) => (t.as_str(), default),
            Inline::Italic(t) => (t.as_str(), [0.35, 0.35, 0.35, 1.0]),
        };
        (x, y) = paint_inline_run(out, x, y, bounds.x, right, text, color);
    }
}

fn paint_inline_run(
    out: &mut Vec<ShapeInstance>,
    mut x: f32,
    mut y: f32,
    left: f32,
    right: f32,
    text: &str,
    color: [f32; 4],
) -> (f32, f32) {
    let mut line_start = x;
    let mut line = String::new();
    for ch in text.chars() {
        if x + text::CHAR_W > right && x > left {
            if !line.is_empty() {
                text::draw_text_left(out, line_start, y, &line, color);
                line.clear();
            }
            x = left;
            line_start = x;
            y += TEXT_LINE;
        }
        line.push(ch);
        x += text::CHAR_W;
    }
    if !line.is_empty() {
        text::draw_text_left(out, line_start, y, &line, color);
    }
    (x, y)
}

fn paint_details_summary(
    out: &mut Vec<ShapeInstance>,
    bounds: Rect,
    summary: &str,
    checkbox: bool,
    open: bool,
    theme: &Theme,
) {
    let row = Rect::new(bounds.x, bounds.y, bounds.width, CONTROL_H);
    fill_rect(out, row, [0.93, 0.93, 0.93, 1.0]);
    stroke_rect(out, row, theme.border, 1.0);
    text::draw_text_left(out, bounds.x + 8.0, bounds.y + 6.0, if open { "v" } else { ">" }, theme.text);
    let mut tx = bounds.x + 24.0;
    if checkbox {
        paint_checkbox(out, Rect::new(tx, bounds.y + 6.0, 16.0, 16.0), open, theme);
        tx += 24.0;
    }
    text::draw_text_left(out, tx, bounds.y + 6.0, summary, theme.text);
}

fn paint_input(
    out: &mut Vec<ShapeInstance>,
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
                paint_checkbox(out, Rect::new(bounds.x, bounds.y + 2.0, 16.0, 16.0), checked, theme);
                text::draw_text_left(out, bounds.x + 24.0, bounds.y + 2.0, label, theme.text);
            }
        }
        InputType::Radio => {
            if let Some(label) = label {
                let c = Rect::new(bounds.x, bounds.y + 2.0, 16.0, 16.0);
                out.push(ShapeInstance::circle(c.x + 8.0, c.y + 8.0, 16.0, theme.control_bg));
                stroke_rect(out, c, theme.border, 1.0);
                if checked {
                    fill_rect(out, Rect::new(c.x + 4.0, c.y + 4.0, 8.0, 8.0), theme.accent);
                }
                text::draw_text_left(out, bounds.x + 24.0, bounds.y + 2.0, label, theme.text);
            }
        }
        _ => {
            if let Some(label) = label {
                text::draw_text_left(out, bounds.x, bounds.y, label, theme.text);
            }
            let field = if label.is_some() {
                Rect::new(bounds.x, bounds.y + 18.0, bounds.width.min(280.0), CONTROL_H)
            } else {
                Rect::new(bounds.x, bounds.y, bounds.width.min(280.0), CONTROL_H)
            };
            let display = if matches!(input_type, InputType::Password) {
                "*".repeat(value.len())
            } else {
                value.to_string()
            };
            paint_text_field(out, field, &display, theme);
        }
    }
}

fn paint_checkbox(out: &mut Vec<ShapeInstance>, rect: Rect, checked: bool, theme: &Theme) {
    fill_rect(out, rect, theme.control_bg);
    stroke_rect(out, rect, theme.border, 1.0);
    if checked {
        fill_rect(out, Rect::new(rect.x + 3.0, rect.y + 3.0, 10.0, 10.0), theme.accent);
    }
}

fn paint_text_field(out: &mut Vec<ShapeInstance>, rect: Rect, value: &str, theme: &Theme) {
    fill_rect(out, rect, theme.control_bg);
    stroke_rect(out, rect, theme.border, 1.0);
    text::draw_text_left(out, rect.x + 6.0, rect.y + 6.0, value, theme.text);
}

fn paint_select(out: &mut Vec<ShapeInstance>, bounds: Rect, options: &[String], selected: usize, theme: &Theme) {
    let rect = Rect::new(bounds.x, bounds.y, bounds.width.min(200.0), CONTROL_H);
    fill_rect(out, rect, theme.control_bg);
    stroke_rect(out, rect, theme.border, 1.0);
    let label = options.get(selected).map(String::as_str).unwrap_or("");
    text::draw_text_left(out, rect.x + 6.0, rect.y + 6.0, label, theme.text);
    fill_rect(out, Rect::new(rect.x + rect.width - 18.0, rect.y + 6.0, 12.0, 12.0), theme.button_bg);
}

fn paint_textarea(out: &mut Vec<ShapeInstance>, bounds: Rect, value: &str, rows: u32, theme: &Theme) {
    let rect = Rect::new(bounds.x, bounds.y, bounds.width.min(320.0), rows as f32 * TEXT_LINE + 8.0);
    fill_rect(out, rect, theme.control_bg);
    stroke_rect(out, rect, theme.border, 1.0);
    text::draw_text_left(out, rect.x + 6.0, rect.y + 6.0, value, theme.text);
}

fn paint_button(out: &mut Vec<ShapeInstance>, bounds: Rect, label: &str, button_type: &ButtonType, theme: &Theme) {
    fill_rect(out, bounds, theme.button_bg);
    stroke_rect(out, bounds, theme.border, 1.0);
    let prefix = match button_type {
        ButtonType::Submit => "[submit] ",
        ButtonType::Reset => "[reset] ",
        ButtonType::Button => "",
    };
    let text = format!("{prefix}{label}");
    text::draw_text_wrapped(
        out,
        bounds.x + 8.0,
        bounds.y + 6.0,
        &text,
        bounds.width - 16.0,
        TEXT_LINE,
        theme.text,
    );
}

fn paint_table(out: &mut Vec<ShapeInstance>, bounds: Rect, headers: &[String], rows: &[Vec<String>], theme: &Theme) {
    let cols = headers.len().max(1);
    let col_w = bounds.width / cols as f32;
    let row_h = 24.0;
    stroke_rect(out, bounds, theme.border, 1.0);
    for (c, header) in headers.iter().enumerate() {
        let cell = Rect::new(bounds.x + c as f32 * col_w, bounds.y, col_w, row_h);
        fill_rect(out, cell, theme.table_header);
        stroke_rect(out, cell, theme.border, 1.0);
        text::draw_text_left(out, cell.x + 4.0, cell.y + 4.0, header, theme.text);
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            let cell = Rect::new(bounds.x + c as f32 * col_w, bounds.y + (r + 1) as f32 * row_h, col_w, row_h);
            fill_rect(out, cell, theme.control_bg);
            stroke_rect(out, cell, theme.border, 1.0);
            text::draw_text_left(out, cell.x + 4.0, cell.y + 4.0, value, theme.text);
        }
    }
}

fn paint_svg(out: &mut Vec<ShapeInstance>, bounds: Rect, width: f32, height: f32, children: &[SvgChild], theme: &Theme) {
    stroke_rect(out, bounds, theme.border, 1.0);
    let sx = bounds.width / width;
    let sy = bounds.height / height;
    for child in children {
        match child {
            SvgChild::Rect { x, y, width, height, fill, stroke } => {
                let r = Rect::new(bounds.x + x * sx, bounds.y + y * sy, width * sx, height * sy);
                fill_rect(out, r, *fill);
                stroke_rect(out, r, *stroke, 2.0);
            }
            SvgChild::Circle { cx, cy, r, fill } => {
                out.push(ShapeInstance::circle(
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
                        out,
                        Rect::new(bounds.x + x0 * sx, bounds.y + y0 * sy, len * sx, 3.0),
                        *stroke,
                    );
                }
            }
        }
    }
}

fn fill_rect(out: &mut Vec<ShapeInstance>, rect: Rect, color: [f32; 4]) {
    if rect.width > 0.0 && rect.height > 0.0 {
        out.push(ShapeInstance::rect(rect, color));
    }
}

fn stroke_rect(out: &mut Vec<ShapeInstance>, rect: Rect, color: [f32; 4], thickness: f32) {
    fill_rect(out, Rect::new(rect.x, rect.y, rect.width, thickness), color);
    fill_rect(out, Rect::new(rect.x, rect.bottom() - thickness, rect.width, thickness), color);
    fill_rect(out, Rect::new(rect.x, rect.y, thickness, rect.height), color);
    fill_rect(out, Rect::new(rect.x + rect.width - thickness, rect.y, thickness, rect.height), color);
}

fn offset_rect(rect: Rect, dx: f32, dy: f32) -> Rect {
    Rect::new(rect.x + dx, rect.y + dy, rect.width, rect.height)
}

fn shift_bounds(node: &mut HtmlNode, dx: f32, dy: f32) {
    node.bounds.x += dx;
    node.bounds.y += dy;
    match &mut node.kind {
        ElementKind::Details { children, .. }
        | ElementKind::Div { children }
        | ElementKind::Form { children }
        | ElementKind::Iframe { children }
        | ElementKind::Dialog { children, .. } => {
            for child in children.iter_mut() {
                shift_bounds(child, dx, dy);
            }
        }
        ElementKind::Label { control, .. } => shift_bounds(control, dx, dy),
        _ => {}
    }
}
