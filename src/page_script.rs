use std::time::Duration;

use rust_qjs_dom::{DomArtifact, JsEngine};

const SCRIPT_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoadedPageScript {
    pub(crate) resolved_url: String,
    pub(crate) source: String,
}

impl LoadedPageScript {
    pub(crate) fn new(resolved_url: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            resolved_url: resolved_url.into(),
            source: source.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageScriptSource {
    Inline,
    External,
}

impl PageScriptSource {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::External => "external",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PageScriptExecution {
    pub(crate) order: usize,
    pub(crate) source_kind: PageScriptSource,
    pub(crate) filename: String,
    pub(crate) bytes: usize,
}

/// Executes at most one page script from an already validated DOM artifact.
///
/// Script tags stay untouched in `artifact.extracted.scripts`. The first tag in
/// document order whose MIME type is a supported classic JavaScript type is the
/// only candidate. Inline source is evaluated directly. External source must
/// already have a matching `kind=script` asset request and is obtained through
/// the browser-owned loader before evaluation in the retained DOM QuickJS
/// runtime. The active caller supplies only a trusted, repository-embedded
/// fixture. Arbitrary network page code needs an isolated page realm or
/// equivalent protection before it can safely share parser-private globals.
pub(crate) fn execute_first_page_script<F>(
    js: &mut JsEngine,
    artifact: &DomArtifact,
    mut load_external_script: F,
) -> Result<Option<PageScriptExecution>, String>
where
    F: FnMut(&str, Option<&str>, &str) -> Result<LoadedPageScript, String>,
{
    for (fallback_order, script) in artifact.extracted.scripts.iter().enumerate() {
        let object = script
            .as_object()
            .ok_or_else(|| format!("script artifact {fallback_order} is not an object"))?;
        let order = object
            .get("order")
            .and_then(|value| value.as_u64())
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(fallback_order);
        let tag_html = object
            .get("tagHtml")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let media_type = html_attribute(tag_html, "type").unwrap_or_default();
        if !is_classic_javascript_type(&media_type) {
            continue;
        }

        let src = object
            .get("src")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let (source_kind, filename, source) = if src.is_empty() {
            (
                PageScriptSource::Inline,
                format!("{}#inline-script-{order}", artifact.source.url),
                object
                    .get("scriptText")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_owned(),
            )
        } else {
            let request = artifact
                .asset_index
                .requests
                .iter()
                .find(|request| request.kind == "script" && request.raw_url == src)
                .ok_or_else(|| {
                    format!(
                        "external script {order} with src {src:?} has no matching asset request"
                    )
                })?;
            let loaded = load_external_script(
                &artifact.source.url,
                artifact.asset_index.base_href.as_deref(),
                &request.raw_url,
            )
            .map_err(|error| format!("external script {order} load failed: {error}"))?;
            let LoadedPageScript {
                resolved_url,
                source,
            } = loaded;
            let filename = if resolved_url.is_empty() {
                request.raw_url.clone()
            } else {
                resolved_url
            };
            (PageScriptSource::External, filename, source)
        };

        let bytes = source.len();
        js.eval_void_with_timeout(&source, &filename, SCRIPT_TIMEOUT)
            .map_err(|error| format!("page script {order} failed: {error}"))?;
        return Ok(Some(PageScriptExecution {
            order,
            source_kind,
            filename,
            bytes,
        }));
    }

    Ok(None)
}

fn is_classic_javascript_type(media_type: &str) -> bool {
    let essence = media_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    matches!(
        essence.as_str(),
        "" | "text/javascript"
            | "application/javascript"
            | "text/ecmascript"
            | "application/ecmascript"
    )
}

fn html_attribute(tag: &str, name: &str) -> Option<String> {
    let opening_tag = tag.split_once('>').map_or(tag, |(opening, _)| opening);
    let lowercase = opening_tag.to_ascii_lowercase();
    let wanted = name.to_ascii_lowercase();
    let mut cursor = 0usize;
    while let Some(relative) = lowercase[cursor..].find(wanted.as_str()) {
        let start = cursor + relative;
        let before_ok = start == 0
            || lowercase.as_bytes()[start - 1].is_ascii_whitespace()
            || lowercase.as_bytes()[start - 1] == b'<';
        let after = start + wanted.len();
        let after_ok = lowercase
            .as_bytes()
            .get(after)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'=');
        if !before_ok || !after_ok {
            cursor = after;
            continue;
        }

        let mut value_start = after;
        while lowercase
            .as_bytes()
            .get(value_start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            value_start += 1;
        }
        if lowercase.as_bytes().get(value_start) != Some(&b'=') {
            return Some(String::new());
        }
        value_start += 1;
        while lowercase
            .as_bytes()
            .get(value_start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            value_start += 1;
        }

        let quote = *opening_tag.as_bytes().get(value_start)?;
        if matches!(quote, b'\'' | b'"') {
            value_start += 1;
            let length = opening_tag.as_bytes()[value_start..]
                .iter()
                .position(|byte| *byte == quote)?;
            return Some(opening_tag[value_start..value_start + length].to_owned());
        }
        let length = opening_tag.as_bytes()[value_start..]
            .iter()
            .position(|byte| byte.is_ascii_whitespace() || *byte == b'>')
            .unwrap_or(opening_tag.len() - value_start);
        return Some(opening_tag[value_start..value_start + length].to_owned());
    }
    None
}

#[cfg(test)]
mod tests {
    use rust_qjs_dom::DomEngine;

    use super::{LoadedPageScript, PageScriptSource, execute_first_page_script};

    #[test]
    fn executes_only_the_first_inline_classic_script_and_preserves_the_inventory() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                r#"
                <script>const type = "module"; globalThis.firstScriptRuns = (globalThis.firstScriptRuns || 0) + 1;</script>
                <script>globalThis.secondScriptRan = true;</script>
                "#,
                "https://example.test/inline.html",
            )
            .expect("document parses");

        let execution = execute_first_page_script(engine.js_mut(), &artifact, |_, _, href| {
            Err(format!("unexpected external script {href:?}"))
        })
        .expect("first script executes")
        .expect("an inline script is selected");

        assert_eq!(execution.order, 0);
        assert_eq!(execution.source_kind, PageScriptSource::Inline);
        assert_eq!(artifact.extracted.script_count, 2);
        assert_eq!(artifact.extracted.scripts.len(), 2);
        assert_eq!(
            artifact.extracted.scripts[1]["scriptText"].as_str(),
            Some("globalThis.secondScriptRan = true;")
        );
        assert_eq!(
            engine
                .js_mut()
                .eval_json("globalThis.firstScriptRuns === 1", "<proof>")
                .expect("proof evaluates")
                .as_bool(),
            Some(true)
        );
        assert_eq!(
            engine
                .js_mut()
                .eval_json("globalThis.secondScriptRan === true", "<proof>")
                .expect("proof evaluates")
                .as_bool(),
            Some(false)
        );
    }

