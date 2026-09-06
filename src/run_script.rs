use url::Url;

const LAUNCH_VFILE: &[u8] = b"vFile:launch";

#[derive(Debug)]
pub(crate) struct OpenRequest {
    pub(crate) url: Url,
    pub(crate) source: Option<String>,
}

pub(crate) fn read() -> Result<Option<OpenRequest>, String> {
    let Some(script) = read_launch_script()? else {
        return Ok(None);
    };
    parse(&script)
}

fn parse(script: &str) -> Result<Option<OpenRequest>, String> {
    let mut url = None;
    let mut source = None;

    for line in script
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line == "fs-scope trueosfs" || line == "home" {
            continue;
        }
        if let Some(value) = line.strip_prefix("open ") {
            if url.is_some() {
                return Err(String::from(
                    "surf run script contains more than one open directive",
                ));
            }
            let parsed = Url::parse(value)
                .map_err(|error| format!("invalid surf URL {value:?}: {error}"))?;
            if !matches!(parsed.scheme(), "http" | "https") {
                return Err(format!(
                    "unsupported surf URL scheme {:?}; expected http or https",
                    parsed.scheme()
                ));
            }
            url = Some(parsed);
            continue;
        }
        if let Some(value) = line.strip_prefix("source ") {
            if source.is_some() {
                return Err(String::from(
                    "surf run script contains more than one source directive",
                ));
            }
            if value.is_empty() {
                return Err(String::from("surf run script source is empty"));
            }
            source = Some(String::from(value));
            continue;
        }
        return Err(format!("unknown surf run-script directive {line:?}"));
    }

    match url {
        Some(url) => Ok(Some(OpenRequest { url, source })),
        None if source.is_none() => Ok(None),
        None => Err("surf run script has a source without an open directive".into()),
    }
}

#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
fn read_launch_script() -> Result<Option<String>, String> {
    match trueos::async_fs::block_on(trueos::async_fs::read_file_utf8(LAUNCH_VFILE)) {
        Ok(script) => Ok(Some(script)),
        Err(trueos::async_fs::ERR_NOT_FOUND) => Ok(None),
        Err(error) => Err(format!("could not read launch script: error {error}")),
    }
}

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
fn read_launch_script() -> Result<Option<String>, String> {
    let _ = LAUNCH_VFILE;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_shell2_surf_contract_with_url_crate() {
        let request = parse(
            "fs-scope trueosfs\nopen https://example.com/a/../page?q=hello%20world\nsource apps/common/solara/surf/surf-1.html\n",
        )
        .unwrap().unwrap();

        assert_eq!(
            request.url.as_str(),
            "https://example.com/page?q=hello%20world"
        );
        assert_eq!(
            request.source.as_deref(),
            Some("apps/common/solara/surf/surf-1.html")
        );
    }

    #[test]
    fn home_and_direct_url_need_no_staged_file() {
        assert!(parse("home\n").unwrap().is_none());
        let request = parse("open https://example.com/\n").unwrap().unwrap();
        assert!(request.source.is_none());
        assert!(parse("source page.html").is_err());
    }

    #[test]
    fn rejects_non_web_urls() {
        let error = parse("open file:///tmp/page.html\nsource page.html\n").unwrap_err();
        assert!(error.contains("expected http or https"));
    }
}
