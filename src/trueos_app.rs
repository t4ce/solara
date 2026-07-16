//! TRUEOS Blueprint entry point for Solara's text-only UI4 experiment.

use trueos::ui4_solara_text::{self, Damage, Error, Font, Frame, TextRow};

const FRAME_WIDTH: u32 = 960;
const FRAME_HEIGHT: u32 = 360;
const PREFERRED_FONT_TARGET: u32 = 256;

pub(crate) fn run() -> ! {
    match present_text_frame() {
        Ok(_frame) => {
            trueos::vsys::write_out(
                b"solara: UI4 text-row frame published; keeping Blueprint resident\n",
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
    let native_scale = supported
        .iter()
        .rev()
        .find(|size| size.target_pixels <= PREFERRED_FONT_TARGET)
        .or_else(|| supported.first())
        .ok_or(Error::Font)?
        .native_scale;

    let mut frame = Frame::open(160, 180, FRAME_WIDTH, FRAME_HEIGHT)?;
    frame.begin(ui4_solara_text::rgba(8, 13, 23, 255))?;
    frame.draw_text_rows(
        Font::Default,
        native_scale,
        (48, 48),
        ui4_solara_text::rgba(223, 235, 255, 255),
        &[
            TextRow {
                text: "Solara / TRUEOS",
                x: 0.0,
                y: 0.0,
            },
            TextRow {
                text: "gpu-text-only -> kernel font service",
                x: 0.0,
                y: 42.0,
            },
            TextRow {
                text: "UI4 broker frame / dirty double buffer",
                x: 0.0,
                y: 84.0,
            },
        ],
    )?;
    frame.publish(Damage::full(FRAME_WIDTH, FRAME_HEIGHT))?;
    Ok(frame)
}
