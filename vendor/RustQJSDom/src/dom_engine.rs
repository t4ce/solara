use std::error::Error;
use std::fmt;

use serde_json::{Value, json};

use crate::artifact::DomArtifact;
use crate::js_engine::{JsEngine, JsEngineOptions, JsError};
use crate::lightning_css;

const ENTRY_SPECIFIER: &str = "/qjs/entry.mjs";
const PARSE_FUNCTION: &str = "__rustQjsDomParseJson";

/// Resource limits for the DOM pipeline and its QuickJS runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomEngineOptions {
    pub js: JsEngineOptions,
    pub max_html_bytes: usize,
}

impl Default for DomEngineOptions {
    fn default() -> Self {
        Self {
            js: JsEngineOptions::default(),
            max_html_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Compatibility name retained for callers of the initial library release.
pub type EngineOptions = DomEngineOptions;

/// A stylesheet returned by the embedding browser's synchronous resource loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedStylesheet {
    pub resolved_url: String,
    pub css: String,
}

impl LoadedStylesheet {
    pub fn new(resolved_url: impl Into<String>, css: impl Into<String>) -> Self {
        Self {
            resolved_url: resolved_url.into(),
            css: css.into(),
        }
    }
}

type StylesheetLoader =
    dyn FnMut(&str, Option<&str>, &str) -> Result<LoadedStylesheet, String> + 'static;

#[derive(Debug)]
pub enum DomError {
    InputTooLarge { actual: usize, maximum: usize },
    JavaScript(JsError),
    InvalidArtifact(serde_json::Error),
    InvalidContract(String),
}

impl fmt::Display for DomError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "HTML input is {actual} bytes; maximum is {maximum} bytes"
                )
            }
            Self::JavaScript(error) => write!(formatter, "DOM JavaScript failed: {error}"),
            Self::InvalidArtifact(error) => {
                write!(formatter, "DOM artifact has an invalid shape: {error}")
            }
            Self::InvalidContract(error) => {
                write!(formatter, "DOM artifact contract failed: {error}")
            }
        }
    }
}

impl Error for DomError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::JavaScript(error) => Some(error),
            Self::InvalidArtifact(error) => Some(error),
            _ => None,
        }
    }
}

impl From<JsError> for DomError {
    fn from(error: JsError) -> Self {
        Self::JavaScript(error)
    }
}

/// TrueSurfer DOM pipeline hosted by an owned [`JsEngine`].
pub struct DomEngine {
    js: JsEngine,
    options: DomEngineOptions,
}

impl DomEngine {
    pub fn new() -> Result<Self, DomError> {
        Self::with_options(DomEngineOptions::default())
    }

    pub fn with_options(options: DomEngineOptions) -> Result<Self, DomError> {
        Self::with_boxed_stylesheet_loader(
            options,
            Box::new(|_, _, href| {
                Err(format!(
                    "external stylesheet {href:?} was not loaded because the host has no stylesheet loader"
                ))
            }),
        )
    }

    /// Starts the DOM pipeline with a browser-owned external stylesheet loader.
    ///
    /// The callback runs synchronously after Parse5 has built the document and
    /// before Lightning CSS computes style references. It receives the source
    /// document URL, the first `<base href>` when present, and the link's `href`.
    pub fn with_stylesheet_loader<F>(loader: F) -> Result<Self, DomError>
    where
        F: FnMut(&str, Option<&str>, &str) -> Result<LoadedStylesheet, String> + 'static,
    {
        Self::with_options_and_stylesheet_loader(DomEngineOptions::default(), loader)
    }

    pub fn with_options_and_stylesheet_loader<F>(
        options: DomEngineOptions,
        loader: F,
    ) -> Result<Self, DomError>
    where
        F: FnMut(&str, Option<&str>, &str) -> Result<LoadedStylesheet, String> + 'static,
    {
        Self::with_boxed_stylesheet_loader(options, Box::new(loader))
    }

