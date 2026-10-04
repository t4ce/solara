//! Renderer-free watch-page resolution. Untrusted bootstrap code has no native
//! functions, filesystem, module loader, DOM, or network access.
use rust_qjs_dom::{DomEngine, DomNode, JsEngine, JsEngineOptions};
use std::time::Duration;
use url::Url;

pub fn is_watch(url: &Url) -> bool {
    url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some("archivebate.com" | "www.archivebate.com")
        )
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && watch_path(url.path())
}
fn watch_path(path: &str) -> bool {
    path.strip_prefix("/watch/")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}
pub fn shorthand(text: &str) -> Option<Url> {
    watch_path(text)
        .then(|| Url::parse(&format!("https://archivebate.com{text}")).ok())
        .flatten()
}
pub fn display(url: &Url) -> String {
    if is_watch(url) {
        url.path().to_owned()
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapping_is_exact() {
        assert!(shorthand("/watch/123").is_some());
        assert!(shorthand("/watch/123/other").is_none());
        assert!(!is_watch(
            &Url::parse("https://archivebate.com.evil.test/watch/123").unwrap()
        ));
        assert!(!is_watch(
            &Url::parse("https://user@archivebate.com/watch/123").unwrap()
        ));
        assert_eq!(display(&shorthand("/watch/123").unwrap()), "/watch/123");
    }
    #[test]
    fn iframe_and_packed_bootstrap() {
        let page = shorthand("/watch/123").unwrap();
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
