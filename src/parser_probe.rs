//! Renderer-free startup probe for Solara's built-in DOM handoff.

use core::fmt;

use rust_qjs_dom::{DomArtifact, DomEngine, LoadedStylesheet};

const DEMO_UI_CSS: &str = include_str!("../docs/demoui.css");
const TEXT_AND_BORDERS_CSS: &str = include_str!("../docs/TextAndBorders.css");

#[derive(Clone, Copy)]
struct EmbeddedPage {
    name: &'static str,
    url: &'static str,
    html: &'static str,
}

const PAGES: [EmbeddedPage; 5] = [
    EmbeddedPage {
        name: "trueos-home",
        url: "https://trueos.eu/",
        html: include_str!("../docs/TrueOsHome.html"),
    },
    EmbeddedPage {
        name: "all-elements",
        url: "trueos://solara/docs/demoui.html",
        html: include_str!("../docs/demoui.html"),
    },
    EmbeddedPage {
        name: "text-and-borders",
        url: "trueos://solara/docs/TextAndBorders.html",
        html: include_str!("../docs/TextAndBorders.html"),
    },
    EmbeddedPage {
        name: "divs-and-panels",
        url: "trueos://solara/docs/DivsAndPanels.html",
        html: include_str!("../docs/DivsAndPanels.html"),
    },
    EmbeddedPage {
        name: "flow-and-forms",
        url: "trueos://solara/docs/FlowAndForms.html",
        html: include_str!("../docs/FlowAndForms.html"),
    },
];

pub(crate) fn run() -> Result<(), String> {
    let suite_started = monotonic_nanos();
    let engine_started = monotonic_nanos();
    let mut engine = DomEngine::with_stylesheet_loader(load_embedded_stylesheet)
        .map_err(|error| format!("DOM engine initialization: {error}"))?;
    let engine_ns = monotonic_nanos().saturating_sub(engine_started);

    report_info(format_args!(
        "solara: parse-suite start pages={} engine_us={} renderer=none scripts=retained-inert",
        PAGES.len(),
        nanos_to_micros(engine_ns),
    ));

    let mut ready_pages = 0usize;
    let mut source_bytes = 0usize;
    let mut node_count = 0usize;
    let mut style_slots = 0usize;
    let mut asset_requests = 0usize;
    let mut parse_ns = 0u64;

    for page in PAGES {
        let started = monotonic_nanos();
        let artifact = engine
            .parse(page.html, page.url)
            .map_err(|error| format!("{}: {error}", page.name))?;
        let elapsed_ns = monotonic_nanos().saturating_sub(started);
        let nodes = count_nodes(&artifact);

        ready_pages += 1;
        source_bytes = source_bytes.saturating_add(artifact.source.bytes);
        node_count = node_count.saturating_add(nodes);
        style_slots = style_slots.saturating_add(artifact.style_index.style_slot_count);
        asset_requests = asset_requests.saturating_add(artifact.asset_index.request_count);
        parse_ns = parse_ns.saturating_add(elapsed_ns);

        report_info(format_args!(
            "solara: handoff-ready page={} ready=1 bytes={} nodes={} elements={} style_slots={} rules={} css_errors={} assets={} scripts={} parse_us={} parse5_ms={} css_ms={} artifact_ms={}",
            page.name,
            artifact.source.bytes,
            nodes,
            artifact.style_index.element_count,
            artifact.style_index.style_slot_count,
            artifact.style_index.rule_count,
            artifact.style_index.load_errors.len(),
            artifact.asset_index.request_count,
            artifact.extracted.script_count,
            nanos_to_micros(elapsed_ns),
            artifact.timings.parse5_ms,
            artifact.timings.lightning_css_ms,
            artifact.timings.total_ms,
        ));
    }

    let total_ns = monotonic_nanos().saturating_sub(suite_started);
    report_info(format_args!(
        "solara: handoff-ready summary ready={}/{} bytes={} nodes={} style_slots={} assets={} engine_us={} parse_us={} total_us={} ui4_frame=0 wgpu=0",
        ready_pages,
        PAGES.len(),
        source_bytes,
        node_count,
        style_slots,
        asset_requests,
        nanos_to_micros(engine_ns),
        nanos_to_micros(parse_ns),
        nanos_to_micros(total_ns),
    ));
    Ok(())
}

fn load_embedded_stylesheet(
    _document_url: &str,
    _base_href: Option<&str>,
    href: &str,
) -> Result<LoadedStylesheet, String> {
    let leaf = href.rsplit('/').next().unwrap_or(href);
    match leaf {
        "demoui.css" => Ok(LoadedStylesheet::new(
            "trueos://solara/docs/demoui.css",
            DEMO_UI_CSS,
        )),
        "TextAndBorders.css" => Ok(LoadedStylesheet::new(
            "trueos://solara/docs/TextAndBorders.css",
            TEXT_AND_BORDERS_CSS,
        )),
        _ => Err(format!(
            "stylesheet {href:?} is outside the embedded five-page corpus"
        )),
    }
}

fn count_nodes(artifact: &DomArtifact) -> usize {
    let mut count = 0usize;
    artifact
        .document
        .walk(&mut |_| count = count.saturating_add(1));
    count
}

const fn nanos_to_micros(nanos: u64) -> u64 {
    nanos / 1_000
}

#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
fn monotonic_nanos() -> u64 {
    trueos::clock::monotonic_nanos()
}

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
fn monotonic_nanos() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;

    static START: OnceLock<Instant> = OnceLock::new();
    let nanos = START.get_or_init(Instant::now).elapsed().as_nanos();
    u64::try_from(nanos).unwrap_or(u64::MAX)
}

#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
fn report_info(arguments: fmt::Arguments<'_>) {
    trueos::logl::log(trueos::logl::level::INFO, arguments);
}

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
fn report_info(arguments: fmt::Arguments<'_>) {
    println!("{arguments}");
}

#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
pub(crate) fn report_error(arguments: fmt::Arguments<'_>) {
    trueos::logl::log(trueos::logl::level::ERROR, arguments);
}

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
pub(crate) fn report_error(arguments: fmt::Arguments<'_>) {
    eprintln!("{arguments}");
}

#[cfg(test)]
mod tests {
    use super::{PAGES, count_nodes, load_embedded_stylesheet};
    use rust_qjs_dom::DomEngine;

    #[test]
    fn all_five_embedded_pages_reach_the_handoff_contract() {
        let mut engine =
            DomEngine::with_stylesheet_loader(load_embedded_stylesheet).expect("DOM engine starts");
        assert_eq!(PAGES.len(), 5);
        for page in PAGES {
            let artifact = engine.parse(page.html, page.url).expect(page.name);
            assert_eq!(artifact.source.url, page.url);
            assert!(count_nodes(&artifact) > 1, "{} has a DOM", page.name);
            assert!(
                artifact.style_index.element_count > 0,
                "{} has styled elements",
                page.name
            );
            assert!(
                artifact.style_index.load_errors.is_empty(),
                "{} has no stylesheet load errors: {:?}",
                page.name,
                artifact.style_index.load_errors
            );
        }
    }
}