    #[test]
    fn loads_the_first_external_script_from_the_asset_stage() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                r#"
                <base href="/assets/">
                <script src="probe.js"></script>
                <script src="later.js"></script>
                "#,
                "https://example.test/docs/page.html",
            )
            .expect("document parses");
        let mut loader_calls = 0usize;

        let execution = execute_first_page_script(
            engine.js_mut(),
            &artifact,
            |document_url, base_href, href| {
                loader_calls += 1;
                assert_eq!(document_url, "https://example.test/docs/page.html");
                assert_eq!(base_href, Some("/assets/"));
                assert_eq!(href, "probe.js");
                Ok(LoadedPageScript::new(
                    "https://example.test/assets/probe.js",
                    "globalThis.externalScriptRuns = (globalThis.externalScriptRuns || 0) + 1;",
                ))
            },
        )
        .expect("external script executes")
        .expect("an external script is selected");

        assert_eq!(loader_calls, 1);
        assert_eq!(execution.order, 0);
        assert_eq!(execution.source_kind, PageScriptSource::External);
        assert_eq!(execution.filename, "https://example.test/assets/probe.js");
        assert_eq!(artifact.extracted.script_count, 2);
        assert_eq!(
            artifact.extracted.scripts[0]["src"].as_str(),
            Some("probe.js")
        );
        assert_eq!(
            artifact.extracted.scripts[1]["src"].as_str(),
            Some("later.js")
        );
        assert_eq!(
            artifact.asset_index.kind_counts.get("script").copied(),
            Some(2)
        );
        assert_eq!(
            artifact
                .asset_index
                .requests
                .iter()
                .filter(|request| request.kind == "script")
                .map(|request| request.raw_url.as_str())
                .collect::<Vec<_>>(),
            vec!["probe.js", "later.js"]
        );
        assert_eq!(
            engine
                .js_mut()
                .eval_json("globalThis.externalScriptRuns === 1", "<proof>")
                .expect("proof evaluates")
                .as_bool(),
            Some(true)
        );
        assert_eq!(
            engine
                .js_mut()
                .eval_json("globalThis.laterExternalScriptRan === true", "<proof>")
                .expect("proof evaluates")
                .as_bool(),
            Some(false)
        );
    }

    #[test]
    fn skips_import_maps_and_modules_before_the_first_classic_script() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                r#"
                <script type="importmap">{"imports": {}}</script>
                <script type="module">globalThis.moduleScriptRan = true;</script>
                <script type="text/javascript; charset=utf-8">globalThis.classicScriptRan = true;</script>
                "#,
                "https://example.test/types.html",
            )
            .expect("document parses");

        let execution = execute_first_page_script(engine.js_mut(), &artifact, |_, _, href| {
            Err(format!("unexpected external script {href:?}"))
        })
        .expect("classic script executes")
        .expect("a classic script is selected");

        assert_eq!(execution.order, 2);
        assert_eq!(artifact.extracted.script_count, 3);
        assert_eq!(
            engine
                .js_mut()
                .eval_json("globalThis.moduleScriptRan === true", "<proof>")
                .expect("proof evaluates")
                .as_bool(),
            Some(false)
        );
        assert_eq!(
            engine
                .js_mut()
                .eval_json("globalThis.classicScriptRan === true", "<proof>")
                .expect("proof evaluates")
                .as_bool(),
            Some(true)
        );
    }
}