    fn with_boxed_stylesheet_loader(
        options: DomEngineOptions,
        mut loader: Box<StylesheetLoader>,
    ) -> Result<Self, DomError> {
        let mut js = JsEngine::with_options(options.js)?;
        lightning_css::install(&mut js)?;
        js.register_json_function("__rustQjsDomLoadStylesheet", 3, move |arguments| {
            let document_url = arguments
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| String::from("stylesheet document URL must be a string"))?;
            let base_href = match arguments.get(1) {
                Some(Value::String(value)) => Some(value.as_str()),
                Some(Value::Null) | None => None,
                _ => {
                    return Err(String::from(
                        "stylesheet base href must be a string or null",
                    ));
                }
            };
            let href = arguments
                .get(2)
                .and_then(Value::as_str)
                .ok_or_else(|| String::from("stylesheet href must be a string"))?;
            Ok(match loader(document_url, base_href, href) {
                Ok(loaded) => json!({
                    "resolvedUrl": loaded.resolved_url,
                    "css": loaded.css,
                    "error": "",
                }),
                Err(error) => json!({
                    "resolvedUrl": "",
                    "css": "",
                    "error": error,
                }),
            })
        })?;
        js.eval_embedded_module(ENTRY_SPECIFIER)?;
        Ok(Self { js, options })
    }

    /// Access to the same runtime used by the DOM pipeline.
    ///
    /// This lets a browser host install its own JavaScript state without a
    /// second QuickJS runtime. Callers must not replace the private DOM globals.
    pub fn js(&self) -> &JsEngine {
        &self.js
    }

    pub fn js_mut(&mut self) -> &mut JsEngine {
        &mut self.js
    }

    /// Parses HTML into the typed, validated artifact contract.
    pub fn parse(&mut self, html: &str, url: &str) -> Result<DomArtifact, DomError> {
        if html.len() > self.options.max_html_bytes {
            return Err(DomError::InputTooLarge {
                actual: html.len(),
                maximum: self.options.max_html_bytes,
            });
        }

        let value = self.js.call_global_json(
            PARSE_FUNCTION,
            &[json!(html), json!(url), json!(html.len())],
        )?;
        let artifact: DomArtifact =
            serde_json::from_value(value).map_err(DomError::InvalidArtifact)?;
        artifact
            .validate_contract()
            .map_err(DomError::InvalidContract)?;
        Ok(artifact)
    }

    /// Parses HTML and returns the artifact as a dynamic JSON value.
    pub fn parse_value(&mut self, html: &str, url: &str) -> Result<Value, DomError> {
        let artifact = self.parse(html, url)?;
        serde_json::to_value(artifact).map_err(DomError::InvalidArtifact)
    }

    /// Parses HTML and returns compact JSON text.
    pub fn parse_json(&mut self, html: &str, url: &str) -> Result<String, DomError> {
        let artifact = self.parse(html, url)?;
        serde_json::to_string(&artifact).map_err(DomError::InvalidArtifact)
    }
}

#[cfg(test)]
mod tests {
    use super::DomEngine;

    #[test]
    fn starts_and_parses_a_small_document() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                "<!doctype html><title>Hi</title><main id='app'>Hello</main>",
                "https://example.test/",
            )
            .expect("document parses");
        assert_eq!(artifact.schema, "rustqjsdom.artifact");
        assert_eq!(artifact.schema_version, 2);
        assert_eq!(artifact.style_index.backend, "lightningcss@1.0.0-alpha.70");
        assert!(artifact.style_index.node_ref_count > 0);
        assert_eq!(artifact.extracted.title.as_deref(), Some("Hi"));
        assert_eq!(artifact.source.url, "https://example.test/");
        assert_eq!(
            artifact
                .document
                .find_element_by_id("app")
                .and_then(|node| node.tag_name.as_deref()),
            Some("main")
        );
    }

    #[test]
    fn parsing_does_not_execute_page_scripts() {
        let mut engine = DomEngine::new().expect("engine starts");
        engine
            .parse(
                "<script>globalThis.pageScriptRan = true</script><p>safe</p>",
                "about:blank",
            )
            .expect("document parses");
        assert_eq!(
            engine
                .js_mut()
                .eval_json("globalThis.pageScriptRan === true", "<proof>")
                .expect("proof evaluates"),
            serde_json::json!(false)
        );
    }
}
