use crate::gpu_ui::draw::{fill_rect, color_to_pixel};
use crate::gpu_ui::layout::{Button, Rect};
use crate::gpu_ui::text::draw_text;

pub fn draw_button(buffer: &mut [u32], width: u32, height: u32, button: &Button) {
    let style = &button.style;
    let border = button.border_rect;
    let fill = button.fill;

    let border_color = [fill[0] * 0.55, fill[1] * 0.55, fill[2] * 0.55, 1.0];
    let face_color = [fill[0], fill[1], fill[2], 1.0];

    fill_rect(buffer, width, height, border, border_color);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: border.x + style.border.left,
            y: border.y + style.border.top,
            width: border.width - style.border.horizontal(),
            height: border.height - style.border.vertical(),
        },
        face_color,
    );

    draw_text(
        buffer,
        width,
        height,
        button.content_rect,
        &button.label,
        color_to_pixel(1.0, 1.0, 1.0),
    );
}

pub fn circle_contains(center_x: f32, center_y: f32, diameter: f32, px: f32, py: f32) -> bool {
    let radius = diameter * 0.5;
    let dx = px - center_x;
    let dy = py - center_y;
    dx * dx + dy * dy <= radius * radius
}
