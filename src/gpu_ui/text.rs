use font8x8::UnicodeBasicFonts;

use crate::gpu_ui::draw::put_pixel;
use crate::gpu_ui::layout::Rect;

const FONT: UnicodeBasicFonts = UnicodeBasicFonts::BasicFont;

pub fn draw_text(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    text: &str,
    color: u32,
) {
    let char_width = 8.0;
    let char_height = 8.0;
    let text_width = text.chars().count() as f32 * char_width;
    let mut x = rect.x + ((rect.width - text_width) * 0.5).max(0.0);
    let y = rect.y + ((rect.height - char_height) * 0.5).max(0.0);

    for ch in text.chars() {
        if let Some(glyph) = FONT.get(ch) {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        put_pixel(
                            buffer,
                            width,
                            height,
                            x as i32 + col,
                            y as i32 + row as i32,
                            color,
                        );
                    }
                }
            }
        }
        x += char_width;
    }
}
