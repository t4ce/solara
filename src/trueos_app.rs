//! TRUEOS Blueprint entry point for Solara's retained Picasso/UI4 pass.

use trueos_helio_runtime::picasso_scene::{
    Color, FontFace as SceneFontFace, FontSlant, ImageResourceRef, LoweredCommand,
};

use trueos::ui4_scene::{
    CursorIcon, CursorSource, Damage, Error, Font, FontCanvasRow, Frame, POINTER_BUTTON_MIDDLE,
    POINTER_BUTTON_PRIMARY, POINTER_BUTTON_SECONDARY, PanPhase, PointerEvent, SpriteCorner,
    SpriteQuad, output_dimensions,
};

use crate::gpu_ui::input::{MouseEventKind, MouseInput};

const FRAME_WIDTH: u32 = 960;
const FRAME_HEIGHT: u32 = 720;
const LAUNCH_SCRIPT_PATH: &[u8] = b"vFile:launch";
const SURF_LAUNCH_HEADER: &str = "solara-surf-v1";
const HANDOFF_ROOT: &str = "/common/solara/surf";
const RESIDENT_POLL_MS: u64 = 16;
const PRESENT_RETRY_MS: u64 = 2;
// FontKernel's current retained-canvas contract admits 4096 pixels per axis.
// Longer documents will become tiled resources; V1 retains the complete demo.
const TEXT_CANVAS_MAX_WIDTH: u32 = 3_556;
const TEXT_CANVAS_MAX_HEIGHT: u32 = 4_096;
const FONT_CANVAS_MAX_ROWS: usize = 256;
const TEXT_ROW_MAX_BYTES: usize = 1_024;
const TEXT_CANVAS_MAX_GLYPHS: usize = 4_096;
const ZOOM_MIN_PERCENT: u32 = 10;
const ZOOM_MAX_PERCENT: u32 = 500;
const ZOOM_STEP_PERCENT: u32 = 10;
const MIN_FRAME_WIDTH: u32 = 320;
const MIN_FRAME_HEIGHT: u32 = 240;
const BACKGROUND: Color = Color::rgba(250, 250, 250, 255);
// Solara lays out every current text run with the same monospaced metrics as
// its bundled desktop face. Keep the UI4 scene on that face as well instead
// of repainting those coordinates with the proportional legacy default.
const SCENE_FONT: Font = Font::Inconsolata;

struct RenderDocument {
    source: Option<String>,
    source_url: String,
    handoff_path: Option<String>,
}

struct SurfLaunch {
    tag: String,
    source_url: String,
}

struct SolaraView {
    frame: Frame,
    document: crate::gpu_ui::HeadlessDocument,
    paint_scene: crate::gpu_ui::picasso::PaintScene,
    canvas: (u32, u32),
    origin: (u32, u32),
    zoom_percent: u32,
    active_pan_source: Option<CursorSource>,
    resize_drag: Option<ResizeDrag>,
    font_canvas_dirty: bool,
    needs_present: bool,
}

#[derive(Clone, Copy)]
struct ResizeDrag {
    source: CursorSource,
    start_screen: (u32, u32),
    start_extent: (u32, u32),
    maximum_extent: (u32, u32),
    pending_extent: (u32, u32),
}

struct PendingImage {
    request: crate::gpu_ui::ImageRequest,
    resource: ImageResourceRef,
}

struct OwnedTextRow {
    text: String,
    x: f32,
    y: f32,
    font_pixels: f32,
    color_rgba: u32,
}

struct TextCanvasStats {
    rows: usize,
    glyphs: usize,
    compatibility_style_mismatches: usize,
}

impl SolaraView {
    fn logical_viewport(&self) -> (u32, u32) {
        crate::gpu_ui::picasso::logical_viewport_extent(
            (self.frame.width(), self.frame.height()),
            self.zoom_percent,
        )
    }

    fn clamp_origin(&mut self) -> bool {
        let previous = self.origin;
        let logical_viewport = self.logical_viewport();
        self.origin.0 = self
            .origin
            .0
            .min(self.canvas.0.saturating_sub(logical_viewport.0));
        self.origin.1 = self
            .origin
            .1
            .min(self.canvas.1.saturating_sub(logical_viewport.1));
        self.origin != previous
    }

    fn sync_dom_viewport(&mut self) {
        if let Err(error) = self
            .document
            .set_visual_viewport(self.origin, self.zoom_percent)
        {
            let message = format!("solara: QuickJS viewport sync failed: {error}\n");
            trueos::vsys::write_err(message.as_bytes());
        }
    }

    fn resize_frame(&mut self, width: u32, height: u32, reason: &str) -> Result<(), Error> {
        if (width, height) == (self.frame.width(), self.frame.height()) {
            return Ok(());
        }
        let previous = (self.frame.width(), self.frame.height());
        retry_busy(|| self.frame.resize(width, height))?;
        self.needs_present = true;
        let _ = self.clamp_origin();
        self.sync_dom_viewport();
        let message = format!(
            "solara: frame resize accepted reason={reason} old={}x{} new={}x{} zoom={}%% projection=top-center-1to1-at-100\n",
            previous.0, previous.1, width, height, self.zoom_percent,
        );
        trueos::vsys::write_out(message.as_bytes());
        Ok(())
    }

