//! Solara's browser-owned integration boundaries.

#[cfg(feature = "spec-layout")]
pub mod spec_layout;

#[cfg(feature = "spec-layout")]
pub mod native_paint;

/// Address editing and URL selection shared with the native navigator.
pub mod navigation;

pub mod favicon;
