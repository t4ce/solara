use rust_qjs_dom::{DOM_ARTIFACT_SCHEMA, DOM_ARTIFACT_VERSION, DomEngine, DomNode};

fn text_content(node: &DomNode) -> String {
    let mut text = String::new();
    node.walk(&mut |entry| {
        if entry.node_name == "#text" {
            if let Some(value) = &entry.value {
                text.push_str(value);
            }
        }
    });
    text
}

#[test]
fn parse5_repairs_malformed_html_and_preserves_namespaces() {
    let html = include_str!("fixtures/html5_edges.html");
    let mut engine = DomEngine::new().expect("engine starts");
    let artifact = engine
        .parse(html, "https://example.test/edges")
        .expect("edge fixture parses");

    assert_eq!(artifact.schema, DOM_ARTIFACT_SCHEMA);
    assert_eq!(artifact.schema_version, DOM_ARTIFACT_VERSION);

    let root = artifact
        .document
        .find_element_by_id("root")
        .expect("main element");
    assert_eq!(root.attribute("data-first"), Some("kept"));

    let entities = artifact
        .document
        .find_element_by_id("entities")
        .expect("entity paragraph");
    assert_eq!(text_content(entities).trim(), "<ok> & 🚀");

    let fostered = artifact
        .document
        .find_element_by_id("fostered")
        .expect("foster-parented paragraph");
    let table = artifact
        .document
        .find_element_by_id("foster")
        .expect("table");
    assert!(
        root.children
            .iter()
            .position(|child| std::ptr::eq(child, fostered))
            < root
                .children
                .iter()
                .position(|child| std::ptr::eq(child, table)),
        "invalid table content should be foster-parented before the table"
    );

    let template = artifact
        .document
        .find_element_by_id("card")
        .expect("template");
    let template_content = template.content.as_deref().expect("template content");
    assert_eq!(
        template_content.children[0].tag_name.as_deref(),
        Some("article")
    );

    let svg = artifact.document.find_element_by_id("vector").expect("svg");
    assert_eq!(
        svg.namespace_uri.as_deref(),
        Some("http://www.w3.org/2000/svg")
    );
    assert_eq!(svg.attribute("viewBox"), Some("0 0 10 10"));
    let svg_link = svg
        .children
        .iter()
        .find(|child| child.tag_name.as_deref() == Some("a"))
        .expect("SVG link");
    let xlink = svg_link
        .attrs
        .iter()
        .find(|attribute| attribute.prefix.as_deref() == Some("xlink"))
        .expect("xlink attribute");
    assert_eq!(
        xlink.namespace.as_deref(),
        Some("http://www.w3.org/1999/xlink")
    );
}
