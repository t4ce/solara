use std::path::{Path, PathBuf};

use scraper::{Html, Selector};
use url::Url;

pub const DEFAULT_HTML_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/demoui.html");

pub struct LoadedPage {
    pub url: Url,
    pub html: String,
    pub css: String,
    pub title: String,
}

pub fn load_page(input: Option<&str>) -> Result<LoadedPage, String> {
    let url = resolve_input(input.unwrap_or(DEFAULT_HTML_PATH))?;
    let html = fetch_text(&url)?;
    let document = Html::parse_document(&html);
    let base_url = document
        .select(&selector("base[href]")?)
        .next()
        .and_then(|element| element.attr("href"))
        .and_then(|href| url.join(href).ok())
        .unwrap_or_else(|| url.clone());

    let mut css = String::new();
    for element in document.select(&selector("style, link[href]")?) {
        if element.value().name() == "style" {
            append_stylesheet(&mut css, &element.text().collect::<String>());
            continue;
        }
        let link = element;
        let is_stylesheet = link.attr("rel").is_some_and(|rel| {
            rel.split_ascii_whitespace()
                .any(|part| part.eq_ignore_ascii_case("stylesheet"))
        });
        if !is_stylesheet {
            continue;
        }
        let href = link.attr("href").expect("selector requires href");
        let stylesheet_url = base_url
            .join(href)
            .map_err(|err| format!("invalid stylesheet URL {href:?}: {err}"))?;
        append_stylesheet(&mut css, &fetch_text(&stylesheet_url)?);
    }

    let title = document
        .select(&selector("title")?)
        .next()
        .map(|element| element.text().collect::<String>())
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Solara".to_string());

    Ok(LoadedPage {
        url,
        html,
        css,
        title,
    })
}

fn selector(value: &str) -> Result<Selector, String> {
    Selector::parse(value).map_err(|err| format!("invalid internal selector {value:?}: {err}"))
}

fn append_stylesheet(target: &mut String, stylesheet: &str) {
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(stylesheet);
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
    }

    #[test]
    fn loads_default_html_and_linked_css() {
        let page = load_page(None).unwrap();
        assert!(page.html.contains("<selectedcontent>"));
        assert!(page.css.contains("box-sizing: border-box"));
        assert_eq!(page.title, "Complete HTML Element Catalog");
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
                    "<title>Remote</title><link rel='stylesheet' href='page.css'><main>Loaded</main>"
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
        assert!(page.css.contains("color: #123456"));
    }
}
