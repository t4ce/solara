//! Dormant legacy Stylo path retained for future browser-engine experiments.
#![allow(dead_code, unused_imports)]

mod engine;
mod resolve;

pub use engine::CssEngine;
pub use resolve::ResolvedStyle;
