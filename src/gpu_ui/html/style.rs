use rust_qjs_dom::{ComputedStyle, StyleIndex, css_color_to_rgba};

use super::HtmlNode;

/// Paint-facing subset of the renderer-neutral RustQJSDom computed style.
#[derive(Clone, Debug, Default)]
pub(super) struct ResolvedStyle {
    pub color: Option<[f32; 4]>,
    pub background_color: Option<[f32; 4]>,
    pub font_size: Option<f32>,
    pub border_width: Option<f32>,
    pub border_color: Option<[f32; 4]>,
}

pub(super) fn resolve(style_index: &StyleIndex, node: &HtmlNode) -> ResolvedStyle {
    node.style_ref
        .and_then(|style_ref| style_index.style(style_ref))
        .map(from_computed)
        .unwrap_or_default()
}

fn from_computed(style: &ComputedStyle) -> ResolvedStyle {
    ResolvedStyle {
        color: authored(style, &["color"])
            .then(|| style.color.as_deref().and_then(css_color_to_rgba))
            .flatten(),
        background_color: authored(style, &["background", "background-color"])
            .then(|| {
                style
                    .background_color
                    .as_deref()
                    .and_then(css_color_to_rgba)
            })
            .flatten(),
        font_size: authored(style, &["font", "font-size"])
            .then_some(style.font_size_px)
            .flatten(),
        border_width: authored(
            style,
            &[
                "border",
                "border-width",
                "border-top-width",
                "border-right-width",
                "border-bottom-width",
                "border-left-width",
            ],
        )
        .then_some(style.border_width_px)
        .flatten(),
        border_color: authored(
            style,
            &[
                "border",
                "border-color",
                "border-top-color",
                "border-right-color",
                "border-bottom-color",
                "border-left-color",
            ],
        )
        .then(|| style.border_color.as_deref().and_then(css_color_to_rgba))
        .flatten(),
    }
}

fn authored(style: &ComputedStyle, names: &[&str]) -> bool {
    style
        .authored_properties
        .iter()
        .any(|property| names.iter().any(|name| property == name))
}

#[cfg(test)]
mod tests {
    use rust_qjs_dom::{ComputedStyle, DomEngine};

    use super::from_computed;

    #[test]
    fn ignores_user_agent_defaults_but_consumes_author_css() {
        let defaults = from_computed(&ComputedStyle {
            color: Some("#1f1f1f".into()),
            font_size_px: Some(30.0),
            ..ComputedStyle::default()
        });
        assert_eq!(defaults.color, None);
        assert_eq!(defaults.font_size, None);

        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                "<style>#target { color: rebeccapurple; font-size: 21px }</style><main id='target'>x</main>",
                "https://solara.test/",
            )
            .expect("document parses");
        let node = artifact
            .document
            .find_element_by_id("target")
            .expect("target node");
        let computed = artifact
            .style_index
            .style(node.style_ref.expect("style ref"))
            .expect("computed style");
        let resolved = from_computed(computed);
        assert!(resolved.color.is_some());
        assert_eq!(resolved.font_size, Some(21.0));
    }
}
