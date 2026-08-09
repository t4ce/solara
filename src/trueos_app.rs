//! TRUEOS Blueprint entry point for Solara's static text-only UI4 pass.

use trueos::ui4_scene::{
    self, Damage, Error, Font, Frame, POINTER_BUTTON_MIDDLE, POINTER_BUTTON_PRIMARY,
    POINTER_BUTTON_SECONDARY, PointerEvent, SceneTextRow,
};

use crate::gpu_ui::input::{MouseEventKind, MouseInput};

const FRAME_WIDTH: u32 = 960;
const FRAME_HEIGHT: u32 = 720;
const RENDER_ONCE_ARG: &str = "--trueos-render-once";
const HTML_TAG_ARG: &str = "--trueos-html-tag";
const SOURCE_URL_ARG: &str = "--trueos-source-url";
const HANDOFF_ROOT: &str = "/common/solara/surf";
const RESIDENT_POLL_MS: u64 = 16;
const PRESENT_RETRY_MS: u64 = 2;
const FONT_STAMP_MAX_ROWS_PER_LAYER: usize = 64;
const FONT_STAMP_MAX_LAYERS: usize = 64;
const FONT_STAMP_MAX_ROWS: usize = 256;
const TEXT_ROW_MAX_BYTES: usize = 1_024;
const TEXT_FRAME_MAX_GLYPHS: usize = 4_096;
const BACKGROUND_RGBA: u32 = ui4_scene::rgba(250, 250, 250, 255);
// Solara lays out every current text run with the same monospaced metrics as
// its bundled desktop face. Keep the UI4 scene on that face as well instead
// of repainting those coordinates with the proportional legacy default.
const SCENE_FONT: Font = Font::Inconsolata;

struct RenderDocument {
    source: Option<String>,
    source_url: String,
    handoff_path: Option<String>,
}

struct SolaraView {
    frame: Frame,
    document: crate::gpu_ui::Ui4TextDocument,
}

struct OwnedTextRow {
    text: String,
    x: f32,
    y: f32,
    font_pixels: f32,
}

struct FontColorLayer {
    color_rgba: u32,
    rows: Vec<OwnedTextRow>,
}

struct StaticTextStats {
    rows: usize,
    layers: usize,
    glyphs: usize,
}

impl SolaraView {
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
}

