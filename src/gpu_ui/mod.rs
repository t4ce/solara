//! Software-rendered immediate-mode UI demo (winit + softbuffer):
//!
//! 1. winit window + CPU-drawn colored triangle
//! 2. rectangle and circle rasterization
//! 3. mouse hit-testing to recolor rectangles on click
//! 4. bitmap text rendering ("Click Me" labels)
//! 5. box-model flex row layout for multiple buttons

mod app;
mod draw;
mod layout;
mod renderer;
mod shapes;
mod text;

pub use app::run;
