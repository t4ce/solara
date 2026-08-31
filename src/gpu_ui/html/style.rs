use rust_qjs_dom::{ComputedStyle, StyleIndex, css_color_to_rgba};

use super::HtmlNode;

/// Paint-facing subset of the renderer-neutral RustQJSDom computed style.
#[derive(Clone, Debug, Default)]
pub(super) struct ResolvedStyle {
    pub color: Option<[f32; 4]>,
    pub background_color: Option<[f32; 4]>,
    pub font_size: Option<f32>,
    pub line_height: Option<f32>,
    pub font_family: Option<String>,
    pub font_style: Option<String>,
    pub border_width: Option<f32>,
    pub border_color: Option<[f32; 4]>,
    /// The first, intentionally narrow layout bridge over the open DOM
    /// declaration map.  It is sufficient for the fixed-coordinate visual
    /// fixture; unsupported position values remain retained in the artifact
    /// without changing the existing cascade.
    pub position: Position,
    pub left_px: Option<f32>,
    pub top_px: Option<f32>,
    pub width_px: Option<f32>,
    pub height_px: Option<f32>,
}

/// CSS positioning values currently consumed by Solara's layout adapter.
///
/// RustQJSDom still retains every value in `cascadedDeclarations`.  This enum
/// merely opts the fixed-coordinate fixture into the two values that Solara
/// can lay out faithfully today.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
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
        // Computed font metrics include inherited values and renderer-neutral
        // HTML defaults (for example h1 versus body text). Unlike colors and
        // backgrounds, these values define layout geometry even when they were
        // not directly authored on this node.
        font_size: style.font_size_px,
        line_height: style.line_height_px,
        font_family: style.font_family.clone(),
        font_style: style.font_style.clone(),
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
        position: cascaded(style, "position")
            .map(|value| match value.trim().to_ascii_lowercase().as_str() {
                "relative" => Position::Relative,
                "absolute" => Position::Absolute,
                _ => Position::Static,
            })
            .unwrap_or_default(),
        left_px: cascaded_px(style, "left"),
        top_px: cascaded_px(style, "top"),
        width_px: cascaded_px(style, "width"),
        height_px: cascaded_px(style, "height"),
    }
}

fn cascaded<'a>(style: &'a ComputedStyle, name: &str) -> Option<&'a str> {
    style.cascaded_declarations.get(name).map(String::as_str)
}

fn cascaded_px(style: &ComputedStyle, name: &str) -> Option<f32> {
    let value = cascaded(style, name)?.trim().to_ascii_lowercase();
    if value == "0" {
        return Some(0.0);
    }
    let pixels = value.strip_suffix("px")?.trim().parse::<f32>().ok()?;
    pixels.is_finite().then_some(pixels)
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
    fn consumes_computed_typography_but_only_authored_paint_colors() {
        let defaults = from_computed(&ComputedStyle {
            color: Some("#1f1f1f".into()),
            font_size_px: Some(30.0),
            ..ComputedStyle::default()
        });
        assert_eq!(defaults.color, None);
        assert_eq!(defaults.font_size, Some(30.0));

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
        assert!(resolved.line_height.is_some());
    }

    #[test]
    fn retains_renderer_unmodeled_border_and_css_declarations() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                r#"
                    <style>
                      #target { border-style: dotted; }
                      main#target {
                        border-top: 3px dashed rebeccapurple;
                        border-style: double !important;
                        border-radius: 2px 4px / 6px 8px;
                        border-image: linear-gradient(red, blue) 30 / 10px / 1px round;
                        border-inline-start-color: currentColor;
                        outline: 1px dotted oklch(62% .18 250);
                        box-shadow: 0 0 4px #123456;
                        --site-border-token: 3px double currentColor;
                      }
                    </style>
                    <main id="target">Border surface</main>
                "#,
                "https://solara.test/declaration-surface",
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
        let declarations = &computed.cascaded_declarations;

        assert_eq!(
            declarations.get("border-style"),
            Some(&String::from("double"))
        );
        for property in [
            "border-top",
            "border-radius",
            "border-image",
            "border-inline-start-color",
            "outline",
            "box-shadow",
            "--site-border-token",
        ] {
            assert!(
                declarations.contains_key(property),
                "the DOM style artifact retained {property}"
            );
        }
        assert!(
            !computed
                .authored_properties
                .iter()
                .any(|property| property == "border-style"),
            "the existing renderer whitelist remains unchanged"
        );
    }

    #[test]
    fn does_not_deduplicate_styles_that_only_differ_in_future_renderer_data() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                r#"
                    <style>
                      #dotted { border-style: dotted; }
                      #dashed { border-style: dashed; }
                    </style>
                    <main id="dotted">dotted</main>
                    <main id="dashed">dashed</main>
                "#,
                "https://solara.test/declaration-deduplication",
            )
            .expect("document parses");
        let dotted = artifact
            .document
            .find_element_by_id("dotted")
            .expect("dotted node");
        let dashed = artifact
            .document
            .find_element_by_id("dashed")
            .expect("dashed node");
        let dotted_style_ref = dotted.style_ref.expect("dotted style ref");
        let dashed_style_ref = dashed.style_ref.expect("dashed style ref");

        assert_ne!(dotted_style_ref, dashed_style_ref);
        assert_eq!(
            artifact
                .style_index
                .style(dotted_style_ref)
                .expect("dotted computed style")
                .cascaded_declarations
                .get("border-style"),
            Some(&String::from("dotted"))
        );
        assert_eq!(
            artifact
                .style_index
                .style(dashed_style_ref)
                .expect("dashed computed style")
                .cascaded_declarations
                .get("border-style"),
            Some(&String::from("dashed"))
        );
    }
}
