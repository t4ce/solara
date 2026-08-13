//! TRUEOS Blueprint entry point for Solara's retained Picasso/UI4 pass.

use trueos_helio_runtime::picasso_scene::{Color, LoweredCommand};

use trueos::ui4_scene::{
    self, CursorSource, Damage, Error, Font, FontCanvasRow, Frame, POINTER_BUTTON_MIDDLE,
    POINTER_BUTTON_PRIMARY, POINTER_BUTTON_SECONDARY, PanPhase, PointerEvent, SpriteCorner,
    SpriteQuad,
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
    document: crate::gpu_ui::Ui4TextDocument,
    paint_scene: crate::gpu_ui::picasso::PaintScene,
    canvas: (u32, u32),
    origin: (u32, u32),
    active_pan_source: Option<CursorSource>,
    font_canvas_dirty: bool,
    needs_present: bool,
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
}

impl SolaraView {
    fn clamp_origin(&mut self) -> bool {
        let previous = self.origin;
        self.origin.0 = self
            .origin
            .0
            .min(self.canvas.0.saturating_sub(self.frame.width()));
        self.origin.1 = self
            .origin
            .1
            .min(self.canvas.1.saturating_sub(self.frame.height()));
        self.origin != previous
    }

    fn sync_dom_viewport(&mut self) {
        if let Err(error) = self.document.set_visual_viewport(self.origin) {
            let message = format!("solara: QuickJS viewport sync failed: {error}\n");
            trueos::vsys::write_err(message.as_bytes());
        }
    }

    fn ensure_font_canvas(&mut self) -> Result<(), Error> {
        if !self.font_canvas_dirty {
            return Ok(());
        }
        let _ = build_text_canvas(&mut self.frame, self.canvas, self.document.text_batch())?;
        self.font_canvas_dirty = false;
        Ok(())
    }

