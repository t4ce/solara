use rust_qjs_dom::DomEngine;

#[test]
fn enumerates_html_css_media_and_favicon_requests_without_render_state() {
    let mut engine = DomEngine::new().expect("engine starts");
    let artifact = engine
        .parse(
            r#"
              <base href="/assets/">
              <link rel="icon" href="icons/site.png">
              <style>main { background: url(bg.webp) }</style>
              <main style="background-image: url('inline.png')">
                <img src="hero.png" srcset="hero@2x.png 2x">
                <video src="clip.mp4" poster="poster.jpg"><track src="captions.vtt"></video>
                <iframe src="frame.html"></iframe>
              </main>
            "#,
            "https://example.test/docs/page.html",
        )
        .expect("document parses");

    assert_eq!(artifact.asset_index.backend, "truesurfer-assets@1");
    assert_eq!(artifact.asset_index.base_href.as_deref(), Some("/assets/"));
    for expected in [
        "icons/site.png",
        "bg.webp",
        "inline.png",
        "hero.png",
        "hero@2x.png",
        "clip.mp4",
        "poster.jpg",
        "captions.vtt",
        "frame.html",
    ] {
        assert!(
            artifact
                .asset_index
                .requests
                .iter()
                .any(|request| request.raw_url == expected),
            "missing asset request for {expected:?}"
        );
    }
    assert_eq!(
        artifact.asset_index.request_count,
        artifact.asset_index.requests.len()
    );
    assert_eq!(
        artifact.extracted.favicon_href.as_deref(),
        Some("icons/site.png")
    );
}
