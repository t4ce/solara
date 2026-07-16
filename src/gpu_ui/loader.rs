use std::path::{Path, PathBuf};

use rust_qjs_dom::{AssetRequest, DomArtifact, DomEngine, DomNode, LoadedStylesheet};
use url::Url;

pub const DEFAULT_HTML_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/demoui.html");

pub struct LoadedPage {
    pub url: Url,
    pub favicon_url: Option<Url>,
    pub title: String,
    pub artifact: DomArtifact,
    pub dom_engine: DomEngine,
}

pub fn load_page(input: Option<&str>) -> Result<LoadedPage, String> {
    let url = resolve_input(input.unwrap_or(DEFAULT_HTML_PATH))?;
    let html = fetch_text(&url)?;
    let mut dom_engine = DomEngine::with_stylesheet_loader(|document_url, base_href, href| {
        let resolved_url = resolve_resource_url(document_url, base_href, href)?;
        let css = fetch_text(&resolved_url)?;
        Ok(LoadedStylesheet::new(resolved_url.to_string(), css))
    })
    .map_err(|error| format!("failed to start DOM engine: {error}"))?;
    let artifact = dom_engine
        .parse(&html, url.as_str())
        .map_err(|error| format!("failed to parse {url}: {error}"))?;
    let base_href = artifact.asset_index.base_href.as_deref();
    let favicon_url = artifact
        .extracted
        .favicon_href
        .as_deref()
        .and_then(|href| resolve_resource_url(url.as_str(), base_href, href).ok());
    trace_asset_requests(&artifact, &url);
    if let Some(favicon_url) = &favicon_url {
        log::trace!(
            target: "solara::assets",
            "favicon_resolved raw_url={:?} resolved_url={} action=metadata-only no_fetch=1",
            artifact.extracted.favicon_href.as_deref().unwrap_or_default(),
            favicon_url,
        );
    }

    let title = find_element(&artifact.document, "title")
        .map(raw_text_content)
        .as_deref()
        .map(str::trim)
        .map(str::to_string)
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Solara".to_string());

    Ok(LoadedPage {
        url,
        favicon_url,
        title,
        artifact,
        dom_engine,
    })
}

fn resolve_resource_url(
    document_url: &str,
    base_href: Option<&str>,
    raw_url: &str,
) -> Result<Url, String> {
    let document = Url::parse(document_url)
        .map_err(|error| format!("invalid document URL {document_url:?}: {error}"))?;
    let base = match base_href.filter(|href| !href.is_empty()) {
        Some(href) => document
            .join(href)
            .map_err(|error| format!("invalid document base URL {href:?}: {error}"))?,
        None => document,
    };
    base.join(raw_url)
        .map_err(|error| format!("invalid resource URL {raw_url:?}: {error}"))
}

fn resolve_asset_request(document_url: &Url, request: &AssetRequest) -> Result<Url, String> {
    let base = Url::parse(&request.base_url)
        .or_else(|_| document_url.join(&request.base_url))
        .map_err(|error| {
            format!(
                "invalid asset base URL {:?} for {:?}: {error}",
                request.base_url, request.raw_url
            )
        })?;
    base.join(&request.raw_url)
        .map_err(|error| format!("invalid asset URL {:?}: {error}", request.raw_url))
}

fn trace_asset_requests(artifact: &DomArtifact, document_url: &Url) {
    let total = artifact.asset_index.requests.len();
    log::trace!(
        target: "solara::assets",
        "asset_batch document_url={} backend={} requests={} kinds={:?} external_css_loaded={} css_load_errors={:?} action=log-only no_fetch=1",
        document_url,
        artifact.asset_index.backend,
        total,
        artifact.asset_index.kind_counts,
        artifact.style_index.external_stylesheet_count,
        artifact.style_index.load_errors,
    );
    for (index, request) in artifact.asset_index.requests.iter().enumerate() {
        match resolve_asset_request(document_url, request) {
            Ok(resolved_url) => log::trace!(
                target: "solara::assets",
                "asset_request index={} total={} kind={} initiator={} tag={} attribute={} path={} media_type={:?} raw_url={:?} base_url={:?} resolved_url={} action={} no_fetch={}",
                index + 1,
                total,
                request.kind,
                request.initiator,
                request.tag,
                request.attribute,
                request.path,
                request.media_type,
                request.raw_url,
                request.base_url,
                resolved_url,
                if request.kind == "stylesheet" { "css-pipeline" } else { "log-only" },
                if request.kind == "stylesheet" { 0 } else { 1 },
            ),
            Err(error) => log::trace!(
                target: "solara::assets",
                "asset_request index={} total={} kind={} initiator={} tag={} attribute={} path={} media_type={:?} raw_url={:?} base_url={:?} resolution_error={:?} action=log-only no_fetch=1",
                index + 1,
                total,
                request.kind,
                request.initiator,
                request.tag,
                request.attribute,
                request.path,
                request.media_type,
                request.raw_url,
                request.base_url,
                error,
            ),
        }
    }
}