pub(crate) fn run() -> ! {
    match trueos::async_fs::block_on(present_text_frame()) {
        Ok(view) => {
            trueos::vsys::write_out(
                b"solara: DOM text published once through immutable UI4 + FontKernel RGBA frame stamp\n",
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
    // Build the DOM once in logical page coordinates. This temporary text-only
    // mode stamps the initial viewport directly into one immutable UI4 frame.
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
    let mut view = SolaraView {
        frame: Frame::open_immutable(160, 180, FRAME_WIDTH, FRAME_HEIGHT)?,
        document: ui4_document,
    };
    let text = view.document.rebuild_text();
    let stats = stamp_static_text_frame(&mut view.frame, text)?;
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
        "solara: static scene rows={} layers={} glyphs={} viewport={}x{} content_height={:.1} font=inconsolata source={}\n",
        stats.rows,
        stats.layers,
        stats.glyphs,
        FRAME_WIDTH,
        FRAME_HEIGHT,
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

fn stamp_static_text_frame(
    frame: &mut Frame,
    text: &crate::gpu_ui::Ui4TextBatch,
) -> Result<StaticTextStats, Error> {
    let glyphs = text
        .sections
        .iter()
        .map(|section| section.text.chars().count())
        .sum::<usize>();
    if glyphs > TEXT_FRAME_MAX_GLYPHS {
        let message = format!(
            "solara: static text frame exceeds {} glyph softcap: {}\n",
            TEXT_FRAME_MAX_GLYPHS, glyphs,
        );
        trueos::vsys::write_err(message.as_bytes());
        return Err(Error::Invalid);
    }

    let mut layers = Vec::<FontColorLayer>::new();
    let mut row_count = 0usize;
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
            let row = OwnedTextRow {
                text: fragment.to_string(),
                x: section.x + character_start as f32 * advance,
                y: section.y,
                font_pixels: section.font_size,
            };
            let layer_index = match layers.iter_mut().position(|layer| {
                layer.color_rgba == color && layer.rows.len() < FONT_STAMP_MAX_ROWS_PER_LAYER
            }) {
                Some(index) => index,
                None => {
                    layers.push(FontColorLayer {
                        color_rgba: color,
                        rows: Vec::new(),
                    });
                    layers.len() - 1
                }
            };
            layers[layer_index].rows.push(row);
            row_count += 1;
            character_start += fragment.chars().count();
            byte_start = byte_end;
        }
    }
    if row_count > FONT_STAMP_MAX_ROWS || layers.len() > FONT_STAMP_MAX_LAYERS {
        let message = format!(
            "solara: static font stamp exceeds contract rows={}/{} layers={}/{}\n",
            row_count,
            FONT_STAMP_MAX_ROWS,
            layers.len(),
            FONT_STAMP_MAX_LAYERS,
        );
        trueos::vsys::write_err(message.as_bytes());
        return Err(Error::Invalid);
    }

    retry_busy(|| frame.begin(BACKGROUND_RGBA))?;
    for layer in &layers {
        let rows = layer
            .rows
            .iter()
            .map(|row| SceneTextRow {
                text: row.text.as_str(),
                x: row.x,
                y: row.y,
                font_pixels: row.font_pixels,
            })
            .collect::<Vec<_>>();
        retry_busy(|| {
            frame.stamp_text_scene(
                SCENE_FONT,
                (FRAME_WIDTH, FRAME_HEIGHT),
                layer.color_rgba,
                rows.as_slice(),
            )
        })?;
    }
    retry_busy(|| frame.publish(Damage::full(frame.width(), frame.height())))?;
    Ok(StaticTextStats {
        rows: row_count,
        layers: layers.len(),
        glyphs,
    })
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
    let mut render_once = false;
    let mut html_tag = None;
    let mut source_url = None;
    let mut args = trueos::env::args();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            RENDER_ONCE_ARG => render_once = true,
            HTML_TAG_ARG => {
                html_tag = Some(
                    args.next()
                        .ok_or_else(|| invalid_startup("missing value after --trueos-html-tag"))?,
                );
            }
            SOURCE_URL_ARG => {
                source_url =
                    Some(args.next().ok_or_else(|| {
                        invalid_startup("missing value after --trueos-source-url")
                    })?);
            }
            _ => {}
        }
    }

    if !render_once && html_tag.is_none() {
        return Ok(RenderDocument {
            source: None,
            source_url: String::from("trueos://solara/docs/demoui.html"),
            handoff_path: None,
        });
    }
    if !render_once {
        return Err(invalid_startup(
            "HTML handoff tag requires --trueos-render-once",
        ));
    }
    let tag = html_tag
        .ok_or_else(|| invalid_startup("--trueos-render-once requires --trueos-html-tag"))?;
    if !valid_handoff_tag(tag.as_str()) {
        return Err(invalid_startup("invalid --trueos-html-tag value"));
    }

    let path = format!("{HANDOFF_ROOT}/{tag}.html");
    let source = trueos::async_fs::read_file_utf8(path.as_bytes())
        .await
        .map_err(|code| {
            let message = format!("solara: HTML handoff read failed path={path} code={code}\n");
            trueos::vsys::write_err(message.as_bytes());
            Error::Invalid
        })?;
    let source_url = source_url.unwrap_or_else(|| format!("trueos://shell2/surf/{tag}"));
    let message = format!(
        "solara: accepted one-shot HTML handoff tag={tag} bytes={} source={}\n",
        source.len(),
        source_url,
    );
    trueos::vsys::write_out(message.as_bytes());

    Ok(RenderDocument {
        source: Some(source),
        source_url,
        handoff_path: Some(path),
    })
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
