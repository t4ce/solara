use rust_qjs_dom::{DomEngine, LoadedStylesheet};

fn style_for_id<'a>(
    artifact: &'a rust_qjs_dom::DomArtifact,
    id: &str,
) -> &'a rust_qjs_dom::ComputedStyle {
    let node = artifact
        .document
        .find_element_by_id(id)
        .unwrap_or_else(|| panic!("element #{id}"));
    artifact
        .style_index
        .style(
            node.style_ref
                .unwrap_or_else(|| panic!("style ref for #{id}")),
        )
        .unwrap_or_else(|| panic!("computed style for #{id}"))
}

#[test]
fn lightning_css_runs_immediately_after_parse5_and_applies_the_cascade() {
    let mut engine = DomEngine::new().expect("engine starts");
    let artifact = engine
        .parse(
            r#"
                <style>
                  main { color: red; margin: 1px 2px 3px 4px; }
                  body main.card { color: blue !important; border: 2px solid #123456; }
                </style>
                <main id="target" class="card" style="color: green">Styled</main>
            "#,
            "https://example.test/page",
        )
        .expect("styled document parses");

    let style = style_for_id(&artifact, "target");
    assert_eq!(style.color.as_deref(), Some("#0000ff"));
    assert_eq!(style.margin_top_px, Some(1.0));
    assert_eq!(style.margin_right_px, Some(2.0));
    assert_eq!(style.margin_bottom_px, Some(3.0));
    assert_eq!(style.margin_left_px, Some(4.0));
    assert_eq!(style.border_width_px, Some(2.0));
    assert_eq!(style.border_color.as_deref(), Some("#123456"));
    assert!(style.authored_properties.iter().any(|name| name == "color"));
    assert!(
        style
            .authored_properties
            .iter()
            .any(|name| name == "margin")
    );
    assert_eq!(artifact.style_index.inline_style_count, 1);
    assert!(artifact.style_index.rule_count >= 2);
    assert!(artifact.timings.total_ms >= artifact.timings.parse5_ms);
    assert!(artifact.timings.total_ms >= artifact.timings.lightning_css_ms);
}

#[test]
fn browser_owned_loader_feeds_external_css_into_the_same_style_index() {
    let mut engine = DomEngine::with_stylesheet_loader(|document_url, base_href, href| {
        assert_eq!(document_url, "https://example.test/docs/page.html");
        assert_eq!(base_href, Some("/assets/"));
        assert_eq!(href, "site.css");
        Ok(LoadedStylesheet::new(
            "https://example.test/assets/site.css",
            "#external { color: rgb(18, 52, 86); font-size: 21px; }",
        ))
    })
    .expect("engine starts");

    let artifact = engine
        .parse(
            "<base href='/assets/'><link rel='stylesheet' href='site.css'><p id='external'>Loaded</p>",
            "https://example.test/docs/page.html",
        )
        .expect("external stylesheet is loaded and applied");
    let style = style_for_id(&artifact, "external");
    assert_eq!(style.color.as_deref(), Some("#123456"));
    assert_eq!(style.font_size_px, Some(21.0));
    assert_eq!(artifact.style_index.external_stylesheet_count, 1);
    assert!(artifact.style_index.load_errors.is_empty());
}

#[test]
fn computes_native_absolute_and_relative_font_sizes_before_the_handoff() {
    let mut engine = DomEngine::new().expect("engine starts");
    let artifact = engine
        .parse(
            r#"
                <style>
                  html { font-size: 20px; }
                  #parent { font-size: 150%; line-height: 1.5; }
                  #em { font-size: .5em; }
                  #rem { font-size: 2rem; }
                  #points { font-size: 12pt; }
                  #shorthand { font: 24px/2 sans-serif; }
                </style>
                <html id="root"><main id="parent">
                  <span id="em">em</span>
                  <span id="rem">rem</span>
                  <span id="points">points</span>
                  <span id="shorthand">shorthand</span>
                </main></html>
            "#,
            "https://example.test/typography",
        )
        .expect("styled document parses");

    let html_style = style_for_id(&artifact, "root");
    let parent = style_for_id(&artifact, "parent");
    let em = style_for_id(&artifact, "em");
    let rem = style_for_id(&artifact, "rem");
    let points = style_for_id(&artifact, "points");
    let shorthand = style_for_id(&artifact, "shorthand");

    assert_eq!(html_style.font_size_px, Some(20.0));
    assert_eq!(parent.font_size_px, Some(30.0));
    assert_eq!(parent.line_height_px, Some(45.0));
    assert_eq!(em.font_size_px, Some(15.0));
    assert_eq!(em.line_height_px, Some(22.5));
    assert_eq!(rem.font_size_px, Some(40.0));
    assert!((points.font_size_px.expect("point size") - 16.0).abs() < 0.001);
    assert_eq!(shorthand.font_size_px, Some(24.0));
    assert_eq!(shorthand.line_height_px, Some(48.0));
}

#[test]
fn missing_external_css_is_reported_without_losing_the_dom() {
    let mut engine = DomEngine::new().expect("engine starts");
    let artifact = engine
        .parse(
            "<link rel='stylesheet' href='missing.css'><main id='content'>Still parsed</main>",
            "https://example.test/",
        )
        .expect("missing CSS is non-fatal");
    assert!(artifact.document.find_element_by_id("content").is_some());
    assert_eq!(artifact.style_index.external_stylesheet_count, 0);
    assert_eq!(artifact.style_index.load_errors.len(), 1);
}
