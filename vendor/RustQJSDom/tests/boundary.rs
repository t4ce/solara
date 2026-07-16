use std::fs;
use std::path::Path;

fn visit_modules(dir: &Path, visitor: &mut impl FnMut(&Path, &str)) {
    for entry in fs::read_dir(dir).expect("module directory") {
        let path = entry.expect("module entry").path();
        if path.is_dir() {
            visit_modules(&path, visitor);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("mjs") {
            let source = fs::read_to_string(&path).expect("module source");
            visitor(&path, &source);
        }
    }
}

#[test]
fn embedded_module_graph_has_no_renderer_or_trueos_host_imports() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("js");
    visit_modules(&root, &mut |path, source| {
        for forbidden in ["rendertree", "renderToTree", "trueos:", "paintPlan"] {
            assert!(
                !source.contains(forbidden),
                "{} contains forbidden boundary {forbidden}",
                path.display()
            );
        }
    });
}

#[test]
fn checked_in_schema_matches_the_public_contract_version() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schema/dom-artifact-v2.schema.json"))
            .expect("schema JSON");
    assert_eq!(
        schema["properties"]["schema"]["const"],
        "rustqjsdom.artifact"
    );
    assert_eq!(schema["properties"]["schemaVersion"]["const"], 2);
    assert_eq!(
        schema["properties"]["styleIndex"]["properties"]["backend"]["const"],
        "lightningcss@1.0.0-alpha.70"
    );
}
