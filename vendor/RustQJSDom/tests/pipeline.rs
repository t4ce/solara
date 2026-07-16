use rust_qjs_dom::DomEngine;
use serde_json::Value;

fn contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(object) => {
            object.contains_key(key) || object.values().any(|value| contains_key(value, key))
        }
        Value::Array(values) => values.iter().any(|value| contains_key(value, key)),
        _ => false,
    }
}

#[test]
fn emits_dom_widgets_and_metadata_without_render_artifacts() {
    let html = include_str!("fixtures/forms.html");
    let mut engine = DomEngine::new().expect("engine starts");
    let artifact = engine
        .parse(html, "https://example.test/forms")
        .expect("fixture parses");

    assert_eq!(artifact.schema, "rustqjsdom.artifact");
    assert_eq!(artifact.schema_version, 2);
    assert_eq!(artifact.source.bytes, html.len());
    assert_eq!(artifact.extracted.title.as_deref(), Some("DOM fixture"));
    assert_eq!(artifact.extracted.style_count, 1);
    assert_eq!(artifact.extracted.script_count, 1);
    assert!(artifact.widget_stats.widgets >= 5);
    assert_eq!(artifact.style_index.backend, "lightningcss@1.0.0-alpha.70");
    assert_eq!(artifact.style_index.stylesheet_count, 1);
    let main = artifact
        .document
        .find_element_by_id("content")
        .expect("main element");
    let style = artifact
        .style_index
        .style(main.style_ref.expect("main style reference"))
        .expect("main computed style");
    assert_eq!(style.color.as_deref(), Some("#663399"));

    let artifact = serde_json::to_value(artifact).expect("artifact serializes");

    assert!(!contains_key(&artifact, "renderTree"));
    assert!(!contains_key(&artifact, "layoutTrace"));
    assert!(!contains_key(&artifact, "paintPlan"));
    assert!(!contains_key(&artifact, "paintedBoxes"));
    assert!(!contains_key(&artifact, "paint"));
}

#[test]
fn engine_can_parse_multiple_documents() {
    let mut engine = DomEngine::new().expect("engine starts");
    let first = engine.parse("<h1>One</h1>", "one:").expect("first parse");
    let second = engine.parse("<h2>Two</h2>", "two:").expect("second parse");
    assert_eq!(first.source.url, "one:");
    assert_eq!(second.source.url, "two:");
}

#[test]
fn engine_remains_reusable_across_a_document_batch() {
    let mut engine = DomEngine::new().expect("engine starts");
    for index in 0..128 {
        let html = format!("<main id='item-{index}'><p>Document {index}</p></main>");
        let artifact = engine
            .parse(&html, &format!("https://example.test/{index}"))
            .expect("batch document parses");
        assert_eq!(artifact.source.bytes, html.len());
        assert!(
            artifact
                .document
                .find_element_by_id(&format!("item-{index}"))
                .is_some()
        );
    }
}