    fn ensure_font_canvas(&mut self) -> Result<(), Error> {
        if !self.font_canvas_dirty {
            return Ok(());
        }
        let _ = build_text_canvas(&mut self.frame, self.canvas, &self.paint_scene)?;
        self.font_canvas_dirty = false;
        Ok(())
    }

    fn take_view_updates(&mut self) -> Result<(), Error> {
        while let Some(event) = self.frame.take_resize_event()? {
            if event.width == self.frame.width() && event.height == self.frame.height() {
                continue;
            }
            self.resize_frame(event.width, event.height, "ui4-maximize-or-restore")?;
        }
        while let Some(event) = self.frame.take_pan_event()? {
            match event.phase {
                PanPhase::Begin
                    if self.active_pan_source.is_none()
                        || self.active_pan_source == Some(event.source) =>
                {
                    self.active_pan_source = Some(event.source);
                }
                PanPhase::Update if self.active_pan_source == Some(event.source) => {
                    let logical_dx = physical_delta_to_logical(event.dx, self.zoom_percent);
                    let logical_dy = physical_delta_to_logical(event.dy, self.zoom_percent);
                    let next = (
                        crate::gpu_ui::clamped_pan_origin(
                            self.origin.0,
                            logical_dx,
                            self.canvas.0,
                            self.logical_viewport().0,
                        ),
                        crate::gpu_ui::clamped_pan_origin(
                            self.origin.1,
                            logical_dy,
                            self.canvas.1,
                            self.logical_viewport().1,
                        ),
                    );
                    if next != self.origin {
                        self.origin = next;
                        self.needs_present = true;
                        self.sync_dom_viewport();
                    }
                }
                PanPhase::End if self.active_pan_source == Some(event.source) => {
                    self.active_pan_source = None;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn take_mouse_updates(&mut self) -> Result<(), Error> {
        while let Some(event) = self.frame.take_pointer_event()? {
            let modifiers = self
                .frame
                .input_routes()?
                .into_iter()
                .find(|route| route.cursor == event.source && route.combo_id == event.combo_id)
                .and_then(|route| route.keyboard)
                .map(|keyboard| keyboard.modifiers)
                .unwrap_or(0);
            if self.handle_resize_pointer(event)? {
                continue;
            }
            let (client, page) = self.pointer_css_coordinates(event);
            let movement = (
                physical_delta_to_logical(event.dx, self.zoom_percent) as f32,
                physical_delta_to_logical(event.dy, self.zoom_percent) as f32,
            );
            let wheel_default = dispatch_pointer_event(
                &mut self.document,
                event,
                modifiers,
                client,
                page,
                movement,
            );
            if event.wheel != 0 && modifiers & 0x11 != 0 && wheel_default {
                self.zoom_at(event.local_x, event.local_y, event.wheel);
                continue;
            }
            let logical_viewport = self.logical_viewport();
            let next = crate::gpu_ui::wheel_scroll_origin(
                self.origin.1,
                event.wheel,
                wheel_default,
                self.canvas.1,
                logical_viewport.1,
            );
            if next != self.origin.1 {
                self.origin.1 = next;
                self.needs_present = true;
                self.sync_dom_viewport();
            }
        }
        Ok(())
    }

    fn pointer_css_coordinates(&self, event: PointerEvent) -> ((f32, f32), (f32, f32)) {
        let scale = self.zoom_percent as f32 / 100.0;
        let scaled_width = (self.canvas.0 as f32 * scale).ceil() as u32;
        let x_offset = self.frame.width().saturating_sub(scaled_width) / 2;
        let client = (
            (event.local_x as f32 - x_offset as f32) / scale,
            event.local_y as f32 / scale,
        );
        let page = (
            client.0 + self.origin.0 as f32,
            client.1 + self.origin.1 as f32,
        );
        (client, page)
    }

    fn zoom_at(&mut self, local_x: i32, local_y: i32, wheel: i16) {
        let next = if wheel > 0 {
            self.zoom_percent.saturating_add(ZOOM_STEP_PERCENT)
        } else {
            self.zoom_percent.saturating_sub(ZOOM_STEP_PERCENT)
        }
        .clamp(ZOOM_MIN_PERCENT, ZOOM_MAX_PERCENT);
        if next == self.zoom_percent {
            return;
        }
        let old_zoom = self.zoom_percent;
        let old_scale = old_zoom as f64 / 100.0;
        let old_width = (self.canvas.0 as f64 * old_scale).ceil() as u32;
        let old_offset = self.frame.width().saturating_sub(old_width) / 2;
        let anchor_x =
            f64::from(self.origin.0) + (f64::from(local_x) - f64::from(old_offset)) / old_scale;
        let anchor_y = f64::from(self.origin.1) + f64::from(local_y) / old_scale;

        self.zoom_percent = next;
        let new_scale = next as f64 / 100.0;
        let new_width = (self.canvas.0 as f64 * new_scale).ceil() as u32;
        let new_offset = self.frame.width().saturating_sub(new_width) / 2;
        self.origin = (
            (anchor_x - (f64::from(local_x) - f64::from(new_offset)) / new_scale)
                .round()
                .max(0.0) as u32,
            (anchor_y - f64::from(local_y) / new_scale).round().max(0.0) as u32,
        );
        let _ = self.clamp_origin();
        self.needs_present = true;
        self.sync_dom_viewport();
        let message = format!(
            "solara: viewport zoom old={}%% new={}%% origin={},{} frame={}x{}\n",
            old_zoom,
            next,
            self.origin.0,
            self.origin.1,
            self.frame.width(),
            self.frame.height(),
        );
        trueos::vsys::write_out(message.as_bytes());
    }

    fn handle_resize_pointer(&mut self, event: PointerEvent) -> Result<bool, Error> {
        if !self.document.resize_handle_enabled() {
            return Ok(false);
        }
        if let Some(mut drag) = self.resize_drag {
            if drag.source != event.source {
                return Ok(false);
            }
            drag.pending_extent = resize_drag_extent(drag, event.x, event.y);
            self.resize_drag = Some(drag);
            if event.buttons_released & POINTER_BUTTON_PRIMARY != 0 {
                self.resize_drag = None;
                self.frame
                    .set_cursor_icon_for(event.source, CursorIcon::Default)?;
                self.resize_frame(
                    drag.pending_extent.0,
                    drag.pending_extent.1,
                    "dom-bottom-right-handle",
                )?;
            }
            return Ok(true);
        }

        let over_handle = resize_handle_contains(
            event.local_x,
            event.local_y,
            self.frame.width(),
            self.frame.height(),
        );
        self.frame.set_cursor_icon_for(
            event.source,
            if over_handle {
                CursorIcon::ResizeDiagonal
            } else {
                CursorIcon::Default
            },
        )?;
        if over_handle && event.buttons_pressed & POINTER_BUTTON_PRIMARY != 0 {
            let maximum_extent = output_dimensions()?;
            self.resize_drag = Some(ResizeDrag {
                source: event.source,
                start_screen: (event.x, event.y),
                start_extent: (self.frame.width(), self.frame.height()),
                maximum_extent,
                pending_extent: (self.frame.width(), self.frame.height()),
            });
            return Ok(true);
        }
        Ok(false)
    }

    fn present_viewport(&mut self) -> Result<usize, Error> {
        if self.clamp_origin() {
            self.sync_dom_viewport();
        }
        self.ensure_font_canvas()?;
        let commands = self
            .paint_scene
            .lower_zoomed(
                self.origin,
                (self.frame.width(), self.frame.height()),
                self.zoom_percent,
            )
            .map_err(|error| invalid_picasso("viewport lower", error))?;
        let mut quads = Vec::with_capacity(commands.len());
        for command in commands {
            match command {
                // SceneDB retains the canonical document backdrop at order 0,
                // while begin_sprite_frame performs the identical full-frame
                // clear. Keep the row in the scene shadow, but do not make the
                // GPU paint the same background twice on every viewport move.
                LoweredCommand::SolidSpan {
                    order: 0, color, ..
                } if color == BACKGROUND => {}
                LoweredCommand::SolidSpan {
                    x,
                    y,
                    width,
                    height,
                    color,
                    ..
                } => quads.push(solid_quad(x, y, width, height, color.to_u32_le())),
                LoweredCommand::FontCanvas {
                    x,
                    y,
                    source_x,
                    source_y,
                    width,
                    height,
                    ..
                } => quads.push(font_canvas_quad(
                    &self.frame,
                    self.canvas,
                    (x, y),
                    (source_x, source_y),
                    (width, height),
                    (
                        zoomed_extent(width, self.zoom_percent),
                        zoomed_extent(height, self.zoom_percent),
                    ),
                )?),
                // Shadow lookup rows are retained for FontKernel resolution;
                // FontCanvas remains the sole visual text pass until UI4 can
                // prove mixed sprite/retained-text release ordering.
                LoweredCommand::FontLookup { .. } => {}
                LoweredCommand::Image {
                    x,
                    y,
                    width,
                    height,
                    source_x,
                    source_y,
                    source_width,
                    source_height,
                    resource,
                    opacity,
                    ..
                } => {
                    let image = self
                        .paint_scene
                        .resolve_image(resource)
                        .ok_or(Error::Invalid)?;
                    quads.push(image_quad(
                        image.sprite_id,
                        (image.width, image.height),
                        (u32::from(x), u32::from(y)),
                        (u32::from(width), u32::from(height)),
                        (u32::from(source_x), u32::from(source_y)),
                        (u32::from(source_width), u32::from(source_height)),
                        opacity,
                    )?);
                }
            }
        }
        retry_busy(|| self.frame.begin_sprite_frame(BACKGROUND.to_u32_le()))?;
        self.frame.draw_sprite_quads(quads.as_slice())?;
        retry_busy(|| {
            self.frame
                .publish(Damage::full(self.frame.width(), self.frame.height()))
        })?;
        self.needs_present = false;
        Ok(quads.len())
    }
}

pub(crate) fn run() -> ! {
    match trueos::async_fs::block_on(present_scene_frame()) {
        Ok(view) => {
            trueos::vsys::write_out(
                b"solara: headless DOM-to-Picasso scene + compatibility FontKernel canvas visible; wheel/middle-pan, Ctrl+wheel zoom, and DOM-configured scrollbar/resize handle active\n",
            );
            resident_view_loop(view)
        }
        Err(error) => {
            let message = format!("solara: Picasso/UI4 frame failed: {error:?}\n");
            trueos::vsys::write_err(message.as_bytes());
            loop {
                trueos::vsys::sleep_ms(250);
            }
        }
    }
}

fn resident_view_loop(mut view: SolaraView) -> ! {
    let mut visible_logged = false;
    let mut presentation_error_logged = false;
    loop {
        if !visible_logged && view.frame.take_first_presentation().unwrap_or(false) {
            visible_logged = true;
            let message = format!(
                "solara: dirty frame visible window={}\n",
                view.frame.window_id(),
            );
            trueos::vsys::write_out(message.as_bytes());
        }
        match view.take_view_updates() {
            Ok(()) | Err(Error::Busy) => {}
            Err(error) => {
                let message = format!("solara: UI4 pan/resize input failed: {error:?}\n");
                trueos::vsys::write_err(message.as_bytes());
            }
        }
        match view.take_mouse_updates() {
            Ok(()) | Err(Error::Busy) => {}
            Err(error) => {
                let message = format!("solara: UI4 mouse input failed: {error:?}\n");
                trueos::vsys::write_err(message.as_bytes());
            }
        }
        if view.needs_present {
            match view.present_viewport() {
                Ok(_) => presentation_error_logged = false,
                Err(Error::Busy) => {}
                Err(error) if !presentation_error_logged => {
                    presentation_error_logged = true;
                    let message = format!("solara: viewport presentation failed: {error:?}\n");
                    trueos::vsys::write_err(message.as_bytes());
                }
                Err(_) => {}
            }
        }
        trueos::vsys::sleep_ms(RESIDENT_POLL_MS);
    }
}

async fn present_scene_frame() -> Result<SolaraView, Error> {
    // Build the DOM once in logical page coordinates and materialize one warm
    // GPU canvas. Pan and maximize only change the crop composed into UI4.
    let document = render_document().await?;
    let mut headless_document = match document.source.as_deref() {
        Some(source) => crate::gpu_ui::headless_document_for_html(
            source,
            document.source_url.as_str(),
            FRAME_WIDTH as f32,
        ),
        None => crate::gpu_ui::embedded_headless_document(FRAME_WIDTH as f32),
    }
    .map_err(|error| {
        trueos::vsys::write_err(b"solara: DOM scene build failed: ");
        trueos::vsys::write_err(error.as_bytes());
        trueos::vsys::write_err(b"\n");
        Error::Invalid
    })?;
    #[cfg(feature = "sandboxed-scene-js")]
    let script_step = match headless_document.execute_opt_in_inline_scene_scripts() {
        Ok(report) => (report.scripts, report.patches),
        Err(error) => {
            let message = format!(
                "solara: sandboxed scene-script step rejected; preserving static CSS scene: {error}\n"
            );
            trueos::vsys::write_err(message.as_bytes());
            (0, 0)
        }
    };
    #[cfg(not(feature = "sandboxed-scene-js"))]
    let script_step = (0usize, 0usize);
    let content_height = headless_document.content_height();
    if content_height.ceil() > TEXT_CANVAS_MAX_HEIGHT as f32 {
        let message = format!(
            "solara: document height {:.1} exceeds retained V1 canvas {}; lower content is clipped until text tiling lands\n",
            content_height, TEXT_CANVAS_MAX_HEIGHT,
        );
        trueos::vsys::write_err(message.as_bytes());
    }
    let canvas = (
        FRAME_WIDTH.min(TEXT_CANVAS_MAX_WIDTH),
        (content_height.ceil() as u32).clamp(1, TEXT_CANVAS_MAX_HEIGHT),
    );
    let image_requests = headless_document.image_requests();
    let mut paint_scene = crate::gpu_ui::picasso::PaintScene::new();
    let mut pending_images = Vec::with_capacity(image_requests.len());
    for request in image_requests {
        let resource = paint_scene
            .request_image(&request)
            .map_err(|error| invalid_picasso("image resource request", error))?;
        pending_images.push(PendingImage { request, resource });
    }
    let mut view = SolaraView {
        frame: Frame::open(160, 180, FRAME_WIDTH, FRAME_HEIGHT)?,
        document: headless_document,
        paint_scene,
        canvas,
        origin: (0, 0),
        zoom_percent: 100,
        active_pan_source: None,
        resize_drag: None,
        font_canvas_dirty: false,
        needs_present: true,
    };
    let mut scene_stats = view
        .document
        .rebuild_scene(&mut view.paint_scene, view.canvas, BACKGROUND)
        .map_err(|error| invalid_picasso("DOM scene publication", error))?;
    let stats = build_text_canvas(&mut view.frame, view.canvas, &view.paint_scene)?;
    view.sync_dom_viewport();
    let mut lowered_commands = view.present_viewport()?;
    let (loaded_images, updated_scene_stats) =
        load_pending_images(&mut view, pending_images.as_slice()).await;
    if let Some(updated_scene_stats) = updated_scene_stats {
        scene_stats = updated_scene_stats;
    }
    if loaded_images != 0 {
        lowered_commands = view.present_viewport()?;
    }
    if let Some(path) = document.handoff_path.as_deref() {
        match trueos::async_fs::remove(path.as_bytes()).await {
            Ok(()) => {
                let message = format!("solara: consumed one-shot HTML handoff {path}\n");
                trueos::vsys::write_out(message.as_bytes());
            }
            Err(code) => {
                let message = format!("solara: could not consume HTML handoff {path}: {code}\n");
                trueos::vsys::write_err(message.as_bytes());
            }
        }
    }
    let summary = format!(
        "solara: Picasso scene epoch={} logical_rows={} shape_rows={} font_lookup_rows={} image_rows={} loaded_images={} lowered_commands={} text_rows={} glyphs={} fallback_style_mismatches={} sandboxed_scripts={} sandboxed_patches={} viewport={}x{} canvas={}x{} content_height={:.1} scrollbar={} resize_handle={} zoom={}%% font=inconsolata source={}\n",
        scene_stats.epoch,
        scene_stats.logical_rows,
        scene_stats.shape_rows,
        scene_stats.font_lookup_rows,
        scene_stats.image_rows,
        loaded_images,
        lowered_commands,
        stats.rows,
        stats.glyphs,
        stats.compatibility_style_mismatches,
        script_step.0,
        script_step.1,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        view.canvas.0,
        view.canvas.1,
        content_height,
        view.document.scrollbar_side().as_str(),
        u8::from(view.document.resize_handle_enabled()),
        view.zoom_percent,
        document.source_url,
    );
    trueos::vsys::write_out(summary.as_bytes());
    Ok(view)
}

async fn load_pending_images(
    view: &mut SolaraView,
    pending: &[PendingImage],
) -> (usize, Option<crate::gpu_ui::picasso::PublicationStats>) {
    let mut loaded = 0usize;
    let mut latest_scene_stats = None;
    for pending in pending {
        let result = fetch_and_decode_image(pending.request.source_url.as_str()).await;
        let decoded = match result {
            Ok(decoded) => decoded,
            Err(error) => {
                view.paint_scene.image_failed(pending.resource);
                let message = format!(
                    "solara: asset failed node={} source={} error={}\n",
                    pending.request.node_id, pending.request.source_url, error,
                );
                trueos::vsys::write_err(message.as_bytes());
                continue;
            }
        };
        let Some(sprite_id) = pending
            .resource
            .0
            .slot
            .checked_add(1)
            .filter(|id| *id != u32::MAX)
        else {
            view.paint_scene.image_failed(pending.resource);
            continue;
        };
        if let Err(error) = view.frame.upload_sprite_rgba8(
            sprite_id,
            decoded.info.width,
            decoded.info.height,
            decoded.rgba.as_slice(),
        ) {
            view.paint_scene.image_failed(pending.resource);
            let message = format!(
                "solara: asset sprite upload failed node={} sprite={} error={error:?}\n",
                pending.request.node_id, sprite_id,
            );
            trueos::vsys::write_err(message.as_bytes());
            continue;
        }
        let ready = match view.paint_scene.image_ready(
            pending.resource,
            decoded.info.width,
            decoded.info.height,
        ) {
            Ok(ready) => ready,
            Err(error) => {
                view.paint_scene.image_failed(pending.resource);
                let message = format!(
                    "solara: asset SceneDB transition failed node={} error={error:?}\n",
                    pending.request.node_id,
                );
                trueos::vsys::write_err(message.as_bytes());
                continue;
            }
        };
        let scene_stats =
            match view
                .document
                .rebuild_scene(&mut view.paint_scene, view.canvas, BACKGROUND)
            {
                Ok(scene_stats) => scene_stats,
                Err(error) => {
                    view.paint_scene.image_failed(pending.resource);
                    let message = format!(
                        "solara: asset Picasso publication failed node={} error={error:?}\n",
                        pending.request.node_id,
                    );
                    trueos::vsys::write_err(message.as_bytes());
                    continue;
                }
            };
        latest_scene_stats = Some(scene_stats);
        view.needs_present = true;
        loaded = loaded.saturating_add(1);
        let message = format!(
            "solara: asset SceneDB ready node={} resource={}:{} revision={} sprite={} intrinsic={}x{} backend={:?} source={}\n",
            pending.request.node_id,
            ready.resource.0.slot,
            ready.resource.0.generation,
            ready.revision,
            ready.sprite_id,
            ready.width,
            ready.height,
            decoded.info.backend,
            pending.request.source_url,
        );
        trueos::vsys::write_out(message.as_bytes());
    }
    (loaded, latest_scene_stats)
}

async fn fetch_and_decode_image(source: &str) -> Result<trueos::vmedia::DecodedImage, String> {
    const FETCH_TIMEOUT_MS: u64 = 30_000;
    const MAX_ENCODED_BYTES: usize = 16 * 1024 * 1024;
    if !(source.starts_with("https://") || source.starts_with("http://")) {
        return Err(String::from("V1 accepts HTTP(S) image assets only"));
    }
    let operation = trueos::netfs::fetch_bytes(source.as_bytes())
        .map_err(|code| format!("fetch start code={code}"))?;
    let wait = trueos::netfs::fetch_bytes_wait(operation, FETCH_TIMEOUT_MS);
    if wait != 0 {
        let _ = trueos::netfs::fetch_bytes_discard(operation);
        return Err(format!("fetch wait code={wait}"));
    }
    let encoded = trueos::netfs::fetch_bytes_read(operation)
        .map_err(|code| format!("fetch read code={code}"));
    let _ = trueos::netfs::fetch_bytes_discard(operation);
    let encoded = encoded?;
    if encoded.is_empty() || encoded.len() > MAX_ENCODED_BYTES {
        return Err(format!("encoded length rejected bytes={}", encoded.len()));
    }
    let format = infer_image_format(encoded.as_slice())
        .ok_or_else(|| String::from("unsupported raster signature"))?;
    trueos::vmedia::decode(format, encoded.as_slice())
        .await
        .map_err(|code| format!("vmedia decode code={code}"))
}

fn infer_image_format(bytes: &[u8]) -> Option<trueos::vmedia::ImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(trueos::vmedia::ImageFormat::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(trueos::vmedia::ImageFormat::Jpeg)
    } else if bytes.starts_with(b"BM") {
        Some(trueos::vmedia::ImageFormat::Bmp)
    } else {
        None
    }
}

fn dispatch_pointer_event(
    document: &mut crate::gpu_ui::HeadlessDocument,
    event: PointerEvent,
    modifiers: u8,
    client: (f32, f32),
    page: (f32, f32),
    movement: (f32, f32),
) -> bool {
    let base = |kind| {
        MouseInput::at(kind, client.0, client.1, event.buttons_down)
            .with_page(page.0, page.1)
            .with_screen(event.x as f32, event.y as f32)
            .with_modifiers(
                modifiers & 0x11 != 0,
                modifiers & 0x22 != 0,
                modifiers & 0x44 != 0,
                modifiers & 0x88 != 0,
            )
    };
    let mut send = |input| match document.dispatch_mouse(input) {
        Ok(outcome) => Some(outcome),
        Err(error) => {
            let message = format!("solara: QuickJS mouse dispatch failed: {error}\n");
            trueos::vsys::write_err(message.as_bytes());
            None
        }
    };

    if event.dx != 0 || event.dy != 0 {
        let _ = send(base(MouseEventKind::Move).with_movement(movement.0, movement.1));
    }
    let wheel_default = event.wheel != 0
        && send(base(MouseEventKind::Wheel).with_wheel(0.0, -f32::from(event.wheel) * 24.0))
            .is_none_or(|outcome| !outcome.default_prevented);
    for (mask, dom_button) in [
        (POINTER_BUTTON_PRIMARY, 0),
        (POINTER_BUTTON_MIDDLE, 1),
        (POINTER_BUTTON_SECONDARY, 2),
    ] {
        if event.buttons_pressed & mask != 0 {
            let _ = send(base(MouseEventKind::Down).with_button(dom_button));
        }
        if event.buttons_released & mask != 0 {
            let _ = send(base(MouseEventKind::Up).with_button(dom_button));
            let _ = send(base(MouseEventKind::Click).with_button(dom_button));
        }
    }
    wheel_default
}

fn resize_handle_contains(local_x: i32, local_y: i32, width: u32, height: u32) -> bool {
    let size = crate::gpu_ui::picasso::RESIZE_HANDLE_SIZE
        .min(width)
        .min(height) as i32;
    local_x >= width as i32 - size
        && local_y >= height as i32 - size
        && local_x < width as i32
        && local_y < height as i32
}

fn resize_drag_extent(drag: ResizeDrag, screen_x: u32, screen_y: u32) -> (u32, u32) {
    let maximum_width = drag.maximum_extent.0.max(MIN_FRAME_WIDTH);
    let maximum_height = drag.maximum_extent.1.max(MIN_FRAME_HEIGHT);
    let width = i64::from(drag.start_extent.0)
        .saturating_add(i64::from(screen_x) - i64::from(drag.start_screen.0))
        .clamp(i64::from(MIN_FRAME_WIDTH), i64::from(maximum_width));
    let height = i64::from(drag.start_extent.1)
        .saturating_add(i64::from(screen_y) - i64::from(drag.start_screen.1))
        .clamp(i64::from(MIN_FRAME_HEIGHT), i64::from(maximum_height));
    (width as u32, height as u32)
}

fn zoomed_extent(value: u32, zoom_percent: u32) -> u32 {
    ((u64::from(value)
        .saturating_mul(u64::from(zoom_percent))
        .saturating_add(50)
        / 100)
        .min(u64::from(u32::MAX)) as u32)
        .max(1)
}

fn physical_delta_to_logical(delta: i32, zoom_percent: u32) -> i32 {
    let scaled = i64::from(delta).saturating_mul(100) / i64::from(zoom_percent.max(1));
    scaled.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn build_text_canvas(
    frame: &mut Frame,
    canvas: (u32, u32),
    scene: &crate::gpu_ui::picasso::PaintScene,
) -> Result<TextCanvasStats, Error> {
    let lookup_rows = scene.font_lookup_rows().collect::<Vec<_>>();
    let glyphs = lookup_rows
        .iter()
        .map(|row| row.text.chars().count())
        .sum::<usize>();
    if glyphs > TEXT_CANVAS_MAX_GLYPHS {
        let message = format!(
            "solara: retained text canvas exceeds {} glyph softcap: {}\n",
            TEXT_CANVAS_MAX_GLYPHS, glyphs,
        );
        trueos::vsys::write_err(message.as_bytes());
        return Err(Error::Invalid);
    }

    let mut rows = Vec::<OwnedTextRow>::new();
    let compatibility_style_mismatches = lookup_rows
        .iter()
        .filter(|row| row.face != SceneFontFace::Inconsolata || row.slant != FontSlant::Normal)
        .count();
    if compatibility_style_mismatches != 0 {
        let message = format!(
            "solara: FontCanvas compatibility fallback maps {compatibility_style_mismatches} SceneDB face/slant rows to Inconsolata normal; FontLookup rows retain the exact requested styles\n",
        );
        trueos::vsys::write_err(message.as_bytes());
    }
    for lookup in &lookup_rows {
        let color = lookup.color.to_u32_le();
        let advance = crate::gpu_ui::ui4_char_width(lookup.font_pixels);
        let mut byte_start = 0usize;
        let mut character_start = 0usize;
        while byte_start < lookup.text.len() {
            let mut byte_end = (byte_start + TEXT_ROW_MAX_BYTES).min(lookup.text.len());
            while byte_end > byte_start && !lookup.text.is_char_boundary(byte_end) {
                byte_end -= 1;
            }
            if byte_end == byte_start {
                return Err(Error::Invalid);
            }
            let fragment = &lookup.text[byte_start..byte_end];
            rows.push(OwnedTextRow {
                text: fragment.to_string(),
                x: lookup.origin[0] + character_start as f32 * advance,
                y: lookup.origin[1],
                font_pixels: lookup.font_pixels,
                color_rgba: color,
            });
            character_start += fragment.chars().count();
            byte_start = byte_end;
        }
    }
    if rows.len() > FONT_CANVAS_MAX_ROWS {
        let message = format!(
            "solara: retained font canvas exceeds row softcap rows={}/{}\n",
            rows.len(),
            FONT_CANVAS_MAX_ROWS,
        );
        trueos::vsys::write_err(message.as_bytes());
        return Err(Error::Invalid);
    }

    let borrowed = rows
        .iter()
        .map(|row| FontCanvasRow {
            text: row.text.as_str(),
            x: row.x,
            y: row.y,
            font_pixels: row.font_pixels,
            color_rgba: row.color_rgba,
        })
        .collect::<Vec<_>>();
    retry_busy(|| frame.retain_font_canvas(SCENE_FONT, canvas, borrowed.as_slice()))?;
    Ok(TextCanvasStats {
        rows: rows.len(),
        glyphs,
        compatibility_style_mismatches,
    })
}

fn solid_quad(x: u32, y: u32, width: u32, height: u32, color_rgba: u32) -> SpriteQuad {
    let left = x as f32;
    let top = y as f32;
    let right = left + width as f32;
    let bottom = top + height as f32;
    SpriteQuad {
        sprite_id: 0,
        c0: SpriteCorner {
            x: left,
            y: top,
            ..SpriteCorner::default()
        },
        c1: SpriteCorner {
            x: right,
            y: top,
            ..SpriteCorner::default()
        },
        c2: SpriteCorner {
            x: right,
            y: bottom,
            ..SpriteCorner::default()
        },
        c3: SpriteCorner {
            x: left,
            y: bottom,
            ..SpriteCorner::default()
        },
        color_rgba,
        source_over: true,
    }
}

fn image_quad(
    sprite_id: u32,
    image_size: (u32, u32),
    destination: (u32, u32),
    destination_size: (u32, u32),
    source: (u32, u32),
    source_size: (u32, u32),
    opacity: u8,
) -> Result<SpriteQuad, Error> {
    if sprite_id == 0
        || image_size.0 == 0
        || image_size.1 == 0
        || destination_size.0 == 0
        || destination_size.1 == 0
        || source_size.0 == 0
        || source_size.1 == 0
        || source
            .0
            .checked_add(source_size.0)
            .is_none_or(|right| right > image_size.0)
        || source
            .1
            .checked_add(source_size.1)
            .is_none_or(|bottom| bottom > image_size.1)
    {
        return Err(Error::Invalid);
    }
    let left = destination.0 as f32;
    let top = destination.1 as f32;
    let right = left + destination_size.0 as f32;
    let bottom = top + destination_size.1 as f32;
    let u0 = source.0 as f32 / image_size.0 as f32;
    let v0 = source.1 as f32 / image_size.1 as f32;
    let u1 = (source.0 + source_size.0) as f32 / image_size.0 as f32;
    let v1 = (source.1 + source_size.1) as f32 / image_size.1 as f32;
    Ok(SpriteQuad {
        sprite_id,
        c0: SpriteCorner {
            x: left,
            y: top,
            u: u0,
            v: v0,
        },
        c1: SpriteCorner {
            x: right,
            y: top,
            u: u1,
            v: v0,
        },
        c2: SpriteCorner {
            x: right,
            y: bottom,
            u: u1,
            v: v1,
        },
        c3: SpriteCorner {
            x: left,
            y: bottom,
            u: u0,
            v: v1,
        },
        color_rgba: u32::from_le_bytes([255, 255, 255, opacity]),
        source_over: true,
    })
}

fn font_canvas_quad(
    frame: &Frame,
    canvas: (u32, u32),
    destination: (u32, u32),
    source: (u32, u32),
    source_size: (u32, u32),
    destination_size: (u32, u32),
) -> Result<SpriteQuad, Error> {
    let mut quad = frame.font_canvas_quad(canvas, source)?;
    let source_right = source.0.checked_add(source_size.0).ok_or(Error::Invalid)?;
    let source_bottom = source.1.checked_add(source_size.1).ok_or(Error::Invalid)?;
    if source_right > canvas.0
        || source_bottom > canvas.1
        || source_size.0 == 0
        || source_size.1 == 0
        || destination_size.0 == 0
        || destination_size.1 == 0
    {
        return Err(Error::Invalid);
    }
    let left = destination.0 as f32;
    let top = destination.1 as f32;
    let right = left + destination_size.0 as f32;
    let bottom = top + destination_size.1 as f32;
    let u0 = source.0 as f32 / canvas.0 as f32;
    let v0 = source.1 as f32 / canvas.1 as f32;
    let u1 = source_right as f32 / canvas.0 as f32;
    let v1 = source_bottom as f32 / canvas.1 as f32;
    quad.c0 = SpriteCorner {
        x: left,
        y: top,
        u: u0,
        v: v0,
    };
    quad.c1 = SpriteCorner {
        x: right,
        y: top,
        u: u1,
        v: v0,
    };
    quad.c2 = SpriteCorner {
        x: right,
        y: bottom,
        u: u1,
        v: v1,
    };
    quad.c3 = SpriteCorner {
        x: left,
        y: bottom,
        u: u0,
        v: v1,
    };
    Ok(quad)
}

fn invalid_picasso(stage: &str, error: impl core::fmt::Debug) -> Error {
    let message = format!("solara: Picasso {stage} failed: {error:?}\n");
    trueos::vsys::write_err(message.as_bytes());
    Error::Invalid
}

fn retry_busy(mut operation: impl FnMut() -> Result<(), Error>) -> Result<(), Error> {
    loop {
        match operation() {
            Ok(()) => return Ok(()),
            Err(Error::Busy) => trueos::vsys::sleep_ms(PRESENT_RETRY_MS),
            Err(error) => return Err(error),
        }
    }
}

async fn render_document() -> Result<RenderDocument, Error> {
    let launch_script = match trueos::async_fs::read_file_utf8(LAUNCH_SCRIPT_PATH).await {
        Ok(script) => script,
        Err(trueos::async_fs::ERR_NOT_FOUND) => {
            return Ok(default_render_document());
        }
        Err(code) => {
            let message = format!("solara: runtime launch read failed code={code}\n");
            trueos::vsys::write_err(message.as_bytes());
            return Err(Error::Invalid);
        }
    };
    let request = parse_surf_launch(launch_script.as_str())?;
    if !valid_handoff_tag(request.tag.as_str()) {
        return Err(invalid_startup("invalid surf handoff tag"));
    }

    let path = format!("{HANDOFF_ROOT}/{}.html", request.tag);
    let source = trueos::async_fs::read_file_utf8(path.as_bytes())
        .await
        .map_err(|code| {
            let message = format!("solara: HTML handoff read failed path={path} code={code}\n");
            trueos::vsys::write_err(message.as_bytes());
            Error::Invalid
        })?;
    let message = format!(
        "solara: accepted one-shot HTML handoff tag={} bytes={} source={}\n",
        request.tag,
        source.len(),
        request.source_url,
    );
    trueos::vsys::write_out(message.as_bytes());

    Ok(RenderDocument {
        source: Some(source),
        source_url: request.source_url,
        handoff_path: Some(path),
    })
}

fn parse_surf_launch(script: &str) -> Result<SurfLaunch, Error> {
    let mut fields = script.splitn(3, '\n');
    if fields.next() != Some(SURF_LAUNCH_HEADER) {
        return Err(invalid_startup("unsupported runtime launch script"));
    }
    let tag = fields
        .next()
        .ok_or_else(|| invalid_startup("runtime launch script is missing a handoff tag"))?;
    let source_url = fields
        .next()
        .ok_or_else(|| invalid_startup("runtime launch script is missing a source URL"))?;
    if source_url.is_empty()
        || source_url
            .bytes()
            .any(|byte| matches!(byte, b'\r' | b'\n' | 0))
    {
        return Err(invalid_startup("invalid runtime source URL"));
    }

    Ok(SurfLaunch {
        tag: String::from(tag),
        source_url: String::from(source_url),
    })
}

fn default_render_document() -> RenderDocument {
    RenderDocument {
        source: None,
        source_url: String::from("trueos://solara/docs/demoui.html"),
        handoff_path: None,
    }
}

fn valid_handoff_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn invalid_startup(reason: &str) -> Error {
    trueos::vsys::write_err(b"solara: invalid render request: ");
    trueos::vsys::write_err(reason.as_bytes());
    trueos::vsys::write_err(b"\n");
    Error::Invalid
}