fn find_element<'a>(node: &'a DomNode, tag: &str) -> Option<&'a DomNode> {
    if node
        .tag_name
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case(tag))
    {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_element(child, tag))
        .or_else(|| {
            node.content
                .as_deref()
                .and_then(|content| find_element(content, tag))
        })
}

fn raw_text_content(node: &DomNode) -> String {
    let mut text = String::new();
    append_raw_text(node, &mut text);
    text
}

fn append_raw_text(node: &DomNode, text: &mut String) {
    if let ("#text", Some(value)) = (node.node_name.as_str(), &node.value) {
        text.push_str(value);
    }
    for child in &node.children {
        append_raw_text(child, text);
    }
    if let Some(content) = &node.content {
        append_raw_text(content, text);
    }
}

fn resolve_input(input: &str) -> Result<Url, String> {
    if let Ok(url) = Url::parse(input) {
        return match url.scheme() {
            "file" | "http" | "https" => Ok(url),
            scheme => Err(format!("unsupported URL scheme {scheme:?}")),
        };
    }

    let path = PathBuf::from(input);
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|err| format!("cannot read current directory: {err}"))?
            .join(path)
    };
    Url::from_file_path(&absolute)
        .map_err(|_| format!("cannot convert path to file URL: {}", absolute.display()))
}

fn fetch_text(url: &Url) -> Result<String, String> {
    match url.scheme() {
        "file" => {
            let path = url
                .to_file_path()
                .map_err(|_| format!("invalid file URL: {url}"))?;
            std::fs::read_to_string(&path)
                .map_err(|err| format!("failed to read {}: {err}", display_path(&path)))
        }
        "http" | "https" => {
            let mut response = ureq::get(url.as_str())
                .call()
                .map_err(|err| format!("failed to fetch {url}: {err}"))?;
            response
                .body_mut()
                .read_to_string()
                .map_err(|err| format!("failed to read response from {url}: {err}"))
        }
        scheme => Err(format!("unsupported URL scheme {scheme:?}")),
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use url::Url;

    use super::{DEFAULT_HTML_PATH, load_page, resolve_input};

    #[test]
    fn resolves_paths_and_supported_urls() {
        assert_eq!(
            resolve_input("https://example.com/index.html")
                .unwrap()
                .scheme(),
            "https"
        );
        assert_eq!(resolve_input(DEFAULT_HTML_PATH).unwrap().scheme(), "file");
        assert!(resolve_input("data:text/html,hello").is_err());
        assert_eq!(
            super::resolve_resource_url(
                "https://example.com/docs/page.html",
                Some("/assets/"),
                "icons/site.png",
            )
            .unwrap()
            .as_str(),
            "https://example.com/assets/icons/site.png"
        );
    }

    #[test]
    fn loads_current_demo_through_parse5() {
        let page = load_page(None).unwrap();
        assert_eq!(page.title, "HTML Only Visual Elements");
        assert!(page.favicon_url.is_none());
        assert_eq!(page.artifact.schema, "rustqjsdom.artifact");
        assert_eq!(page.artifact.schema_version, 2);
        assert!(super::find_element(&page.artifact.document, "body").is_some());
    }

    #[test]
    fn loads_http_html_and_relative_stylesheet() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let mut request = [0; 1024];
                let length = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..length]);
                let body = if request.starts_with("GET /page.css ") {
                    "main { color: #123456; }"
                } else {
                    "<title>Remote</title><link rel='icon' href='icons/site.png'><link rel='stylesheet' href='page.css'><main id='remote'>Loaded<img src='not-fetched.png'></main>"
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body,
                )
                .unwrap();
            }
        });

        let page = load_page(Some(&format!("http://{address}/index.html"))).unwrap();
        server.join().unwrap();
        assert_eq!(page.title, "Remote");
        let expected_favicon = format!("http://{address}/icons/site.png");
        assert_eq!(
            page.favicon_url.as_ref().map(Url::as_str),
            Some(expected_favicon.as_str())
        );
        let main = page
            .artifact
            .document
            .find_element_by_id("remote")
            .expect("remote main");
        let style = page
            .artifact
            .style_index
            .style(main.style_ref.expect("style ref"))
            .expect("computed style");
        assert_eq!(style.color.as_deref(), Some("#123456"));
        assert_eq!(page.artifact.style_index.external_stylesheet_count, 1);
        assert!(
            page.artifact
                .asset_index
                .requests
                .iter()
                .any(|request| request.raw_url == "not-fetched.png")
        );
    }
}
