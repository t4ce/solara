//! Main-video projection for generic /video/ pages. No site layout or ads run.
use rust_qjs_dom::{DomEngine, DomNode, JsEngine, JsEngineOptions};
use std::{cell::RefCell, rc::Rc, time::Duration};
use url::Url;

pub fn is_video_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path().contains("/video/")
}

#[derive(Debug, PartialEq)]
pub enum PageMedia {
    Source(Url),
    Player(Url),
}

fn https_url(base: &Url, value: &str) -> Option<Url> {
    if value.len() > 8192 {
        return None;
    }
    base.join(value).ok().filter(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}
fn media_url(base: &Url, value: &str) -> Option<Url> {
    https_url(base, value)
        .filter(|url| url.path().ends_with(".mp4") || url.path().ends_with(".m3u8"))
}
fn attr<'a>(node: &'a DomNode, name: &str) -> Option<&'a str> {
    node.attrs
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.value.as_str())
}
fn collect(node: &DomNode, tag: &str, output: &mut Vec<DomNode>) {
    if node
        .tag_name
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case(tag))
    {
        output.push(node.clone());
    }
    for child in &node.children {
        collect(child, tag, output);
    }
}
fn declared_media(value: &serde_json::Value, page: &Url, output: &mut Vec<PageMedia>) {
    if let Some(values) = value.as_array() {
        for value in values {
            declared_media(value, page, output);
        }
    } else if value.is_object() {
        if let Some(graph) = value.get("@graph") {
            declared_media(graph, page, output);
        }
        let video_type = value["@type"].as_str() == Some("VideoObject")
            || value["@type"]
                .as_array()
                .is_some_and(|v| v.iter().any(|t| t == "VideoObject"));
        let same_page = value["url"].as_str().is_none_or(|raw| {
            page.join(raw)
                .is_ok_and(|url| url.host_str() == page.host_str() && url.path() == page.path())
        });
        if video_type && same_page {
            if let Some(url) = value["contentUrl"]
                .as_str()
                .and_then(|v| media_url(page, v))
            {
                output.push(PageMedia::Source(url));
            } else if let Some(url) = value["embedUrl"].as_str().and_then(|v| https_url(page, v)) {
                output.push(PageMedia::Player(url));
            }
        }
    }
}

pub fn page_media(html: &str, page: &Url) -> Result<PageMedia, String> {
    let mut parser = DomEngine::new().map_err(|e| e.to_string())?;
    let artifact = parser
        .parse(html, page.as_str())
        .map_err(|e| e.to_string())?;
    let mut videos = Vec::new();
    collect(&artifact.document, "video", &mut videos);
    if videos.len() == 1 {
        let video = &videos[0];
        let src = attr(video, "src").or_else(|| {
            video.children.iter().find_map(|child| {
                (child.tag_name.as_deref() == Some("source"))
                    .then(|| attr(child, "src"))
                    .flatten()
            })
        });
        if let Some(url) = src.and_then(|v| media_url(page, v)) {
            return Ok(PageMedia::Source(url));
        }
    }
    let mut declared = Vec::new();
    let mut scripts = Vec::new();
    collect(&artifact.document, "script", &mut scripts);
    for script in scripts {
        let text: String = script
            .children
            .iter()
            .filter_map(|n| n.value.as_deref())
            .collect();
        if attr(&script, "type") == Some("application/ld+json")
            && let Ok(value) = serde_json::from_str(&text)
        {
            declared_media(&value, page, &mut declared);
        }
    }
    if declared.len() == 1 {
        return Ok(declared.remove(0));
    }
    if videos.len() > 1 || declared.len() > 1 {
        return Err("Page has multiple main-video candidates".into());
    }
    let mut frames = Vec::new();
    collect(&artifact.document, "iframe", &mut frames);
    if frames.len() == 1
        && let Some(url) = attr(&frames[0], "src").and_then(|v| https_url(page, v))
    {
        return Ok(PageMedia::Player(url));
    }
    Err("Page has no unambiguous supported video source".into())
}

