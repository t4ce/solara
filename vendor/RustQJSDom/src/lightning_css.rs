use std::sync::{Arc, RwLock};

use lightningcss::printer::PrinterOptions;
use lightningcss::stylesheet::{MinifyOptions, ParserOptions, StyleAttribute, StyleSheet};
use lightningcss::traits::Parse;
use lightningcss::values::color::CssColor;
use serde_json::{Value, json};

use crate::js_engine::{JsEngine, JsError};

pub(crate) const LIGHTNING_CSS_BACKEND: &str = "lightningcss@1.0.0-alpha.70";

/// Parses any absolute CSS color supported by the hosted Lightning CSS build.
///
/// `currentColor` and system colors intentionally return `None` because they
/// require cascade or platform context that a renderer-neutral helper lacks.
pub fn color_to_rgba(input: &str) -> Option<[f32; 4]> {
    let color = CssColor::parse_string(input).ok()?.to_rgb().ok()?;
    let CssColor::RGBA(rgba) = color else {
        return None;
    };
    Some([
        rgba.red_f32(),
        rgba.green_f32(),
        rgba.blue_f32(),
        rgba.alpha_f32(),
    ])
}

pub(crate) fn install(js: &mut JsEngine) -> Result<(), JsError> {
    js.register_json_function("__rustQjsDomLightningCssParseInlineStyle", 1, |arguments| {
        let input = string_argument(arguments, 0, "inline CSS")?;
        Ok(parse_inline_style(input))
    })?;
    js.register_json_function("__rustQjsDomLightningCssParseStylesheet", 1, |arguments| {
        let input = string_argument(arguments, 0, "stylesheet CSS")?;
        Ok(parse_stylesheet(input))
    })?;
    Ok(())
}

fn string_argument<'a>(
    arguments: &'a [Value],
    index: usize,
    label: &str,
) -> Result<&'a str, String> {
    arguments
        .get(index)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{label} must be a string"))
}

fn parser_options<'a, 'i>(
    filename: &str,
    warnings: &'a Arc<
        RwLock<Vec<lightningcss::error::Error<lightningcss::error::ParserError<'i>>>>,
    >,
) -> ParserOptions<'a, 'i> {
    ParserOptions {
        filename: String::from(filename),
        error_recovery: true,
        warnings: Some(Arc::clone(warnings)),
        ..ParserOptions::default()
    }
}

fn printer_options() -> PrinterOptions<'static> {
    PrinterOptions {
        minify: true,
        ..PrinterOptions::default()
    }
}

fn parse_inline_style(input: &str) -> Value {
    let warnings = Arc::new(RwLock::new(Vec::new()));
    let mut style = match StyleAttribute::parse(input, parser_options("<inline-style>", &warnings))
    {
        Ok(style) => style,
        Err(error) => return failure(error.to_string(), warnings_to_json(&warnings)),
    };
    style.minify(MinifyOptions::default());
    let css = match style.to_css(printer_options()) {
        Ok(result) => result.code,
        Err(error) => return failure(error.to_string(), warnings_to_json(&warnings)),
    };
    let declarations = split_declarations(&css);
    json!({
        "ok": true,
        "backend": LIGHTNING_CSS_BACKEND,
        "css": css,
        "declarations": declarations,
        "warnings": warnings_to_json(&warnings),
    })
}

fn parse_stylesheet(input: &str) -> Value {
    let warnings = Arc::new(RwLock::new(Vec::new()));
    let mut sheet = match StyleSheet::parse(input, parser_options("<stylesheet>", &warnings)) {
        Ok(sheet) => sheet,
        Err(error) => return failure(error.to_string(), warnings_to_json(&warnings)),
    };
    if let Err(error) = sheet.minify(MinifyOptions::default()) {
        return failure(error.to_string(), warnings_to_json(&warnings));
    }
    let css = match sheet.to_css(printer_options()) {
        Ok(result) => result.code,
        Err(error) => return failure(error.to_string(), warnings_to_json(&warnings)),
    };
    json!({
        "ok": true,
        "backend": LIGHTNING_CSS_BACKEND,
        "css": css,
        "warnings": warnings_to_json(&warnings),
    })
}

fn failure(error: String, warnings: Vec<Value>) -> Value {
    json!({
        "ok": false,
        "backend": LIGHTNING_CSS_BACKEND,
        "error": error,
        "warnings": warnings,
    })
}

fn warnings_to_json<'i>(
    warnings: &Arc<RwLock<Vec<lightningcss::error::Error<lightningcss::error::ParserError<'i>>>>>,
) -> Vec<Value> {
    let Ok(warnings) = warnings.read() else {
        return vec![json!({ "message": "Lightning CSS warning lock was poisoned" })];
    };
    warnings
        .iter()
        .map(|warning| json!({ "message": warning.to_string() }))
        .collect()
}

fn split_declarations(css: &str) -> Vec<Value> {
    let mut declarations = Vec::new();
    let mut start = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut paren_depth = 0u32;
    let mut bracket_depth = 0u32;

    for (index, character) in css.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => quote = Some(character),
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth = bracket_depth.saturating_add(1),
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ';' if paren_depth == 0 && bracket_depth == 0 => {
                push_declaration(&css[start..index], &mut declarations);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    push_declaration(&css[start..], &mut declarations);
    declarations
}

fn push_declaration(input: &str, declarations: &mut Vec<Value>) {
    let Some((name, value)) = input.trim().split_once(':') else {
        return;
    };
    let name = name.trim();
    let value = value.trim();
    if name.is_empty() || value.is_empty() {
        return;
    }
    declarations.push(json!({ "name": name, "value": value }));
}

#[cfg(test)]
mod tests {
    use super::{color_to_rgba, parse_inline_style, parse_stylesheet};

    #[test]
    fn parses_named_and_modern_colors_without_stylo() {
        assert_eq!(color_to_rgba("blue"), Some([0.0, 0.0, 1.0, 1.0]));
        assert!(color_to_rgba("oklch(62% .18 250)").is_some());
        assert_eq!(color_to_rgba("currentColor"), None);
    }

    #[test]
    fn parses_and_minifies_inline_declarations() {
        let result = parse_inline_style(
            "color: rgb(255, 0, 0); margin: 1px 2px; background: url('data:x;a:b')",
        );
        assert_eq!(result["ok"], true);
        assert_eq!(result["backend"], "lightningcss@1.0.0-alpha.70");
        assert_eq!(result["declarations"].as_array().map(Vec::len), Some(3));
    }

    #[test]
    fn parses_stylesheets_with_recovery() {
        let result = parse_stylesheet("main { color: rebeccapurple; invalid } p { margin: 2px; }");
        assert_eq!(result["ok"], true);
        assert!(result["css"].as_str().is_some_and(|css| css.contains('p')));
    }
}
