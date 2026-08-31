//! WGPU and headless retained rendering for Solara HTML documents.

/// TRUEOS's built-in, no-handoff visual verification document.  Keep this URL
/// stable because the headless stylesheet loader below uses it to resolve the
/// bundled relative CSS without granting the resident app arbitrary I/O.
#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
pub(crate) const TEXT_AND_BORDERS_DOCUMENT_URL: &str = "trueos://solara/docs/TextAndBorders.html";

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
const TEXT_AND_BORDERS_STYLESHEET_HREF: &str = "TextAndBorders.css";

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
const TEXT_AND_BORDERS_STYLESHEET_URL: &str = "trueos://solara/docs/TextAndBorders.css";

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
#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
#[cfg_attr(
    all(
        feature = "headless-picasso",
        not(any(test, target_os = "trueos", target_os = "zkvm"))
    ),
    allow(dead_code)
)]
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
pub(crate) use html::ImageRequest;
#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
pub(crate) use text::char_width as ui4_char_width;

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
#[cfg_attr(
    all(
        feature = "headless-picasso",
        not(any(test, target_os = "trueos", target_os = "zkvm"))
    ),
    allow(dead_code)
)]
fn clamped_vertical_pan(
    scroll_y: f32,
    drag_dy: i32,
    content_height: f32,
    viewport_height: f32,
) -> f32 {
    let maximum = (content_height - viewport_height).max(0.0);
    (scroll_y - drag_dy as f32).clamp(0.0, maximum)
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
#[cfg_attr(
    all(
        feature = "headless-picasso",
        not(any(test, target_os = "trueos", target_os = "zkvm"))
    ),
    allow(dead_code)
)]
pub(crate) fn clamped_pan_origin(origin: u32, drag: i32, canvas: u32, viewport: u32) -> u32 {
    let maximum = canvas.saturating_sub(viewport);
    (i64::from(origin) - i64::from(drag)).clamp(0, i64::from(maximum)) as u32
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
#[cfg_attr(
    all(
        feature = "headless-picasso",
        not(any(test, target_os = "trueos", target_os = "zkvm"))
    ),
    allow(dead_code)
)]
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

/// Renderer-independent retained DOM/layout state.
///
/// This is Solara's headless boundary: it owns the DOM projection, CSS-derived
/// layout batch, and the compiler input to Picasso, but never a UI4 frame or a
/// GPU surface.  A presentation backend may consume the resulting scene.
#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
#[allow(dead_code)]
pub(crate) struct HeadlessDocument {
    document: html::Document,
    batch: html::RenderBatch,
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
#[allow(dead_code)]
impl HeadlessDocument {
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

    pub(crate) fn rebuild_scene(
        &mut self,
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
            self.document.resize_handle_enabled(),
        )
    }

    /// Run the document-selected first JavaScript feature step before a
    /// presentation backend observes the next retained Picasso publication.
    #[cfg(feature = "sandboxed-scene-js")]
    pub(crate) fn execute_opt_in_inline_scene_scripts(
        &mut self,
    ) -> Result<html::SceneScriptReport, String> {
        self.document
            .execute_opt_in_inline_scene_scripts(html::SceneScriptBudget::default())
    }

    pub(crate) const fn scrollbar_side(&self) -> html::ScrollbarSide {
        self.document.scrollbar_side()
    }

    pub(crate) const fn resize_handle_enabled(&self) -> bool {
        self.document.resize_handle_enabled()
    }

    pub(crate) fn image_requests(&self) -> Vec<html::ImageRequest> {
        self.document.image_requests()
    }

    pub(crate) fn set_visual_viewport(
        &mut self,
        origin: (u32, u32),
        zoom_percent: u32,
    ) -> Result<(), String> {
        self.document
            .set_visual_viewport(origin.0, origin.1, zoom_percent)
    }

    pub(crate) fn dispatch_mouse(
        &mut self,
        input: input::MouseInput,
    ) -> Result<input::MouseDispatch, String> {
        self.document.dispatch_mouse(input)
    }
}

/// Parse and retain the same bundled document used by the desktop renderer.
#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
#[allow(dead_code)]
pub(crate) fn embedded_headless_document(page_width: f32) -> Result<HeadlessDocument, String> {
    headless_document_for_html(
        include_str!("../../docs/TextAndBorders.html"),
        TEXT_AND_BORDERS_DOCUMENT_URL,
        page_width,
    )
}

/// Parse one caller-supplied document and retain its DOM for later scene
/// compilation.  This has no window, frame lease, or presentation side effect.
#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
#[allow(dead_code)]
pub(crate) fn headless_document_for_html(
    source: &str,
    source_url: &str,
    page_width: f32,
) -> Result<HeadlessDocument, String> {
    use rust_qjs_dom::{DomEngine, LoadedStylesheet};

    let mut engine = DomEngine::with_stylesheet_loader(|document_url, _base_href, href| {
        if document_url == TEXT_AND_BORDERS_DOCUMENT_URL && href == TEXT_AND_BORDERS_STYLESHEET_HREF
        {
            return Ok(LoadedStylesheet::new(
                TEXT_AND_BORDERS_STYLESHEET_URL,
                include_str!("../../docs/TextAndBorders.css"),
            ));
        }
        Err(format!(
            "external stylesheet {href:?} is not bundled for headless source {document_url:?}"
        ))
    })
    .map_err(|error| format!("failed to start DOM: {error}"))?;
    let artifact = engine
        .parse(source, source_url)
        .map_err(|error| format!("failed to parse document: {error}"))?;
    let document = html::Document::from_dom(artifact, engine, page_width)?;
    Ok(HeadlessDocument {
        document,
        batch: html::RenderBatch::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        TEXT_AND_BORDERS_DOCUMENT_URL, clamped_pan_origin, clamped_vertical_pan,
        embedded_headless_document, wheel_scroll_origin,
    };

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

    #[test]
    fn bundled_trueos_fixture_loads_its_relative_stylesheet() {
        let document = embedded_headless_document(960.0).expect("bundled fixture parses");
        let dom = document.document.dom();
        assert_eq!(dom.source.url, TEXT_AND_BORDERS_DOCUMENT_URL);
        assert_eq!(dom.style_index.external_stylesheet_count, 1);
        assert!(dom.style_index.load_errors.is_empty());
        assert!(document.content_height() >= 720.0);

        let sample = dom
            .document
            .find_element_by_id("sample-18")
            .expect("outline sample");
        let style = dom
            .style_index
            .style(sample.style_ref.expect("style ref"))
            .expect("computed style");
        assert!(style.cascaded_declarations.contains_key("outline"));
        assert_eq!(
            style.cascaded_declarations.get("position"),
            Some(&String::from("absolute"))
        );
    }

    #[cfg(feature = "headless-picasso")]
    #[test]
    fn headless_document_compiles_a_picasso_scene_without_a_frame() {
        let mut document = super::headless_document_for_html(
            "<style>h1 { color: #123456; }</style><main><h1>Headless scene</h1></main>",
            "https://solara.test/headless",
            640.0,
        )
        .expect("headless document parses");
        let mut scene = super::picasso::PaintScene::new();
        let publication = document
            .rebuild_scene(
                &mut scene,
                (640, 480),
                trueos_helio_runtime::picasso_scene::Color::rgba(250, 250, 250, 255),
            )
            .expect("headless document publishes a scene");

        assert!(publication.logical_rows >= 2);
        assert!(publication.font_lookup_rows >= 1);
        assert!(
            !scene
                .lower((0, 0), (640, 480))
                .expect("scene lowers for a later presentation backend")
                .is_empty()
        );
    }
}
