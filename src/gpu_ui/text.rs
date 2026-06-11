use font8x8::{BASIC_FONTS, UnicodeFonts};

use crate::gpu_ui::layout::Rect;
use crate::gpu_ui::shapes::{ShapeInstance, SHAPE_RECT};

const CHAR_W: f32 = 8.0;
const CHAR_H: f32 = 8.0;

pub fn append_text_instances(instances: &mut Vec<ShapeInstance>, rect: Rect, text: &str) {
    let text_width = text.chars().count() as f32 * CHAR_W;
    let mut x = rect.x + ((rect.width - text_width) * 0.5).max(0.0);
    let y = rect.y + ((rect.height - CHAR_H) * 0.5).max(0.0);
    let color = [1.0, 1.0, 1.0, 1.0];

    for ch in text.chars() {
        if let Some(glyph) = BASIC_FONTS.get(ch) {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        instances.push(ShapeInstance {
                            pos_size: [x + col as f32, y + row as f32, 1.0, 1.0],
                            color,
                            shape_type: SHAPE_RECT,
                            _pad: 0,
                        });
                    }
                }
            }
        }
        x += CHAR_W;
    }
}
