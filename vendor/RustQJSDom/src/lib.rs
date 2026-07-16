//! A standard-library QuickJS and renderer-free TrueSurfer DOM library.
//!
//! [`JsEngine`] is the reusable, single-thread-owned JavaScript runtime.
//! [`DomEngine`] builds on it to turn HTML into a typed, versioned [`DomArtifact`].
//! Rendering, layout, computed paint, networking, and page-script execution are
//! deliberately outside the DOM pipeline.

#![forbid(unsafe_op_in_unsafe_fn)]

mod artifact;
mod dom_engine;
mod ffi;
mod js_engine;
mod lightning_css;

pub use artifact::{
    AssetIndex, AssetRequest, ComputedStyle, DOM_ARTIFACT_SCHEMA, DOM_ARTIFACT_VERSION,
    DomArtifact, DomAttribute, DomNode, ExtractedArtifacts, NodeStyleRef, SourceMetadata,
    StyleIndex, TimingMetadata, WidgetStats,
};
pub use dom_engine::{DomEngine, DomEngineOptions, DomError, EngineOptions, LoadedStylesheet};
pub use js_engine::{JsEngine, JsEngineOptions, JsError};
pub use lightning_css::color_to_rgba as css_color_to_rgba;
