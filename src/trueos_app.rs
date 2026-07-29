//! TRUEOS Blueprint entry point for Solara's text-only UI4 experiment.

use trueos::ui4_solara_text::{self, CursorSource, Error, Font, Frame, PanPhase, SceneTextRow};

const FRAME_WIDTH: u32 = 960;
const FRAME_HEIGHT: u32 = 720;
const RENDER_ONCE_ARG: &str = "--trueos-render-once";
const HTML_TAG_ARG: &str = "--trueos-html-tag";
const SOURCE_URL_ARG: &str = "--trueos-source-url";
const HANDOFF_ROOT: &str = "/common/solara/surf";
const INPUT_POLL_MS: u64 = 8;
const PRESENT_RETRY_MS: u64 = 2;
const TEXT_ROWS_PER_CALL: usize = 64;
const TEXT_ROW_MAX_BYTES: usize = 1_024;
const TEXT_BACKBUFFER_MAX_EXTENT: u32 = 4_096;
const TEXT_BACKBUFFER_MAX_GLYPHS: usize = 4_096;
const BACKGROUND_RGBA: u32 = ui4_solara_text::rgba(250, 250, 250, 255);
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
    canvas: (u32, u32),
    active_pan_source: Option<CursorSource>,
}

impl SolaraView {
    fn present_viewport(&mut self) -> Result<(), Error> {
        self.frame.begin_sprite_frame(BACKGROUND_RGBA)?;
        publish_text_backbuffer_view(
            &mut self.frame,
            self.canvas,
            (0, self.document.scroll_y().round() as u32),
        )
    }

    fn take_pan_updates(&mut self) -> Result<(), Error> {
        let mut changed = false;
        while let Some(event) = self.frame.take_pan_event()? {
            match event.phase {
                PanPhase::Begin => self.active_pan_source = Some(event.source),
                PanPhase::Update if self.active_pan_source == Some(event.source) => {
                    // Solara's current layout is vertical. UI4 still reports
                    // both axes, but horizontal movement is deliberately left
                    // for a future wide-layout viewport.
                    changed |= self.document.pan_vertical(event.dy, FRAME_HEIGHT as f32);
                }
                PanPhase::End if self.active_pan_source == Some(event.source) => {
                    self.active_pan_source = None;
                    let message = format!(
                        "solara: pan ended window={} scroll_y={:.1}\n",
                        self.frame.window_id(),
                        self.document.scroll_y(),
                    );
                    trueos::vsys::write_out(message.as_bytes());
                }
                _ => {}
            }
        }
        if changed {
            self.present_viewport()?;
        }
        Ok(())
    }
}

pub(crate) fn run() -> ! {
    match trueos::async_fs::block_on(present_text_frame()) {
        Ok(view) => {
            trueos::vsys::write_out(
                b"solara: DOM text backbuffer published to UI4; middle-drag crop pan active\n",
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
    loop {
        match view.take_pan_updates() {
            Ok(()) => {}
            Err(Error::Busy) => {}
            Err(error) => {
                let message = format!("solara: UI4 pan input failed: {error:?}\n");
                trueos::vsys::write_err(message.as_bytes());
            }
        }
        trueos::vsys::sleep_ms(INPUT_POLL_MS);
    }
}

async fn present_text_frame() -> Result<SolaraView, Error> {
    // Build the complete immutable document in logical page coordinates. The
    // Blueprint font path materializes it once; UI4 only receives cropped views.
    let document = render_document().await?;
    let ui4_document = match document.source.as_deref() {
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
    let canvas_height = (ui4_document.content_height().ceil() as u32).max(FRAME_HEIGHT);
    if FRAME_WIDTH > TEXT_BACKBUFFER_MAX_EXTENT || canvas_height > TEXT_BACKBUFFER_MAX_EXTENT {
        let message = format!(
            "solara: text backbuffer exceeds {}px softcap: {}x{}\n",
            TEXT_BACKBUFFER_MAX_EXTENT, FRAME_WIDTH, canvas_height,
        );
        trueos::vsys::write_err(message.as_bytes());
        return Err(Error::Invalid);
    }
    let mut view = SolaraView {
        frame: Frame::open(160, 180, FRAME_WIDTH, FRAME_HEIGHT)?,
        document: ui4_document,
        canvas: (FRAME_WIDTH, canvas_height),
        active_pan_source: None,
    };
    let text = view.document.rebuild_text();
    let scene_rows = build_text_backbuffer(&mut view.frame, view.canvas, text)?;
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
        "solara: scene rows={} viewport={}x{} backbuffer={}x{} content_height={:.1} source={}\n",
        scene_rows,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        view.canvas.0,
        view.canvas.1,
        view.document.content_height(),
        document.source_url,
    );
    trueos::vsys::write_out(summary.as_bytes());
    Ok(view)
}

fn build_text_backbuffer(
    frame: &mut Frame,
    canvas: (u32, u32),
    text: &crate::gpu_ui::Ui4TextBatch,
) -> Result<usize, Error> {
    #[derive(Clone)]
    struct OwnedSceneTextRow {
        text: String,
        x: f32,
        y: f32,
        font_pixels: f32,
    }

    let glyphs = text
        .sections
        .iter()
        .map(|section| section.text.chars().count())
        .sum::<usize>();
    if glyphs > TEXT_BACKBUFFER_MAX_GLYPHS {
        let message = format!(
            "solara: text backbuffer exceeds {} glyph softcap: {}\n",
            TEXT_BACKBUFFER_MAX_GLYPHS, glyphs,
        );
        trueos::vsys::write_err(message.as_bytes());
        return Err(Error::Invalid);
    }

    let mut retained_scenes: Vec<(u32, Vec<OwnedSceneTextRow>)> = Vec::new();
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
            let row = OwnedSceneTextRow {
                text: fragment.to_string(),
                x: section.x + character_start as f32 * advance,
                y: section.y,
                font_pixels: section.font_size,
            };
            if let Some((_, rows)) = retained_scenes.iter_mut().find(|(existing_color, rows)| {
                *existing_color == color && rows.len() < TEXT_ROWS_PER_CALL
            }) {
                rows.push(row);
            } else {
                retained_scenes.push((color, vec![row]));
            }
            character_start += fragment.chars().count();
            byte_start = byte_end;
        }
    }

    frame.begin_sprite_frame(BACKGROUND_RGBA)?;
    for (color, rows) in &retained_scenes {
        let borrowed = rows
            .iter()
            .map(|row| SceneTextRow {
                text: row.text.as_str(),
                x: row.x,
                y: row.y,
                font_pixels: row.font_pixels,
            })
            .collect::<Vec<_>>();
        frame.retain_text_backbuffer(SCENE_FONT, canvas, *color, borrowed.as_slice())?;
    }
    publish_text_backbuffer_view(frame, canvas, (0, 0))?;
    Ok(text.sections.len())
}

fn publish_text_backbuffer_view(
    frame: &mut Frame,
    canvas: (u32, u32),
    origin: (u32, u32),
) -> Result<(), Error> {
    loop {
        match frame.publish_text_backbuffer_view(canvas, origin) {
            Err(Error::Busy) => trueos::vsys::sleep_ms(PRESENT_RETRY_MS),
            result => return result,
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
    ui4_solara_text::rgba(
        channel(color[0]),
        channel(color[1]),
        channel(color[2]),
        channel(color[3]),
    )
}
