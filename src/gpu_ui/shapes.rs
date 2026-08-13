#![cfg_attr(
    not(any(test, target_os = "trueos", target_os = "zkvm")),
    allow(dead_code)
)]

use crate::gpu_ui::geometry::Rect;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeInstance {
    pub pos_size: [f32; 4],
    pub color: [f32; 4],
    pub shape_type: u32,
    /// Backend-neutral shape parameters. Rounded rectangles use `[radius, 0]`;
    /// rounded borders use `[radius, width]`.
    pub parameters: [f32; 2],
}

pub const SHAPE_RECT: u32 = 0;
pub const SHAPE_CIRCLE: u32 = 1;
pub const SHAPE_ROUNDED_RECT: u32 = 2;
pub const SHAPE_ROUNDED_BORDER: u32 = 3;

impl ShapeInstance {
    pub fn rect(rect: Rect, color: [f32; 4]) -> Self {
        Self {
            pos_size: [rect.x, rect.y, rect.width, rect.height],
            color,
            shape_type: SHAPE_RECT,
            parameters: [0.0; 2],
        }
    }

    pub fn circle(center_x: f32, center_y: f32, diameter: f32, color: [f32; 4]) -> Self {
        Self {
            pos_size: [
                center_x - diameter * 0.5,
                center_y - diameter * 0.5,
                diameter,
                diameter,
            ],
            color,
            shape_type: SHAPE_CIRCLE,
            parameters: [diameter * 0.5, 0.0],
        }
    }

    pub fn rounded_rect(rect: Rect, radius: f32, color: [f32; 4]) -> Self {
        Self {
            pos_size: [rect.x, rect.y, rect.width, rect.height],
            color,
            shape_type: SHAPE_ROUNDED_RECT,
            parameters: [radius, 0.0],
        }
    }

    pub fn rounded_border(rect: Rect, radius: f32, width: f32, color: [f32; 4]) -> Self {
        Self {
            pos_size: [rect.x, rect.y, rect.width, rect.height],
            color,
            shape_type: SHAPE_ROUNDED_BORDER,
            parameters: [radius, width],
        }
    }
}

/// Scale layout coordinates (logical points) to physical framebuffer pixels.
pub fn scale_shape_instances(instances: &mut [ShapeInstance], scale: f32) {
    if (scale - 1.0).abs() < f32::EPSILON {
        return;
    }
    for instance in instances {
        instance.pos_size[0] *= scale;
        instance.pos_size[1] *= scale;
        instance.pos_size[2] *= scale;
        instance.pos_size[3] *= scale;
        instance.parameters[0] *= scale;
        instance.parameters[1] *= scale;
    }
}
