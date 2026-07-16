//! wgpu renderer for demoui.html elements (excluding document structure tags).

mod app;
mod async_utils;
// Preserved for future Stylo experiments. The active cascade and style table
// now come from RustQJSDom/Lightning CSS; nothing in paint or loading calls it.
mod css;
mod geometry;
mod html;
mod loader;
mod renderer;
mod shapes;
mod text;

pub use app::run;
