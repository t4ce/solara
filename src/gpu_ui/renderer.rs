pub use solara_wgpu_shim::{RenderError, Renderer, RendererContext};

#[cfg(not(feature = "gpu-text-only"))]
use crate::gpu_ui::shapes::ShapeInstance;
use crate::gpu_ui::text::TextSection;

#[cfg(not(feature = "gpu-text-only"))]
impl solara_wgpu_shim::Shape for ShapeInstance {
    fn position_size(&self) -> [f32; 4] {
        self.pos_size
    }

    fn color(&self) -> [f32; 4] {
        self.color
    }

    fn kind(&self) -> u32 {
        self.shape_type
    }
}

impl solara_wgpu_shim::TextRun for TextSection {
    fn position(&self) -> (f32, f32) {
        (self.x, self.y)
    }

    fn bounds(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn color(&self) -> [f32; 4] {
        self.color
    }

    fn scale(&self) -> f32 {
        crate::gpu_ui::text::metrics(self.font_size).glyph_scale
    }
}
