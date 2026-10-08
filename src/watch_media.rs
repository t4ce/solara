//! Renderer-free watch-page resolution. Untrusted bootstrap code has no native
//! functions, filesystem, module loader, DOM, or network access.
use rust_qjs_dom::{DomEngine, DomNode, JsEngine, JsEngineOptions};
use std::time::Duration;
use url::Url;

pub fn is_watch(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path().contains("/watch/")
}
/// Resolve a displayed watch path against the address's remembered origin.
pub fn shorthand(text: &str, base: &Url) -> Option<Url> {
    text.starts_with('/')
        .then(|| base.join(text).ok())
        .flatten()
        .filter(is_watch)
}
pub fn display(url: &Url) -> String {
    if is_watch(url) {
        url[url::Position::BeforePath..].to_owned()
    } else {
        url.to_string()
    }
}
fn attr<'a>(node: &'a DomNode, name: &str) -> Option<&'a str> {
    node.attrs
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.value.as_str())
}
fn frame(node: &DomNode, page: &Url) -> Option<Url> {
    if node
        .tag_name
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case("iframe"))
    {
        if let Some(url) = attr(node, "src").and_then(|s| page.join(s).ok()) {
            // Do not follow unrelated advertisements or arbitrary iframe origins.
            if url.scheme() == "https"
                && url.host_str() == Some("mixdrop.ag")
                && url.path().starts_with("/e/")
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
            {
                return Some(url);
            }
        }
    }
    node.children.iter().find_map(|child| frame(child, page))
}
pub fn player_frame(html: &str, page: &Url) -> Result<Url, String> {
    let mut parser = DomEngine::new().map_err(|e| e.to_string())?;
    let artifact = parser
        .parse(html, page.as_str())
        .map_err(|e| e.to_string())?;
    frame(&artifact.document, page).ok_or("No supported player iframe found".into())
}
pub fn media_source(html: &str, player: &Url) -> Result<Url, String> {
    let mut parser = DomEngine::new().map_err(|e| e.to_string())?;
    let artifact = parser
        .parse(html, player.as_str())
        .map_err(|e| e.to_string())?;
    let mut js = JsEngine::with_options(JsEngineOptions {
        memory_limit_bytes: 16 * 1024 * 1024,
        stack_limit_bytes: 512 * 1024,
    })
    .map_err(|e| e.to_string())?;
    js.disable_module_loading();
    let captured = std::rc::Rc::new(std::cell::RefCell::new(None::<String>));
    let result = captured.clone();
    js.register_json_function("__captureMedia", 1, move |args| {
        let value = args
            .first()
            .and_then(|v| v.as_str())
            .ok_or("Missing media source")?;
        if value.len() > 8192 {
            return Err("Media URL limit reached".into());
        }
        *result.borrow_mut() = Some(value.to_owned());
        Ok(serde_json::Value::Null)
    })
    .map_err(|e| e.to_string())?;
    js.eval_void("globalThis.MDCore = Object.create(null);", "<media-init>")
        .map_err(|e| e.to_string())?;
    let mut count = 0;
    for script in &artifact.extracted.scripts {
        let source = script
            .get("scriptText")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        // Only the packed player configuration; no ad scripts or external code.
        if source.contains("eval(function(p,a,c,k,e,d)") && source.contains("MDCore") {
            count += 1;
            if count > 4 || source.len() > 256 * 1024 {
                return Err("Player bootstrap limit reached".into());
            }
            js.eval_void_with_timeout(source, "<media-bootstrap>", Duration::from_millis(250))
                .map_err(|e| e.to_string())?;
        }
    }
    // Property access/string conversion also runs under the execution deadline.
    js.eval_void_with_timeout(
        "if (typeof MDCore.wurl === 'string') __captureMedia(MDCore.wurl);",
        "<media-result>",
        Duration::from_millis(250),
    )
    .map_err(|e| e.to_string())?;
    let source = captured
        .borrow_mut()
        .take()
        .ok_or("Player has no MP4 source")?;
    if source.len() > 8192 {
        return Err("Media URL limit reached".into());
    }
    let url = player.join(&source).map_err(|e| e.to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !url.path().ends_with(".mp4")
    {
        return Err("Player source is not an HTTPS MP4".into());
    }
    Ok(url)
}

/// Project the resolved source into a real, minimal HTML video document.
/// The original site is used only for resolution; its layout and ad scripts
/// are not included. Attribute escaping preserves signed query parameters.
pub fn video_document(page: &Url, source: &Url) -> Result<String, String> {
    if !is_watch(page)
        || source.scheme() != "https"
        || source.host_str().is_none()
        || !source.username().is_empty()
        || source.password().is_some()
        || !source.path().ends_with(".mp4")
    {
        return Err("Unsupported watch media projection".into());
    }
    fn escape(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    Ok(format!(
        r#"<!doctype html><html><head><title>Solara watch video</title><style>
body {{ margin:12px; background:#fff; color:#222; font-size:18px; }}
h1 {{ font-size:20px; margin:0 0 12px; }}
video {{ display:block; width:100%; height:360px; max-height:calc(100vh - 80px); background:#000; }}
</style></head><body><h1>{}</h1><video width="640" height="360" controls autoplay playsinline>
<source src="{}" type="video/mp4">Video playback is unavailable.
</video></body></html>"#,
        escape(&display(page)),
        escape(source.as_str())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routing_uses_watch_path_on_any_https_host() {
        for address in [
            "https://one.example/watch/123",
            "https://two.example:8443/watch/episode-one?token=abc",
            "https://three.example/library/watch/clip",
        ] {
            let page = Url::parse(address).unwrap();
            assert!(is_watch(&page));
            assert_eq!(display(&page), page[url::Position::BeforePath..]);
        }
        for address in [
            "https://example.test/watch?v=123",
            "https://example.test/rewatch/123",
            "https://example.test/?next=/watch/123",
            "http://example.test/watch/123",
            "https://user@example.test/watch/123",
        ] {
            assert!(!is_watch(&Url::parse(address).unwrap()));
        }
    }
    #[test]
    fn iframe_and_packed_bootstrap() {
        let page = Url::parse("https://example.test/watch/123").unwrap();
        let player = player_frame(r#"<iframe src="https://evil.test/e/ad"></iframe><iframe src="https://mixdrop.ag/e/fixture"></iframe>"#, &page).unwrap();
        let html = r#"<script>eval(function(p,a,c,k,e,d){return p}('MDCore.wurl="//cdn.example.test/fixture.mp4?token=abc";',0,0,[],0,{}))</script>"#;
        assert_eq!(
            media_source(html, &player).unwrap().as_str(),
            "https://cdn.example.test/fixture.mp4?token=abc"
        );
        assert!(
            media_source(
                "<script>MDCore.wurl='https://evil.test/a.mp4'</script>",
                &player
            )
            .is_err()
        );
    }
    #[test]
    fn runaway_bootstrap_is_interrupted() {
        let player = Url::parse("https://mixdrop.ag/e/fixture").unwrap();
        assert!(
            media_source(
                "<script>eval(function(p,a,c,k,e,d){while(true){}}('MDCore',0,0,[],0,{}))</script>",
                &player
            )
            .is_err()
        );
    }
}