    fn take_view_updates(&mut self) -> Result<(), Error> {
        while let Some(event) = self.frame.take_resize_event()? {
            if event.width == self.frame.width() && event.height == self.frame.height() {
                continue;
            }
            retry_busy(|| self.frame.resize(event.width, event.height))?;
            self.font_canvas_dirty = true;
            self.needs_present = true;
            if self.clamp_origin() {
                self.sync_dom_viewport();
            }
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
                    let next = (
                        crate::gpu_ui::clamped_pan_origin(
                            self.origin.0,
                            event.dx,
                            self.canvas.0,
                            self.frame.width(),
                        ),
                        crate::gpu_ui::clamped_pan_origin(
                            self.origin.1,
                            event.dy,
                            self.canvas.1,
                            self.frame.height(),
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
            let pre_default_origin = self.origin;
            let wheel_default =
                dispatch_pointer_event(&mut self.document, event, modifiers, pre_default_origin);
            let next = crate::gpu_ui::wheel_scroll_origin(
                self.origin.1,
                event.wheel,
                wheel_default,
                self.canvas.1,
                self.frame.height(),
            );
            if next != self.origin.1 {
                self.origin.1 = next;
                self.needs_present = true;
                self.sync_dom_viewport();
            }
        }
        Ok(())
    }

    fn present_viewport(&mut self) -> Result<usize, Error> {
        if self.clamp_origin() {
            self.sync_dom_viewport();
        }
        self.ensure_font_canvas()?;
        let commands = self
            .paint_scene
            .lower(self.origin, (self.frame.width(), self.frame.height()))
            .map_err(|error| invalid_picasso("viewport lower", error))?;
        let mut quads = Vec::with_capacity(commands.len());
        for command in commands {
            match command {
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
                )?),
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
    match trueos::async_fs::block_on(present_text_frame()) {
        Ok(view) => {
            trueos::vsys::write_out(
                b"solara: retained Picasso document + FontKernel canvas visible; wheel/middle-pan and DOM-configured scrollbar active\n",
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

async fn present_text_frame() -> Result<SolaraView, Error> {
    // Build the DOM once in logical page coordinates and materialize one warm
    // GPU canvas. Pan and maximize only change the crop composed into UI4.
    let document = render_document().await?;
    let mut ui4_document = match document.source.as_deref() {
        Some(source) => crate::gpu_ui::ui4_document_for_html(
            source,
            document.source_url.as_str(),
            FRAME_WIDTH as f32,
        ),
        None => crate::gpu_ui::embedded_ui4_document(FRAME_WIDTH as f32),
    }
    .map_err(|error| {
        trueos::vsys::write_err(b"solara: DOM scene build failed: ");
        trueos::vsys::write_err(error.as_bytes());
        trueos::vsys::write_err(b"\n");
        Error::Invalid
    })?;
    let content_height = ui4_document.content_height();
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
    let mut view = SolaraView {
        frame: Frame::open(160, 180, FRAME_WIDTH, FRAME_HEIGHT)?,
        document: ui4_document,
        paint_scene: crate::gpu_ui::picasso::PaintScene::new(),
        canvas,
        origin: (0, 0),
        active_pan_source: None,
        font_canvas_dirty: false,
        needs_present: true,
    };
    let (scene_stats, text) = view
        .document
        .rebuild_scene(&mut view.paint_scene, view.canvas, BACKGROUND)
        .map_err(|error| invalid_picasso("DOM scene publication", error))?;
    let stats = build_text_canvas(&mut view.frame, view.canvas, text)?;
    view.sync_dom_viewport();
    let lowered_commands = view.present_viewport()?;
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
        "solara: Picasso scene epoch={} logical_rows={} shape_rows={} lowered_commands={} text_rows={} glyphs={} viewport={}x{} canvas={}x{} content_height={:.1} scrollbar={} font=inconsolata source={}\n",
        scene_stats.epoch,
        scene_stats.logical_rows,
        scene_stats.shape_rows,
        lowered_commands,
        stats.rows,
        stats.glyphs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        view.canvas.0,
        view.canvas.1,
        content_height,
        view.document.scrollbar_side().as_str(),
        document.source_url,
    );
    trueos::vsys::write_out(summary.as_bytes());
    Ok(view)
}

fn dispatch_pointer_event(
    document: &mut crate::gpu_ui::Ui4TextDocument,
    event: PointerEvent,
    modifiers: u8,
    page_origin: (u32, u32),
) -> bool {
    let base = |kind| {
        MouseInput::at(
            kind,
            event.local_x as f32,
            event.local_y as f32,
            event.buttons_down,
        )
        .with_page(
            event.local_x as f32 + page_origin.0 as f32,
            event.local_y as f32 + page_origin.1 as f32,
        )
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
        let _ = send(base(MouseEventKind::Move).with_movement(event.dx as f32, event.dy as f32));
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

fn build_text_canvas(
    frame: &mut Frame,
    canvas: (u32, u32),
    text: &crate::gpu_ui::Ui4TextBatch,
) -> Result<TextCanvasStats, Error> {
    let glyphs = text
        .sections
        .iter()
        .map(|section| section.text.chars().count())
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
    for section in &text.sections {
        let color = packed_color(section.color);
        let advance = crate::gpu_ui::ui4_char_width(section.font_size);
        let mut byte_start = 0usize;
        let mut character_start = 0usize;
        while byte_start < section.text.len() {
            let mut byte_end = (byte_start + TEXT_ROW_MAX_BYTES).min(section.text.len());
            while byte_end > byte_start && !section.text.is_char_boundary(byte_end) {
                byte_end -= 1;
            }
            if byte_end == byte_start {
                return Err(Error::Invalid);
            }
            let fragment = &section.text[byte_start..byte_end];
            rows.push(OwnedTextRow {
                text: fragment.to_string(),
                x: section.x + character_start as f32 * advance,
                y: section.y,
                font_pixels: section.font_size,
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

fn font_canvas_quad(
    frame: &Frame,
    canvas: (u32, u32),
    destination: (u32, u32),
    source: (u32, u32),
    size: (u32, u32),
) -> Result<SpriteQuad, Error> {
    let mut quad = frame.font_canvas_quad(canvas, source)?;
    let source_right = source.0.checked_add(size.0).ok_or(Error::Invalid)?;
    let source_bottom = source.1.checked_add(size.1).ok_or(Error::Invalid)?;
    if source_right > canvas.0 || source_bottom > canvas.1 || size.0 == 0 || size.1 == 0 {
        return Err(Error::Invalid);
    }
    let left = destination.0 as f32;
    let top = destination.1 as f32;
    let right = left + size.0 as f32;
    let bottom = top + size.1 as f32;
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

fn packed_color(color: [f32; 4]) -> u32 {
    let channel = |value: f32| ((value.clamp(0.0, 1.0) * 255.0) + 0.5) as u8;
    ui4_scene::rgba(
        channel(color[0]),
        channel(color[1]),
        channel(color[2]),
        channel(color[3]),
    )
}
