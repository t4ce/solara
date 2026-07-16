//! wgpu renderer for demoui.html elements (excluding document structure tags).

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod app;
#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod async_utils;
mod geometry;
mod html;
#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod loader;
#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
mod renderer;
mod shapes;
mod text;

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
pub use app::run;

/// Parse and paint the same bundled document used by the desktop renderer,
/// returning its backend-neutral text records for the TRUEOS UI4 consumer.
#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
pub(crate) fn embedded_text_batch(page_width: f32) -> Result<text::TextBatch, String> {
    use rust_qjs_dom::DomEngine;

    let mut engine = DomEngine::new().map_err(|error| format!("failed to start DOM: {error}"))?;
    let artifact = engine
        .parse(
            include_str!("../../docs/demoui.html"),
            "trueos://solara/docs/demoui.html",
        )
        .map_err(|error| format!("failed to parse bundled demo: {error}"))?;
    let document = html::Document::from_dom(artifact, engine, page_width)?;
    let mut batch = html::RenderBatch::default();
    html::collect_batch(&document, 1.0, &mut batch);
    Ok(batch.text)
}
