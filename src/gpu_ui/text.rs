use font8x8::{BASIC_FONTS, UnicodeFonts};

use crate::gpu_ui::geometry::Rect;
use crate::gpu_ui::shapes::{ShapeInstance, SHAPE_RECT};

const CHAR_W: f32 = 8.0;
const CHAR_H: f32 = 8.0;

pub fn measure_text(text: &str) -> (f32, f32) {
    (text.chars().count() as f32 * CHAR_W, CHAR_H)
}

pub fn draw_text_left(
    out: &mut Vec<ShapeInstance>,
    x: f32,
    y: f32,
    text: &str,
    color: [f32; 4],
) {
    let mut cursor_x = x;
    for ch in text.chars() {
        if let Some(glyph) = BASIC_FONTS.get(ch) {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        out.push(ShapeInstance {
                            pos_size: [cursor_x + col as f32, y + row as f32, 1.0, 1.0],
                            color,
                            shape_type: SHAPE_RECT,
                            _pad: 0,
                        });
                    }
                }
            }
        }
        cursor_x += CHAR_W;
    }
}

pub fn append_text_instances(instances: &mut Vec<ShapeInstance>, rect: Rect, text: &str) {
    let text_width = text.chars().count() as f32 * CHAR_W;
    let x = rect.x + ((rect.width - text_width) * 0.5).max(0.0);
    let y = rect.y + ((rect.height - CHAR_H) * 0.5).max(0.0);
    draw_text_left(instances, x, y, text, [1.0, 1.0, 1.0, 1.0]);
}
