#![allow(clippy::too_many_arguments)]

pub const DEFAULT_FONT_SIZE: f32 = 14.0;

#[derive(Clone, Debug, Default)]
pub struct TextSection {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub text: String,
    pub color: [f32; 4],
    /// CSS `font-size` in logical pixels. The renderer converts this em size
    /// to the bundled font's `ab_glyph` scale at the final handoff.
    pub font_size: f32,
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

pub fn char_width(font_size: f32) -> f32 {
    solara_wgpu_shim::char_width(font_size)
}

pub fn chars_per_line_sized(max_width: f32, font_size: f32) -> usize {
    (max_width / char_width(font_size)).floor().max(1.0) as usize
}

pub fn wrapped_line_count_sized(text: &str, max_width: f32, font_size: f32) -> usize {
    let per_line = chars_per_line_sized(max_width, font_size);
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
        section.font_size *= scale;
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
    font_size: f32,
) {
    if text.is_empty() || font_size <= 0.0 {
        return;
    }
    batch.sections.push(TextSection {
        x,
        y,
        width,
        height,
        text: text.to_string(),
        color,
        font_size,
    });
}

pub fn queue_left(batch: &mut TextBatch, x: f32, y: f32, text: &str, color: [f32; 4]) {
    queue_left_at_size(batch, x, y, text, color, DEFAULT_FONT_SIZE);
}

pub fn queue_left_at_size(
    batch: &mut TextBatch,
    x: f32,
    y: f32,
    text: &str,
    color: [f32; 4],
    font_size: f32,
) {
    queue_left_sized(
        batch,
        x,
        y,
        text,
        color,
        font_size,
        metrics(font_size).natural_line_height(),
    );
}

pub fn queue_left_sized(
    batch: &mut TextBatch,
    x: f32,
    y: f32,
    text: &str,
    color: [f32; 4],
    font_size: f32,
    line_height: f32,
) {
    let cw = char_width(font_size);
    let width = text.chars().count() as f32 * cw;
    let height = line_height.max(metrics(font_size).natural_line_height());
    push_section(batch, x, y, width, height, text, color, font_size);
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
    let line_height = metrics(DEFAULT_FONT_SIZE).natural_line_height();
    queue_wrapped_sized(
        batch,
        x,
        y,
        text,
        max_width,
        max_height,
        color,
        DEFAULT_FONT_SIZE,
        line_height,
    );
}

pub fn queue_wrapped_sized(
    batch: &mut TextBatch,
    x: f32,
    y: f32,
    text: &str,
    max_width: f32,
    max_height: f32,
    color: [f32; 4],
    font_size: f32,
    line_height: f32,
) {
    if text.is_empty() || font_size <= 0.0 {
        return;
    }
    let per_line = chars_per_line_sized(max_width, font_size);
    let line_advance = line_height.max(0.0);
    let max_lines = if line_advance > 0.0 {
        (max_height / line_advance).floor().max(1.0) as usize
    } else {
        usize::MAX
    };
    let section_height = line_advance.max(metrics(font_size).natural_line_height());
    let characters = text.chars().collect::<Vec<_>>();
    for (line_index, chunk) in characters.chunks(per_line).take(max_lines).enumerate() {
        let line = chunk.iter().collect::<String>();
        push_section(
            batch,
            x,
            y + line_index as f32 * line_advance,
            max_width,
            section_height,
            &line,
            color,
            font_size,
        );
    }
}

pub fn metrics(font_size: f32) -> solara_wgpu_shim::FontMetrics {
    solara_wgpu_shim::font_metrics(font_size)
}