// Keep only the leading setup call. Event handlers, popups and advertising
// after it are never evaluated. Quotes/escapes protect ')' inside URL strings.
fn setup_call(source: &str) -> Option<&str> {
    let source = source.trim_start();
    if !source.starts_with("jwplayer(") {
        return None;
    }
    let start = source.find(".setup(")? + ".setup(".len();
    let mut depth = 1u32;
    let mut quote = None;
    let mut escaped = false;
    for (offset, byte) in source.as_bytes()[start..].iter().copied().enumerate() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == q {
                quote = None;
            }
        } else {
            match byte {
                b'\'' | b'"' | b'`' => quote = Some(byte),
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&source[..start + offset + 1]);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

pub fn player_source(html: &str, player: &Url) -> Result<Url, String> {
    let mut parser = DomEngine::new().map_err(|e| e.to_string())?;
    let artifact = parser
        .parse(html, player.as_str())
        .map_err(|e| e.to_string())?;
    // A player can also expose an ordinary HTML5 source directly.
    if let Ok(PageMedia::Source(url)) = page_media(html, player) {
        return Ok(url);
    }
    let mut js = JsEngine::with_options(JsEngineOptions {
        memory_limit_bytes: 16 * 1024 * 1024,
        stack_limit_bytes: 512 * 1024,
    })
    .map_err(|e| e.to_string())?;
    js.disable_module_loading();
    let unpacked = Rc::new(RefCell::new(None::<String>));
    let result = unpacked.clone();
    js.register_json_function("__captureBootstrap", 1, move |args| {
        let text = args
            .first()
            .and_then(|v| v.as_str())
            .ok_or("Missing bootstrap")?;
        if text.len() > 256 * 1024 {
            return Err("Bootstrap size limit reached".into());
        }
        *result.borrow_mut() = Some(text.to_owned());
        Ok(serde_json::Value::Null)
    })
    .map_err(|e| e.to_string())?;
    // Intercept the packer's eval: capture its output instead of running it.
    js.eval_void("globalThis.eval = __captureBootstrap;", "<unpack-init>")
        .map_err(|e| e.to_string())?;
    let sources = Rc::new(RefCell::new(None::<String>));
    let result = sources.clone();
    js.register_json_function("__captureSources", 1, move |args| {
        let text = args
            .first()
            .and_then(|v| v.as_str())
            .ok_or("Missing sources")?;
        if text.len() > 64 * 1024 {
            return Err("Sources size limit reached".into());
        }
        *result.borrow_mut() = Some(text.to_owned());
        Ok(serde_json::Value::Null)
    })
    .map_err(|e| e.to_string())?;
    js.eval_void("globalThis.jwplayer = function(){return {setup:function(o){__captureSources(JSON.stringify(o.sources || []));}}};", "<player-init>").map_err(|e| e.to_string())?;
    let mut count = 0;
    for script in &artifact.extracted.scripts {
        let source = script
            .get("scriptText")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let packed = source.contains("eval(function(p,a,c,k,e,d)") && source.contains("jwplayer");
        if !packed && setup_call(source).is_none() {
            continue;
        }
        count += 1;
        if count > 4 || source.len() > 256 * 1024 {
            return Err("Player bootstrap limit reached".into());
        }
        let decoded;
        let config = if packed {
            js.eval_void_with_timeout(source, "<player-unpack>", Duration::from_millis(250))
                .map_err(|e| e.to_string())?;
            decoded = unpacked
                .borrow_mut()
                .take()
                .ok_or("Packer produced no configuration")?;
            setup_call(&decoded)
        } else {
            setup_call(source)
        };
        if let Some(config) = config {
            js.eval_void_with_timeout(config, "<player-setup>", Duration::from_millis(250))
                .map_err(|e| e.to_string())?;
            let text = sources
                .borrow_mut()
                .take()
                .ok_or("Player supplied no sources")?;
            let values: Vec<serde_json::Value> =
                serde_json::from_str(&text).map_err(|_| "Invalid player sources")?;
            if let Some(url) = values
                .iter()
                .find_map(|v| v["file"].as_str().and_then(|s| media_url(player, s)))
            {
                return Ok(url);
            }
        }
    }
    Err("Player has no supported HTTPS MP4 or HLS source".into())
}

pub fn video_document(page: &Url, source: &Url) -> String {
    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    format!(
        r#"<!doctype html><html><head><title>Solara online video</title><style>
body {{ margin:12px; background:#fafafa; color:#171717; font-size:18px; line-height:28px; }}
h1 {{ font-size:20px; line-height:30px; }} video {{ display:block; width:100%; height:360px; max-height:calc(100vh - 80px); background:#000; }}
</style></head><body><h1>{}</h1><video id="target-video" width="640" height="360" controls autoplay muted playsinline><source src="{}" type="{}"></video></body></html>"#,
        escape(&page[url::Position::BeforePath..]),
        escape(source.as_str()),
        if source.path().ends_with(".m3u8") {
            "application/vnd.apple.mpegurl"
        } else {
            "video/mp4"
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_path_and_declared_main_video_ignore_unrelated_iframes() {
        let page = Url::parse("http://example.test/video/episode-one").unwrap();
        assert!(is_video_url(&page));
        assert!(!is_video_url(
            &Url::parse("https://example.test/?next=/video/one").unwrap()
        ));
        let html = r#"<iframe src="https://ads.example.test/ad"></iframe><script type="application/ld+json">[{"@type":"VideoObject","url":"http://example.test/video/episode-one","embedUrl":"https://player.example.test/embed-clip.html"}]</script><iframe src="https://player.example.test/embed-clip.html"></iframe>"#;
        assert_eq!(
            page_media(html, &page).unwrap(),
            PageMedia::Player(Url::parse("https://player.example.test/embed-clip.html").unwrap())
        );
        assert!(page_media("<iframe src='https://one.test/'></iframe><iframe src='https://two.test/'></iframe>", &page).is_err());
    }
    #[test]
    fn html5_source_and_signed_query_survive_projection() {
        let page = Url::parse("https://example.test/video/clip").unwrap();
        let html = r#"<video><source src="/movie.mp4?token=a&amp;expires=123"></video>"#;
        let PageMedia::Source(source) = page_media(html, &page).unwrap() else {
            panic!()
        };
        assert_eq!(source.query(), Some("token=a&expires=123"));
        assert!(video_document(&page, &source).contains("token=a&amp;expires=123"));
    }
    #[test]
    fn packed_setup_resolves_hls_without_running_following_page_code() {
        let player = Url::parse("https://player.example.test/embed.html").unwrap();
        let html = r#"<script>eval(function(p,a,c,k,e,d){return p}('jwplayer("main").setup({sources:[{file:"https://cdn.example.test/main.m3u8?token=abc"}]});throw new Error("ad code must not run");',0,0,[],0,{}))</script>"#;
        assert_eq!(
            player_source(html, &player).unwrap().as_str(),
            "https://cdn.example.test/main.m3u8?token=abc"
        );
        assert!(
            player_source(
                "<script>jwplayer('main').setup({sources:[{file:'file:///etc/passwd'}]})</script>",
                &player
            )
            .is_err()
        );
    }
    #[test]
    fn unbounded_unpacker_is_interrupted() {
        let player = Url::parse("https://player.example.test/embed.html").unwrap();
        assert!(player_source("<script>eval(function(p,a,c,k,e,d){while(true){}}('jwplayer',0,0,[],0,{}))</script>", &player).is_err());
    }
}
