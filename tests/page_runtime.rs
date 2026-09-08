use rust_qjs_dom::DomEngine;
use solara::{
    page_runtime::PageRuntime,
    spec_layout::{DocumentConfig, SpecLayout, Viewport},
};
fn page(html: &str) -> (PageRuntime, SpecLayout) {
    let artifact = DomEngine::new()
        .unwrap()
        .parse(html, "http://127.0.0.1:8338/")
        .unwrap();
    let layout = SpecLayout::from_artifact(
        &artifact,
        DocumentConfig {
            viewport: Some(Viewport {
                window_size: (800, 600),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .unwrap();
    (PageRuntime::new(&artifact, &layout).unwrap(), layout)
}
#[test]
fn fetch_promises_fragments_and_click_update_retained_nodes() {
    let (mut runtime, mut layout) =
        page("<h1 id='title'>Loading</h1><button id='go'>Select</button><div id='data'></div>");
    let title = layout.document().query_selector("#title").unwrap().unwrap();
    runtime.execute(r#"
      fetch('/api/schema').then(r => r.json()).then(data => {
        document.getElementById('title').textContent = data.title;
        document.getElementById('data').innerHTML = '<table><tr><td>A &amp; B</td></tr></table>';
      });
      document.getElementById('go').onclick = () => document.getElementById('title').classList.add('selected');
    "#, "test.js").unwrap();
    let request = runtime.take_request().unwrap();
    assert_eq!(request.url, "http://127.0.0.1:8338/api/schema");
    runtime
        .complete(request.id, Ok(r#"{"title":"System Overview"}"#.into()))
        .unwrap();
    assert!(runtime.tick(0, &mut layout).unwrap());
    assert_eq!(
        layout.document().query_selector("#title").unwrap(),
        Some(title)
    );
    assert!(
        layout
            .document()
            .query_selector("#data table tbody tr td")
            .unwrap()
            .is_some()
    );
    let button = layout.document().query_selector("#go").unwrap().unwrap();
    runtime.click(button).unwrap();
    runtime.tick(1, &mut layout).unwrap();
    assert_eq!(
        layout.document().query_selector("h1.selected").unwrap(),
        Some(title)
    );
    layout.resolve(0.0).unwrap();
}
#[test]
fn page_is_isolated_and_transport_rejections_reach_catch() {
    let (mut runtime, mut layout) = page("<div id='result'></div>");
    runtime.execute(r#"
      if (typeof __rustQjsDomParseJson !== 'undefined' || typeof __pageRecord !== 'undefined') throw Error('parser leaked');
      Promise.all([
        fetch('http://elsewhere.test/').then(() => 'bad', () => 'origin'),
        fetch('/write', {method:'POST'}).then(() => 'bad', () => 'method')
      ]).then(results => document.getElementById('result').className = results.join(' '));
    "#, "test.js").unwrap();
    runtime.tick(0, &mut layout).unwrap();
    assert!(runtime.take_request().is_none());
    assert!(
        layout
            .document()
            .query_selector(".origin.method")
            .unwrap()
            .is_some()
    );
}
#[test]
fn script_and_promise_loops_are_interrupted() {
    let (mut runtime, _) = page("<p>Still rendered</p>");
    assert!(runtime.execute("while(true) {}", "loop.js").is_err());
    let (mut runtime, mut layout) = page("<p>Still rendered</p>");
    runtime
        .execute("Promise.resolve().then(() => {while(true) {}})", "loop.js")
        .unwrap();
    assert!(runtime.tick(0, &mut layout).is_err());
}

#[test]
fn malformed_mirror_and_exception_getters_cannot_escape_the_budget() {
    let (mut runtime, mut layout) = page("<div id='a'></div><div id='b'></div>");
    runtime.execute("const a=document.getElementById('a'), b=document.getElementById('b'); b._id=a._id; a.appendChild(b);", "tamper.js").unwrap();
    assert!(runtime.tick(0, &mut layout).is_err());
    let (mut runtime, _) = page("<p>Static page</p>");
    assert!(
        runtime
            .execute("throw { toString() { while(true) {} } };", "getter.js")
            .is_err()
    );
}
