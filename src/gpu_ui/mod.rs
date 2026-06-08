//! GPU-backed immediate-mode UI demo built in five incremental steps:
//!
//! 1. winit window + wgpu colored triangle (top-left corner)
//! 2. shape shader for arbitrary rectangles and circles
//! 3. mouse hit-testing to recolor rectangles on click
//! 4. glyphon text rendering ("Click Me" labels)
//! 5. box-model flex row layout for multiple buttons

mod app;
mod layout;
mod renderer;
mod shapes;

pub use app::run;
