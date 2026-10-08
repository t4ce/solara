//! Renderer-free watch-page resolution. Untrusted bootstrap code has no native
//! functions, filesystem, module loader, DOM, or network access.
use rust_qjs_dom::{DomEngine, DomNode, JsEngine, JsEngineOptions};
use std::time::Duration;
use url::Url;

pub fn is_watch(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path().contains("/watch/")
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routing_uses_watch_path_on_any_http_or_https_host() {
        for address in [
            "https://one.example/watch/123",
            "http://one.example/watch/123",
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
            "https://user@example.test/watch/123",
            "ftp://example.test/watch/123",
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
    fn cache_separates_hosts_and_query_parameters() {
        let urls = [
            "https://one.example/watch/123",
            "https://two.example/watch/123",
            "https://one.example/watch/123?token=abc",
        ];
        let keys: Vec<_> = urls
            .iter()
            .map(|s| cache_key(&Url::parse(s).unwrap()))
            .collect();
        assert_ne!(keys[0], keys[1]);
        assert_ne!(keys[0], keys[2]);
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

/// Resolve a watch page using the shared QuickJS bootstrap resolver and cache
/// the media for the existing desktop decoder. Signed CDN URLs are not logged.
pub(crate) fn cache_media(page: &Url) -> Result<std::path::PathBuf, String> {
    use std::io::Read;
    if !is_watch(page) {
        return Err("Unsupported watch URL".into());
    }
    let key = cache_key(page);
    let directory = super::media_store::root().join("watch");
    std::fs::create_dir_all(&directory).map_err(|_| "Cannot create watch cache")?;
    let destination = directory.join(format!("{key}.mp4"));
    if destination.metadata().is_ok_and(|m| m.len() > 0) {
        return Ok(destination);
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(180)))
        .build()
        .into();
    let html = agent
        .get(page.as_str())
        .header("User-Agent", "Mozilla/5.0")
        .call()
        .map_err(|_| "Watch page request failed")?
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_string()
        .map_err(|_| "Watch page could not be read")?;
    let player = player_frame(&html, page)?;
    let html = agent
        .get(player.as_str())
        .header("User-Agent", "Mozilla/5.0")
        .header("Referer", page.as_str())
        .call()
        .map_err(|_| "Player request failed")?
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_string()
        .map_err(|_| "Player could not be read")?;
    let source = media_source(&html, &player)?;
    println!("solara: watch player resolved; caching source MP4");
    let mut response = agent
        .get(source.as_str())
        .header("User-Agent", "Mozilla/5.0")
        .header("Referer", player.as_str())
        .call()
        .map_err(|_| "Media request failed")?;
    let expected = response
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
    if expected.is_some_and(|n| n > MAX_BYTES) {
        return Err("Media exceeds 2 GiB cache limit".into());
    }
    let temporary = directory.join(format!("{key}.{}.part", std::process::id()));
    let result = (|| {
        let mut file =
            std::fs::File::create(&temporary).map_err(|_| "Cannot create media cache file")?;
        let length = std::io::copy(
            &mut response.body_mut().as_reader().take(MAX_BYTES + 1),
            &mut file,
        )
        .map_err(|_| "Media transfer failed")?;
        if length == 0 || length > MAX_BYTES || expected.is_some_and(|n| n != length) {
            return Err("Media transfer was incomplete or oversized");
        }
        drop(file);
        let mut file =
            std::fs::File::open(&temporary).map_err(|_| "Cannot validate media cache")?;
        let mut header = [0u8; 12];
        file.read_exact(&mut header)
            .map_err(|_| "Media is not an MP4")?;
        if &header[4..8] != b"ftyp" {
            return Err("Media is not an MP4");
        }
        std::fs::rename(&temporary, &destination).map_err(|_| "Cannot publish media cache")?;
        println!("solara: source MP4 cached ({length} bytes)");
        Ok(destination)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(str::to_owned)
}

fn cache_key(page: &Url) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hash = DefaultHasher::new();
    page.as_str().hash(&mut hash);
    format!("{:016x}", hash.finish())
}
