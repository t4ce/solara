//! TRUEOS Blueprint entry point for Solara's static text-only UI4 pass.

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
// A 2,000-line-pixel 16:9 surface is the retained-document budget. Solara's
// current page is only FRAME_WIDTH wide, so it normally allocates considerably
// less while retaining enough height to pan at 1440p.
const TEXT_CANVAS_MAX_WIDTH: u32 = 3_556;
const TEXT_CANVAS_MAX_HEIGHT: u32 = 2_000;
const FONT_CANVAS_MAX_ROWS: usize = 256;
const TEXT_ROW_MAX_BYTES: usize = 1_024;
const TEXT_CANVAS_MAX_GLYPHS: usize = 4_096;
const BACKGROUND_RGBA: u32 = ui4_scene::rgba(250, 250, 250, 255);
const TRANSPARENT_RGBA: u32 = ui4_scene::rgba(0, 0, 0, 0);
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
    canvas: (u32, u32),
    origin: (u32, u32),
    active_pan_source: Option<CursorSource>,
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
    fn clamp_origin(&mut self) {
        self.origin.0 = self
            .origin
            .0
            .min(self.canvas.0.saturating_sub(self.frame.width()));
        self.origin.1 = self
            .origin
            .1
            .min(self.canvas.1.saturating_sub(self.frame.height()));
    }

    fn take_view_updates(&mut self) -> Result<bool, Error> {
        let mut changed = false;
        while let Some(event) = self.frame.take_resize_event()? {
            if event.width == self.frame.width() && event.height == self.frame.height() {
                continue;
            }
            retry_busy(|| self.frame.resize(event.width, event.height))?;
            self.clamp_origin();
            changed = true;
        }
        while let Some(event) = self.frame.take_pan_event()? {
            match event.phase {
                PanPhase::Begin => self.active_pan_source = Some(event.source),
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
                    changed |= next != self.origin;
                    self.origin = next;
                }
                PanPhase::End if self.active_pan_source == Some(event.source) => {
                    self.active_pan_source = None;
                }
                _ => {}
            }
        }
        Ok(changed)
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
            dispatch_pointer_event(&mut self.document, event, modifiers);
        }
        Ok(())
    }

    fn present_viewport(&mut self) -> Result<(), Error> {
        self.clamp_origin();
        retry_busy(|| self.frame.begin_sprite_frame(TRANSPARENT_RGBA))?;
        let visible_width = self.frame.width().min(self.canvas.0 - self.origin.0);
        let visible_height = self.frame.height().min(self.canvas.1 - self.origin.1);
        let text = self.frame.font_canvas_quad(self.canvas, self.origin)?;
        self.frame.draw_sprite_quads(&[
            solid_quad(visible_width, visible_height, BACKGROUND_RGBA),
            text,
        ])?;
        retry_busy(|| {
            self.frame
                .publish(Damage::full(self.frame.width(), self.frame.height()))
        })
    }
}

pub(crate) fn run() -> ! {
    match trueos::async_fs::block_on(present_text_frame()) {
        Ok(view) => {
            trueos::vsys::write_out(
                b"solara: retained FontKernel canvas visible; middle-drag crop pan and maximize active\n",
            );
            resident_view_loop(view)
        }
        Err(error) => {
            let message = format!("solara: UI4 text-row frame failed: {error:?}\n");
            trueos::vsys::write_err(message.as_bytes());
            loop {
                trueos::vsys::sleep_ms(250);
            }
        }
    }
}

fn resident_view_loop(mut view: SolaraView) -> ! {
    let mut visible_logged = false;
    loop {
        if !visible_logged && view.frame.take_first_presentation().unwrap_or(false) {
            visible_logged = true;
            let message = format!(
                "solara: immutable frame visible window={}\n",
                view.frame.window_id(),
            );
            trueos::vsys::write_out(message.as_bytes());
        }
        match view.take_view_updates() {
            Ok(true) => {
                if let Err(error) = view.present_viewport() {
                    let message = format!("solara: viewport presentation failed: {error:?}\n");
                    trueos::vsys::write_err(message.as_bytes());
                }
            }
            Ok(false) | Err(Error::Busy) => {}
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
    let canvas = (
        FRAME_WIDTH.min(TEXT_CANVAS_MAX_WIDTH),
        (content_height.ceil() as u32).clamp(1, TEXT_CANVAS_MAX_HEIGHT),
    );
    let mut view = SolaraView {
        frame: Frame::open_immutable(160, 180, FRAME_WIDTH, FRAME_HEIGHT)?,
        document: ui4_document,
        canvas,
        origin: (0, 0),
        active_pan_source: None,
    };
    let text = view.document.rebuild_text();
    let stats = build_text_canvas(&mut view.frame, view.canvas, text)?;
    view.present_viewport()?;
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
        "solara: retained scene rows={} glyphs={} viewport={}x{} canvas={}x{} content_height={:.1} font=inconsolata source={}\n",
        stats.rows,
        stats.glyphs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        view.canvas.0,
        view.canvas.1,
        content_height,
        document.source_url,
    );
    trueos::vsys::write_out(summary.as_bytes());
    Ok(view)
}

fn dispatch_pointer_event(
    document: &mut crate::gpu_ui::Ui4TextDocument,
    event: PointerEvent,
    modifiers: u8,
) {
    let base = |kind| {
        MouseInput::at(
            kind,
            event.local_x as f32,
            event.local_y as f32,
            event.buttons_down,
        )
        .with_screen(event.x as f32, event.y as f32)
        .with_modifiers(
            modifiers & 0x11 != 0,
            modifiers & 0x22 != 0,
            modifiers & 0x44 != 0,
            modifiers & 0x88 != 0,
        )
    };
    let mut send = |input| {
        if let Err(error) = document.dispatch_mouse(input) {
            let message = format!("solara: QuickJS mouse dispatch failed: {error}\n");
            trueos::vsys::write_err(message.as_bytes());
        }
    };

    if event.dx != 0 || event.dy != 0 {
        send(base(MouseEventKind::Move).with_movement(event.dx as f32, event.dy as f32));
    }
    if event.wheel != 0 {
        send(base(MouseEventKind::Wheel).with_wheel(0.0, -f32::from(event.wheel) * 24.0));
    }
    for (mask, dom_button) in [
        (POINTER_BUTTON_PRIMARY, 0),
        (POINTER_BUTTON_MIDDLE, 1),
        (POINTER_BUTTON_SECONDARY, 2),
    ] {
        if event.buttons_pressed & mask != 0 {
            send(base(MouseEventKind::Down).with_button(dom_button));
        }
        if event.buttons_released & mask != 0 {
            send(base(MouseEventKind::Up).with_button(dom_button));
            send(base(MouseEventKind::Click).with_button(dom_button));
        }
    }
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

fn solid_quad(width: u32, height: u32, color_rgba: u32) -> SpriteQuad {
    SpriteQuad {
        sprite_id: 0,
        c0: SpriteCorner::default(),
        c1: SpriteCorner {
            x: width as f32,
            ..SpriteCorner::default()
        },
        c2: SpriteCorner {
            x: width as f32,
            y: height as f32,
            ..SpriteCorner::default()
        },
        c3: SpriteCorner {
            y: height as f32,
            ..SpriteCorner::default()
        },
        color_rgba,
        source_over: true,
    }
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
