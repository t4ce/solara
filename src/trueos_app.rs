//! TRUEOS Blueprint entry point for Solara's text-only UI4 experiment.

use trueos::ui4_solara_text::{self, Damage, Error, Font, Frame, SceneTextRow};

const FRAME_WIDTH: u32 = 960;
const FRAME_HEIGHT: u32 = 720;

pub(crate) fn run() -> ! {
    match present_text_frame() {
        Ok(_frame) => {
            trueos::vsys::write_out(
                b"solara: DOM text scene published to UI4; keeping Blueprint resident\n",
            );
            loop {
                trueos::vsys::sleep_ms(250);
            }
        }
        Err(_) => {
            trueos::vsys::write_err(b"solara: UI4 text-row frame failed\n");
            loop {
                trueos::vsys::sleep_ms(250);
            }
        }
    }
}

fn present_text_frame() -> Result<Frame, Error> {
    let supported = ui4_solara_text::font_sizes()?;
    let required_extent = FRAME_WIDTH.max(FRAME_HEIGHT);
    supported
        .iter()
        .any(|size| size.target_pixels >= required_extent)
        .then_some(())
        .ok_or(Error::Font)?;

    let text = crate::gpu_ui::embedded_text_batch(FRAME_WIDTH as f32).map_err(|error| {
        trueos::vsys::write_err(b"solara: DOM scene build failed: ");
        trueos::vsys::write_err(error.as_bytes());
        trueos::vsys::write_err(b"\n");
        Error::Invalid
    })?;
    let mut color_groups: Vec<(u32, Vec<SceneTextRow<'_>>)> = Vec::new();
    for section in &text.sections {
        let color = packed_color(section.color);
        let row = SceneTextRow {
            text: section.text.as_str(),
            x: section.x,
            y: section.y,
            font_pixels: section.font_size,
        };
        if let Some((_, rows)) = color_groups
            .iter_mut()
            .find(|(group_color, _)| *group_color == color)
        {
            rows.push(row);
        } else {
            color_groups.push((color, vec![row]));
        }
    }

    let mut frame = Frame::open(160, 180, FRAME_WIDTH, FRAME_HEIGHT)?;
    frame.begin(ui4_solara_text::rgba(250, 250, 250, 255))?;
    for (color, rows) in &color_groups {
        for chunk in rows.chunks(ui4_solara_text::MAX_SCENE_TEXT_ROWS_PER_CALL) {
            frame.draw_text_scene(Font::Default, (FRAME_WIDTH, FRAME_HEIGHT), *color, chunk)?;
        }
    }
    frame.publish(Damage::full(FRAME_WIDTH, FRAME_HEIGHT))?;
    let summary = format!(
        "solara: scene rows={} color_passes={} viewport={}x{}\n",
        text.sections.len(),
        color_groups.len(),
        FRAME_WIDTH,
        FRAME_HEIGHT,
    );
    trueos::vsys::write_out(summary.as_bytes());
    Ok(frame)
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
