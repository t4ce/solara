//! wgpu renderer for demoui.html elements (excluding document structure tags).

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod app;
#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod async_utils;
mod geometry;
mod html;
pub(crate) mod input;
#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod loader;
#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod media_store;
#[cfg(any(test, target_os = "trueos", target_os = "zkvm"))]
pub(crate) mod picasso;
#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod renderer;
mod shapes;
mod text;
#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod video;
#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
pub(crate) mod youtube;
#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod youtube_media;

#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
pub(crate) use text::char_width as ui4_char_width;

#[cfg(any(test, target_os = "trueos", target_os = "zkvm"))]
fn clamped_vertical_pan(
    scroll_y: f32,
    drag_dy: i32,
    content_height: f32,
    viewport_height: f32,
) -> f32 {
    let maximum = (content_height - viewport_height).max(0.0);
    (scroll_y - drag_dy as f32).clamp(0.0, maximum)
}

#[cfg(any(test, target_os = "trueos", target_os = "zkvm"))]
pub(crate) fn clamped_pan_origin(origin: u32, drag: i32, canvas: u32, viewport: u32) -> u32 {
    let maximum = canvas.saturating_sub(viewport);
    (i64::from(origin) - i64::from(drag)).clamp(0, i64::from(maximum)) as u32
}

#[cfg(any(test, target_os = "trueos", target_os = "zkvm"))]
pub(crate) fn wheel_scroll_origin(
    origin: u32,
    wheel: i16,
    default_allowed: bool,
    canvas: u32,
    viewport: u32,
) -> u32 {
    const WHEEL_STEP_PX: i32 = 24;
    if !default_allowed {
        return origin;
    }
    clamped_pan_origin(
        origin,
        i32::from(wheel).saturating_mul(WHEEL_STEP_PX),
        canvas,
        viewport,
    )
}

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
pub use app::run;

/// Retained DOM/layout state for the TRUEOS UI4 text viewport.
#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
pub(crate) struct Ui4TextDocument {
    document: html::Document,
    batch: html::RenderBatch,
}

#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
impl Ui4TextDocument {
    pub(crate) fn content_height(&self) -> f32 {
        self.document.content_height
    }

    pub(crate) fn scroll_y(&self) -> f32 {
        self.document.scroll_y
    }

    /// Apply a direct-manipulation vertical drag. Dragging upward reveals
    /// content below; dragging downward moves back toward the document top.
    pub(crate) fn pan_vertical(&mut self, drag_dy: i32, viewport_height: f32) -> bool {
        let before = self.document.scroll_y;
        self.document.scroll_y = clamped_vertical_pan(
            before,
            drag_dy,
            self.document.content_height,
            viewport_height,
        );
        self.document.scroll_y != before
    }

    pub(crate) fn rebuild_scene<'a>(
        &'a mut self,
        scene: &mut picasso::PaintScene,
        canvas: (u32, u32),
        backdrop: trueos_helio_runtime::picasso_scene::Color,
    ) -> Result<picasso::PublicationStats, picasso::BuildError> {
        html::collect_batch(&self.document, 1.0, &mut self.batch);
        scene.rebuild(
            &self.batch.shapes,
            &self.batch.text,
            canvas,
            backdrop,
            self.document.scrollbar_side(),
        )
    }

    pub(crate) const fn scrollbar_side(&self) -> html::ScrollbarSide {
        self.document.scrollbar_side()
    }

    pub(crate) fn set_visual_viewport(&mut self, origin: (u32, u32)) -> Result<(), String> {
        self.document.set_visual_viewport(origin.0, origin.1)
    }

    pub(crate) fn dispatch_mouse(
        &mut self,
        input: input::MouseInput,
    ) -> Result<input::MouseDispatch, String> {
        self.document.dispatch_mouse(input)
    }
}

/// Parse and retain the same bundled document used by the desktop renderer.
#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
pub(crate) fn embedded_ui4_document(page_width: f32) -> Result<Ui4TextDocument, String> {
    ui4_document_for_html(
        include_str!("../../docs/demoui.html"),
        "trueos://solara/docs/demoui.html",
        page_width,
    )
}

/// Parse one caller-supplied document and retain its DOM for UI4 repaints.
#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
pub(crate) fn ui4_document_for_html(
    source: &str,
    source_url: &str,
    page_width: f32,
) -> Result<Ui4TextDocument, String> {
    use rust_qjs_dom::DomEngine;

    let mut engine = DomEngine::new().map_err(|error| format!("failed to start DOM: {error}"))?;
    let artifact = engine
        .parse(source, source_url)
        .map_err(|error| format!("failed to parse document: {error}"))?;
    let document = html::Document::from_dom(artifact, engine, page_width)?;
    Ok(Ui4TextDocument {
        document,
        batch: html::RenderBatch::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::{clamped_pan_origin, clamped_vertical_pan, wheel_scroll_origin};

    #[test]
    fn direct_vertical_pan_follows_drag_and_clamps_to_document() {
        assert_eq!(clamped_vertical_pan(0.0, -120, 2_000.0, 720.0), 120.0);
        assert_eq!(clamped_vertical_pan(120.0, 40, 2_000.0, 720.0), 80.0);
        assert_eq!(clamped_vertical_pan(80.0, 500, 2_000.0, 720.0), 0.0);
        assert_eq!(clamped_vertical_pan(0.0, -5_000, 2_000.0, 720.0), 1_280.0);
        assert_eq!(clamped_vertical_pan(0.0, -100, 500.0, 720.0), 0.0);
    }

    #[test]
    fn retained_canvas_pan_only_moves_on_overflowing_axes() {
        assert_eq!(clamped_pan_origin(0, -120, 2_000, 720), 120);
        assert_eq!(clamped_pan_origin(120, 40, 2_000, 720), 80);
        assert_eq!(clamped_pan_origin(0, -120, 960, 2_560), 0);
        assert_eq!(clamped_pan_origin(560, -120, 2_000, 1_440), 560);
    }

    #[test]
    fn wheel_scroll_uses_dom_direction_and_retained_extent_clamps() {
        assert_eq!(wheel_scroll_origin(0, -1, true, 2_000, 720), 24);
        assert_eq!(wheel_scroll_origin(120, 1, true, 2_000, 720), 96);
        assert_eq!(wheel_scroll_origin(8, 1, true, 2_000, 720), 0);
        assert_eq!(wheel_scroll_origin(1_270, -1, true, 2_000, 720), 1_280);
        assert_eq!(wheel_scroll_origin(0, -1, true, 500, 720), 0);
        assert_eq!(wheel_scroll_origin(120, -1, false, 2_000, 720), 120);
    }
}
