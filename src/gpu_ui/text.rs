#![allow(clippy::too_many_arguments)]

pub const FONT_SCALE: f32 = 14.0;

#[derive(Clone, Debug, Default)]
pub struct TextSection {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub text: String,
    pub color: [f32; 4],
    pub scale: f32,
}

#[derive(Clone, Debug, Default)]
pub struct TextBatch {
    pub sections: Vec<TextSection>,
}

impl TextBatch {
    pub fn clear(&mut self) {
        self.sections.clear();
    }
}

pub fn char_width(scale: f32) -> f32 {
    solara_wgpu_shim::char_width(scale)
}

pub fn char_width_default() -> f32 {
    char_width(FONT_SCALE)
}

pub fn chars_per_line(max_width: f32) -> usize {
    (max_width / char_width_default()).floor().max(1.0) as usize
}

pub fn wrapped_line_count(text: &str, max_width: f32) -> usize {
    let per_line = chars_per_line(max_width);
    text.chars().count().max(1).div_ceil(per_line)
}

pub fn scale_text_batch(batch: &mut TextBatch, scale: f32) {
    if (scale - 1.0).abs() < f32::EPSILON {
        return;
    }
    for section in &mut batch.sections {
        section.x *= scale;
        section.y *= scale;
        section.width *= scale;
        section.height *= scale;
        section.scale *= scale;
    }
}

fn push_section(
    batch: &mut TextBatch,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    text: &str,
    color: [f32; 4],
    scale: f32,
) {
    if text.is_empty() {
        return;
    }
    batch.sections.push(TextSection {
        x,
        y,
        width,
        height,
        text: text.to_string(),
        color,
        scale,
    });
}

pub fn queue_left(batch: &mut TextBatch, x: f32, y: f32, text: &str, color: [f32; 4]) {
    queue_left_scaled(batch, x, y, text, color, FONT_SCALE);
}

pub fn queue_left_scaled(
    batch: &mut TextBatch,
    x: f32,
    y: f32,
    text: &str,
    color: [f32; 4],
    scale: f32,
) {
    let cw = char_width(scale);
    let width = text.chars().count() as f32 * cw;
    push_section(batch, x, y, width, scale * 1.25, text, color, scale);
}

#[allow(dead_code)]
pub fn queue_wrapped(
    batch: &mut TextBatch,
    x: f32,
    y: f32,
    text: &str,
    max_width: f32,
    max_height: f32,
    color: [f32; 4],
) {
    queue_wrapped_scaled(batch, x, y, text, max_width, max_height, color, FONT_SCALE);
}

pub fn queue_wrapped_scaled(
    batch: &mut TextBatch,
    x: f32,
    y: f32,
    text: &str,
    max_width: f32,
    max_height: f32,
    color: [f32; 4],
    scale: f32,
) {
    push_section(batch, x, y, max_width, max_height, text, color, scale);
}
