use crate::gpu_ui::layout::Rect;

pub fn color_to_pixel(r: f32, g: f32, b: f32) -> u32 {
    let r = (r.clamp(0.0, 1.0) * 255.0) as u32;
    let g = (g.clamp(0.0, 1.0) * 255.0) as u32;
    let b = (b.clamp(0.0, 1.0) * 255.0) as u32;
    (r << 16) | (g << 8) | b
}

pub fn clear(buffer: &mut [u32], color: u32) {
    buffer.fill(color);
}

pub fn put_pixel(buffer: &mut [u32], width: u32, height: u32, x: i32, y: i32, color: u32) {
    if x < 0 || y < 0 {
        return;
    }
    let x = x as u32;
    let y = y as u32;
    if x < width && y < height {
        buffer[(y * width + x) as usize] = color;
    }
}

pub fn fill_rect(buffer: &mut [u32], width: u32, height: u32, rect: Rect, color: [f32; 4]) {
    let pixel = color_to_pixel(color[0], color[1], color[2]);
    let x0 = rect.x.max(0.0).floor() as i32;
    let y0 = rect.y.max(0.0).floor() as i32;
    let x1 = (rect.x + rect.width).min(width as f32).ceil() as i32;
    let y1 = (rect.y + rect.height).min(height as f32).ceil() as i32;

    for y in y0..y1 {
        for x in x0..x1 {
            put_pixel(buffer, width, height, x, y, pixel);
        }
    }
}

pub fn fill_circle(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    center_x: f32,
    center_y: f32,
    diameter: f32,
    color: [f32; 4],
) {
    let pixel = color_to_pixel(color[0], color[1], color[2]);
    let radius = diameter * 0.5;
    let radius_sq = radius * radius;
    let min_x = (center_x - radius).floor() as i32;
    let max_x = (center_x + radius).ceil() as i32;
    let min_y = (center_y - radius).floor() as i32;
    let max_y = (center_y + radius).ceil() as i32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 + 0.5 - center_x;
            let dy = y as f32 + 0.5 - center_y;
            if dx * dx + dy * dy <= radius_sq {
                put_pixel(buffer, width, height, x, y, pixel);
            }
        }
    }
}

pub fn fill_triangle(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    c0: [f32; 4],
    c1: [f32; 4],
    c2: [f32; 4],
) {
    let min_x = p0.0.min(p1.0).min(p2.0).floor() as i32;
    let max_x = p0.0.max(p1.0).max(p2.0).ceil() as i32;
    let min_y = p0.1.min(p1.1).min(p2.1).floor() as i32;
    let max_y = p0.1.max(p1.1).max(p2.1).ceil() as i32;

    let area = edge(p0, p1, p2);
    if area.abs() < f32::EPSILON {
        return;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let point = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge(p1, p2, point) / area;
            let w1 = edge(p2, p0, point) / area;
            let w2 = edge(p0, p1, point) / area;

            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let r = c0[0] * w0 + c1[0] * w1 + c2[0] * w2;
                let g = c0[1] * w0 + c1[1] * w1 + c2[1] * w2;
                let b = c0[2] * w0 + c1[2] * w1 + c2[2] * w2;
                put_pixel(buffer, width, height, x, y, color_to_pixel(r, g, b));
            }
        }
    }
}

fn edge(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> f32 {
    (p.0 - a.0) * (b.1 - a.1) - (p.1 - a.1) * (b.0 - a.0)
}

pub fn draw_triangle_demo(buffer: &mut [u32], width: u32, height: u32) {
    fill_triangle(
        buffer,
        width,
        height,
        (24.0, 24.0),
        (124.0, 24.0),
        (74.0, 110.0),
        [1.0, 0.2, 0.3, 1.0],
        [0.2, 1.0, 0.4, 1.0],
        [0.3, 0.5, 1.0, 1.0],
    );
}
