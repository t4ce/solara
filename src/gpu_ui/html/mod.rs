mod layout;
mod node;
mod paint;
mod parser;
mod style;

pub use node::HtmlNode;

use layout::{document_height, hit_test_details_summary, layout_document};
use paint::paint_document;
use parser::parse_html;
#[cfg(feature = "sandboxed-scene-js")]
use rust_qjs_dom::JsEngineOptions;
use rust_qjs_dom::{DomArtifact, DomEngine, JsEngine};
#[cfg(feature = "sandboxed-scene-js")]
use serde_json::Value;
#[cfg(feature = "sandboxed-scene-js")]
use std::{cell::RefCell, rc::Rc, time::Duration};

use crate::gpu_ui::input::{self, MouseDispatch, MouseInput};

#[derive(Default)]
pub struct RenderBatch {
    pub shapes: Vec<crate::gpu_ui::shapes::ShapeInstance>,
    pub text: crate::gpu_ui::text::TextBatch,
}

impl RenderBatch {
    pub fn clear(&mut self) {
        self.shapes.clear();
        self.text.clear();
    }
}

pub struct Document {
    pub nodes: Vec<HtmlNode>,
    pub scroll_y: f32,
    pub content_height: f32,
    #[cfg(any(
        test,
        target_os = "trueos",
        target_os = "zkvm",
        feature = "headless-picasso"
    ))]
    scrollbar_side: ScrollbarSide,
    #[cfg(any(
        test,
        target_os = "trueos",
        target_os = "zkvm",
        feature = "headless-picasso"
    ))]
    resize_handle: bool,
    dom: DomArtifact,
    dom_engine: DomEngine,
    page_width: f32,
    #[cfg(feature = "sandboxed-scene-js")]
    scene_patch_sink: Rc<RefCell<ScenePatchSink>>,
    #[cfg(feature = "sandboxed-scene-js")]
    scene_script_engine: JsEngine,
    #[cfg(feature = "sandboxed-scene-js")]
    scene_patch_host_installed: bool,
}

/// Load-time page policy for Solara's Rust-owned viewport scrollbar.
///
/// The document selects a side; browser chrome owns its geometry and paint.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
pub(crate) enum ScrollbarSide {
    Left,
    #[default]
    Right,
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
impl ScrollbarSide {
    #[cfg(any(target_os = "trueos", target_os = "zkvm"))]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

pub(crate) struct SelectInteraction {
    pub(crate) selected: Option<usize>,
}

#[derive(Clone, Debug)]
#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
#[cfg_attr(
    all(
        feature = "headless-picasso",
        not(any(test, target_os = "trueos", target_os = "zkvm"))
    ),
    allow(dead_code)
)]
pub(crate) struct ImageRequest {
    pub(crate) node_id: u32,
    pub(crate) source_url: String,
    pub(crate) rect: crate::gpu_ui::geometry::Rect,
}

/// Explicit limits for the opt-in first JavaScript execution step.
///
/// This is deliberately not a browser DOM API.  The only host capability is
/// `__solara.scenePatch(...)`, whose accepted messages are validated before
/// Solara mutates its render projection and publishes a new scene.
#[cfg(feature = "sandboxed-scene-js")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SceneScriptBudget {
    pub(crate) max_scripts: usize,
    pub(crate) max_source_bytes: usize,
    pub(crate) max_patches: usize,
    pub(crate) max_text_bytes: usize,
    pub(crate) timeout: Duration,
}

#[cfg(feature = "sandboxed-scene-js")]
const SCENE_SCRIPT_MEMORY_LIMIT_BYTES: usize = 8 * 1024 * 1024;
#[cfg(feature = "sandboxed-scene-js")]
const SCENE_SCRIPT_STACK_LIMIT_BYTES: usize = 512 * 1024;

#[cfg(feature = "sandboxed-scene-js")]
impl Default for SceneScriptBudget {
    fn default() -> Self {
        Self {
            max_scripts: 4,
            max_source_bytes: 16 * 1024,
            max_patches: 32,
            max_text_bytes: 4 * 1024,
            timeout: Duration::from_millis(16),
        }
    }
}

/// Result of one explicit sandboxed-script feature step.
#[cfg(feature = "sandboxed-scene-js")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SceneScriptReport {
    pub(crate) scripts: usize,
    pub(crate) patches: usize,
}

#[cfg(feature = "sandboxed-scene-js")]
#[derive(Clone, Debug, Eq, PartialEq)]
enum ScenePatch {
    SetPrimaryHeading { text: String },
    SetFirstPlainText { text: String },
}

#[cfg(feature = "sandboxed-scene-js")]
impl ScenePatch {
    fn decode(arguments: &[Value], max_text_bytes: usize) -> Result<Self, String> {
        let [message] = arguments else {
            return Err(String::from(
                "__solara.scenePatch expects exactly one JSON object",
            ));
        };
        let object = message
            .as_object()
            .ok_or_else(|| String::from("scene patch must be an object"))?;
        if object.len() != 2 || !object.contains_key("op") || !object.contains_key("text") {
            return Err(String::from(
                "scene patch must contain only string op and text fields",
            ));
        }
        let text = object
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| String::from("scene patch text must be a string"))?;
        if text.len() > max_text_bytes || text.contains('\0') {
            return Err(String::from("scene patch text exceeds its bounded policy"));
        }
        match object.get("op").and_then(Value::as_str) {
            Some("set-primary-heading") => Ok(Self::SetPrimaryHeading {
                text: String::from(text),
            }),
            Some("set-first-plain-text") => Ok(Self::SetFirstPlainText {
                text: String::from(text),
            }),
            _ => Err(String::from("scene patch operation is not enabled")),
        }
    }
}

#[cfg(feature = "sandboxed-scene-js")]
#[derive(Default)]
struct ScenePatchSink {
    active: bool,
    max_patches: usize,
    max_text_bytes: usize,
    patches: Vec<ScenePatch>,
}

impl Document {
    pub fn from_dom(
        dom: DomArtifact,
        mut dom_engine: DomEngine,
        page_width: f32,
    ) -> Result<Self, String> {
        #[cfg(any(
            test,
            target_os = "trueos",
            target_os = "zkvm",
            feature = "headless-picasso"
        ))]
        let scrollbar_side = scrollbar_side_from_dom(&dom);
        #[cfg(any(
            test,
            target_os = "trueos",
            target_os = "zkvm",
            feature = "headless-picasso"
        ))]
        let resize_handle = resize_handle_from_dom(&dom);
        let mut nodes = parse_html(&dom, &mut dom_engine)?;
        layout_document(&mut nodes, page_width, &dom.style_index);
        let content_height = document_height(&nodes);
        #[cfg(feature = "sandboxed-scene-js")]
        let scene_patch_sink = Rc::new(RefCell::new(ScenePatchSink::default()));
        #[cfg(feature = "sandboxed-scene-js")]
        let scene_script_engine = JsEngine::with_options(JsEngineOptions {
            memory_limit_bytes: SCENE_SCRIPT_MEMORY_LIMIT_BYTES,
            stack_limit_bytes: SCENE_SCRIPT_STACK_LIMIT_BYTES,
        })
        .map_err(|error| format!("failed to start Solara's isolated script runtime: {error}"))?;
        let mut document = Self {
            nodes,
            scroll_y: 0.0,
            content_height,
            #[cfg(any(
                test,
                target_os = "trueos",
                target_os = "zkvm",
                feature = "headless-picasso"
            ))]
            scrollbar_side,
            #[cfg(any(
                test,
                target_os = "trueos",
                target_os = "zkvm",
                feature = "headless-picasso"
            ))]
            resize_handle,
            dom,
            dom_engine,
            page_width,
            #[cfg(feature = "sandboxed-scene-js")]
            scene_patch_sink,
            #[cfg(feature = "sandboxed-scene-js")]
            scene_script_engine,
            #[cfg(feature = "sandboxed-scene-js")]
            scene_patch_host_installed: false,
        };
        let bootstrap = format!(
            "globalThis.__solara = Object.freeze({{ domArtifactVersion: {} }});",
            document.dom().schema_version
        );
        document
            .js_mut()
            .eval_void(&bootstrap, "<solara-bootstrap>")
            .map_err(|error| format!("failed to initialize Solara's JavaScript host: {error}"))?;
        #[cfg(feature = "sandboxed-scene-js")]
        {
            document.install_scene_patch_host()?;
            let script_bootstrap = format!(
                "globalThis.__solara = Object.freeze({{ domArtifactVersion: {}, scenePatch: globalThis.__solaraScenePatch }});",
                document.dom().schema_version
            );
            document
                .scene_script_engine
                .eval_void(&script_bootstrap, "<solara-scene-script-bootstrap>")
                .map_err(|error| {
                    format!("failed to initialize Solara's isolated script host: {error}")
                })?;
        }
        input::install(document.js_mut())?;
        Ok(document)
    }

    pub fn dom(&self) -> &DomArtifact {
        &self.dom
    }

    #[cfg(any(
        test,
        target_os = "trueos",
        target_os = "zkvm",
        feature = "headless-picasso"
    ))]
    pub(crate) const fn scrollbar_side(&self) -> ScrollbarSide {
        self.scrollbar_side
    }

    #[cfg(any(
        test,
        target_os = "trueos",
        target_os = "zkvm",
        feature = "headless-picasso"
    ))]
    pub(crate) const fn resize_handle_enabled(&self) -> bool {
        self.resize_handle
    }

    #[cfg(any(
        test,
        target_os = "trueos",
        target_os = "zkvm",
        feature = "headless-picasso"
    ))]
    pub(crate) fn image_requests(&self) -> Vec<ImageRequest> {
        let mut requests = Vec::new();
        collect_image_requests(&self.nodes, &mut requests);
        let Ok(document_url) = url::Url::parse(self.dom.source.url.as_str()) else {
            return requests;
        };
        let base = self
            .dom
            .asset_index
            .base_href
            .as_deref()
            .and_then(|href| document_url.join(href).ok())
            .unwrap_or(document_url);
        for request in &mut requests {
            if let Ok(url) = base.join(request.source_url.as_str()) {
                request.source_url = url.into();
            }
        }
        requests
    }

    /// Returns the same QuickJS runtime that produced this document's Parse5 DOM.
    pub fn js_mut(&mut self) -> &mut JsEngine {
        self.dom_engine.js_mut()
    }

    #[cfg(feature = "sandboxed-scene-js")]
    fn install_scene_patch_host(&mut self) -> Result<(), String> {
        if self.scene_patch_host_installed {
            return Ok(());
        }
        let sink = Rc::clone(&self.scene_patch_sink);
        self.scene_script_engine
            .register_json_function("__solaraScenePatch", 1, move |arguments| {
                let mut state = sink
                    .try_borrow_mut()
                    .map_err(|_| String::from("Solara scene patch sink is unavailable"))?;
                if !state.active {
                    return Err(String::from(
                        "Solara scene patch capability is inactive outside an execution step",
                    ));
                }
                if state.patches.len() >= state.max_patches {
                    return Err(String::from("Solara scene patch budget is exhausted"));
                }
                let patch = ScenePatch::decode(arguments, state.max_text_bytes)?;
                state.patches.push(patch);
                Ok(Value::Null)
            })
            .map_err(|error| format!("failed to install Solara scene-patch host: {error}"))?;
        self.scene_patch_host_installed = true;
        Ok(())
    }

    /// Execute the document's explicitly opted-in, inline classic scripts.
    ///
    /// A document must place `data-solara-feature-step="sandboxed-scene-js"`
    /// on its `<html>` element. External scripts and every other browser API
    /// remain unavailable in this first step.
    #[cfg(feature = "sandboxed-scene-js")]
    pub(crate) fn execute_opt_in_inline_scene_scripts(
        &mut self,
        budget: SceneScriptBudget,
    ) -> Result<SceneScriptReport, String> {
        if !document_requests_sandboxed_scene_js(&self.dom) {
            return Ok(SceneScriptReport::default());
        }
        let scripts = self
            .dom
            .extracted
            .scripts
            .iter()
            .enumerate()
            .filter_map(|(index, metadata)| {
                let source = metadata.get("scriptText")?.as_str()?;
                if !is_inline_classic_script(metadata) {
                    return None;
                }
                Some((
                    source.to_owned(),
                    format!("{}#inline-script-{}", self.dom.source.url, index + 1),
                ))
            })
            .collect::<Vec<_>>();
        self.execute_scene_script_batch(scripts.as_slice(), budget)
    }

    /// Execute one explicitly supplied script against Solara's bounded
    /// scene-patch capability.  This is useful for deterministic tests and
    /// for future trusted host-driven feature steps; it does not implement a
    /// fake live `document` object.
    #[cfg(feature = "sandboxed-scene-js")]
    #[allow(dead_code)]
    pub(crate) fn execute_scene_script(
        &mut self,
        source: &str,
        filename: &str,
        budget: SceneScriptBudget,
    ) -> Result<SceneScriptReport, String> {
        let scripts = vec![(String::from(source), String::from(filename))];
        self.execute_scene_script_batch(scripts.as_slice(), budget)
    }

    #[cfg(feature = "sandboxed-scene-js")]
    fn execute_scene_script_batch(
        &mut self,
        scripts: &[(String, String)],
        budget: SceneScriptBudget,
    ) -> Result<SceneScriptReport, String> {
        if scripts.len() > budget.max_scripts {
            return Err(format!(
                "Solara sandbox accepts at most {} inline scripts per feature step",
                budget.max_scripts
            ));
        }
        if let Some((index, _)) = scripts
            .iter()
            .enumerate()
            .find(|(_, (source, _))| source.len() > budget.max_source_bytes)
        {
            return Err(format!(
                "Solara inline script {} exceeds its {} byte budget",
                index + 1,
                budget.max_source_bytes
            ));
        }

        self.install_scene_patch_host()?;
        {
            let mut state = self
                .scene_patch_sink
                .try_borrow_mut()
                .map_err(|_| String::from("Solara scene patch sink is already active"))?;
            if state.active {
                return Err(String::from(
                    "Solara scene patch execution is already active",
                ));
            }
            state.patches.clear();
            state.max_patches = budget.max_patches;
            state.max_text_bytes = budget.max_text_bytes;
            state.active = true;
        }

        let evaluation = scripts.iter().try_for_each(|(source, filename)| {
            self.scene_script_engine
                .eval_void_with_timeout(source, filename, budget.timeout)
                .map_err(|error| format!("sandboxed Solara script {filename} failed: {error}"))
        });
        let patches = {
            let mut state = self
                .scene_patch_sink
                .try_borrow_mut()
                .map_err(|_| String::from("Solara scene patch sink was not released"))?;
            state.active = false;
            core::mem::take(&mut state.patches)
        };
        evaluation?;
        self.apply_scene_patches(patches.as_slice())?;
        Ok(SceneScriptReport {
            scripts: scripts.len(),
            patches: patches.len(),
        })
    }

    #[cfg(feature = "sandboxed-scene-js")]
    fn apply_scene_patches(&mut self, patches: &[ScenePatch]) -> Result<(), String> {
        for patch in patches {
            let present = match patch {
                ScenePatch::SetPrimaryHeading { .. } => has_first_heading(&self.nodes),
                ScenePatch::SetFirstPlainText { .. } => has_first_plain_text(&self.nodes),
            };
            if !present {
                return Err(String::from(
                    "sandboxed script addressed a render target absent from this document",
                ));
            }
        }
        for patch in patches {
            let applied = match patch {
                ScenePatch::SetPrimaryHeading { text } => {
                    set_first_heading_text(&mut self.nodes, text)
                }
                ScenePatch::SetFirstPlainText { text } => {
                    set_first_plain_text(&mut self.nodes, text)
                }
            };
            debug_assert!(applied, "the validated scene target remains present");
        }
        if !patches.is_empty() {
            self.relayout(self.page_width);
        }
        Ok(())
    }

    pub(crate) fn dispatch_mouse(&mut self, input: MouseInput) -> Result<MouseDispatch, String> {
        input::dispatch(self.js_mut(), input)
    }

    #[cfg(any(
        test,
        target_os = "trueos",
        target_os = "zkvm",
        feature = "headless-picasso"
    ))]
    #[allow(dead_code)]
    pub(crate) fn set_visual_viewport(
        &mut self,
        x: u32,
        y: u32,
        zoom_percent: u32,
    ) -> Result<(), String> {
        input::set_viewport(self.js_mut(), x, y, zoom_percent)
    }

    pub fn relayout(&mut self, page_width: f32) {
        self.page_width = page_width;
        layout_document(&mut self.nodes, page_width, &self.dom.style_index);
        self.content_height = document_height(&self.nodes);
    }

    pub(crate) fn set_primary_heading_text(&mut self, text: &str) -> bool {
        if !set_first_heading_text(&mut self.nodes, text) {
            return false;
        }
        self.relayout(self.page_width);
        true
    }

    pub(crate) fn configure_select(
        &mut self,
        id: &str,
        options: Vec<String>,
        selected: usize,
    ) -> bool {
        if options.is_empty() || !configure_select_node(&mut self.nodes, id, &options, selected) {
            return false;
        }
        self.relayout(self.page_width);
        true
    }

    pub(crate) fn activate_select_at(&mut self, x: f32, y: f32) -> Option<SelectInteraction> {
        let interaction = activate_select(&mut self.nodes, x, y + self.scroll_y)?;
        self.relayout(self.page_width);
        Some(interaction)
    }

    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll_y = (self.scroll_y - delta).max(0.0);
    }

    pub fn clamp_scroll_to(&mut self, viewport_height: f32) {
        let max_scroll = (self.content_height - viewport_height).max(0.0);
        if self.scroll_y > max_scroll {
            self.scroll_y = max_scroll;
        }
    }

    pub fn toggle_details_at(&mut self, x: f32, y: f32) -> bool {
        if !toggle_details_recursive(&mut self.nodes, x, y + self.scroll_y) {
            return false;
        }
        layout_document(&mut self.nodes, self.page_width, &self.dom.style_index);
        self.content_height = document_height(&self.nodes);
        true
    }

    #[cfg_attr(feature = "gpu-text-only", allow(dead_code))]
    pub fn video_bounds(&self) -> Option<[f32; 4]> {
        find_video_bounds(&self.nodes).map(|bounds| {
            [
                bounds.x,
                bounds.y - self.scroll_y,
                bounds.width,
                bounds.height,
            ]
        })
    }
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
fn collect_image_requests(nodes: &[HtmlNode], out: &mut Vec<ImageRequest>) {
    for node in nodes {
        match &node.kind {
            node::ElementKind::Image { src, .. } if !src.is_empty() => out.push(ImageRequest {
                node_id: node.id,
                source_url: src.clone(),
                rect: node.bounds,
            }),
            node::ElementKind::Element { children, .. }
            | node::ElementKind::Details { children, .. }
            | node::ElementKind::Div { children }
            | node::ElementKind::Form { children }
            | node::ElementKind::Iframe { children, .. }
            | node::ElementKind::Dialog { children, .. } => collect_image_requests(children, out),
            node::ElementKind::Label { control, .. } => {
                collect_image_requests(core::slice::from_ref(control.as_ref()), out);
            }
            _ => {}
        }
    }
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
fn scrollbar_side_from_dom(dom: &DomArtifact) -> ScrollbarSide {
    let Some(html) = find_element(&dom.document, "html") else {
        return ScrollbarSide::default();
    };
    let body = find_element(html, "body");
    body.and_then(scrollbar_side_attribute)
        .or_else(|| scrollbar_side_attribute(html))
        .unwrap_or_default()
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
fn scrollbar_side_attribute(node: &rust_qjs_dom::DomNode) -> Option<ScrollbarSide> {
    node.attribute("data-solara-scrollbar")
        .and_then(parse_scrollbar_side)
        .or_else(|| {
            node.attribute("data-solara-scrollbar-side")
                .and_then(parse_scrollbar_side)
        })
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
fn parse_scrollbar_side(value: &str) -> Option<ScrollbarSide> {
    if value.trim().eq_ignore_ascii_case("left") {
        Some(ScrollbarSide::Left)
    } else if value.trim().eq_ignore_ascii_case("right") {
        Some(ScrollbarSide::Right)
    } else {
        None
    }
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
fn resize_handle_from_dom(dom: &DomArtifact) -> bool {
    let Some(html) = find_element(&dom.document, "html") else {
        return false;
    };
    let body = find_element(html, "body");
    body.and_then(resize_handle_attribute)
        .or_else(|| resize_handle_attribute(html))
        .unwrap_or(false)
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
fn resize_handle_attribute(node: &rust_qjs_dom::DomNode) -> Option<bool> {
    let value = node.attribute("data-solara-resize-handle")?.trim();
    if value.eq_ignore_ascii_case("bottom-right")
        || value.eq_ignore_ascii_case("on")
        || value.eq_ignore_ascii_case("true")
    {
        Some(true)
    } else if value.eq_ignore_ascii_case("off")
        || value.eq_ignore_ascii_case("none")
        || value.eq_ignore_ascii_case("false")
    {
        Some(false)
    } else {
        None
    }
}

#[cfg(any(
    test,
    target_os = "trueos",
    target_os = "zkvm",
    feature = "headless-picasso"
))]
fn find_element<'a>(
    node: &'a rust_qjs_dom::DomNode,
    tag: &str,
) -> Option<&'a rust_qjs_dom::DomNode> {
    if node
        .tag_name
        .as_deref()
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(tag))
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

#[cfg(feature = "sandboxed-scene-js")]
fn document_requests_sandboxed_scene_js(dom: &DomArtifact) -> bool {
    find_element(&dom.document, "html")
        .and_then(|html| html.attribute("data-solara-feature-step"))
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("sandboxed-scene-js"))
}

#[cfg(feature = "sandboxed-scene-js")]
fn is_inline_classic_script(metadata: &Value) -> bool {
    if metadata
        .get("src")
        .and_then(Value::as_str)
        .is_some_and(|source| !source.is_empty())
    {
        return false;
    }
    let Some(tag) = metadata.get("tagHtml").and_then(Value::as_str) else {
        return false;
    };
    let media_type = html_attribute(tag, "type")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    matches!(
        media_type.as_str(),
        "" | "text/javascript" | "application/javascript" | "text/ecmascript"
    )
}

/// Extract one HTML attribute from an already extracted opening tag.
///
/// This is intentionally only used to enforce a deny-by-default script policy;
/// it is not a replacement for Parse5.
#[cfg(feature = "sandboxed-scene-js")]
fn html_attribute(tag: &str, name: &str) -> Option<String> {
    let lowercase = tag.to_ascii_lowercase();
    let mut cursor = 0usize;
    while let Some(relative) = lowercase[cursor..].find(name) {
        let start = cursor + relative;
        let before_ok = start == 0
            || lowercase.as_bytes()[start - 1].is_ascii_whitespace()
            || lowercase.as_bytes()[start - 1] == b'<';
        let after = start + name.len();
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
        let quote = *tag.as_bytes().get(value_start)?;
        if matches!(quote, b'\'' | b'"') {
            value_start += 1;
            let length = tag.as_bytes()[value_start..]
                .iter()
                .position(|byte| *byte == quote)?;
            return Some(tag[value_start..value_start + length].to_owned());
        }
        let length = tag.as_bytes()[value_start..]
            .iter()
            .position(|byte| byte.is_ascii_whitespace() || *byte == b'>')
            .unwrap_or(tag.len() - value_start);
        return Some(tag[value_start..value_start + length].to_owned());
    }
    None
}

#[cfg_attr(feature = "gpu-text-only", allow(dead_code))]
fn find_video_bounds(nodes: &[HtmlNode]) -> Option<crate::gpu_ui::geometry::Rect> {
    for node in nodes {
        if matches!(node.kind, node::ElementKind::Video { .. }) {
            return Some(node.bounds);
        }
        let found = match &node.kind {
            node::ElementKind::Element { children, .. }
            | node::ElementKind::Details { children, .. }
            | node::ElementKind::Div { children }
            | node::ElementKind::Form { children }
            | node::ElementKind::Iframe { children, .. }
            | node::ElementKind::Dialog { children, .. } => find_video_bounds(children),
            node::ElementKind::Label { control, .. } => {
                find_video_bounds(std::slice::from_ref(control))
            }
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

fn set_first_heading_text(nodes: &mut [HtmlNode], text: &str) -> bool {
    for node in nodes {
        let updated = match &mut node.kind {
            node::ElementKind::Heading {
                text: heading_text, ..
            } => {
                text.clone_into(heading_text);
                true
            }
            node::ElementKind::Element { tag, children }
                if matches!(tag.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") =>
            {
                set_first_plain_text(children, text)
            }
            node::ElementKind::Element { children, .. }
            | node::ElementKind::Details { children, .. }
            | node::ElementKind::Div { children }
            | node::ElementKind::Form { children }
            | node::ElementKind::Iframe { children, .. }
            | node::ElementKind::Dialog { children, .. } => set_first_heading_text(children, text),
            node::ElementKind::Label { control, .. } => {
                set_first_heading_text(std::slice::from_mut(control), text)
            }
            _ => false,
        };
        if updated {
            return true;
        }
    }
    false
}

#[cfg(feature = "sandboxed-scene-js")]
fn has_first_heading(nodes: &[HtmlNode]) -> bool {
    nodes.iter().any(|node| match &node.kind {
        node::ElementKind::Heading { .. } => true,
        node::ElementKind::Element { tag, children }
            if matches!(tag.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") =>
        {
            has_first_plain_text(children)
        }
        node::ElementKind::Element { children, .. }
        | node::ElementKind::Details { children, .. }
        | node::ElementKind::Div { children }
        | node::ElementKind::Form { children }
        | node::ElementKind::Iframe { children, .. }
        | node::ElementKind::Dialog { children, .. } => has_first_heading(children),
        node::ElementKind::Label { control, .. } => {
            has_first_heading(core::slice::from_ref(control.as_ref()))
        }
        _ => false,
    })
}

fn set_first_plain_text(nodes: &mut [HtmlNode], text: &str) -> bool {
    for node in nodes {
        let updated = match &mut node.kind {
            node::ElementKind::PlainText { text: current } => {
                text.clone_into(current);
                true
            }
            node::ElementKind::Element { children, .. }
            | node::ElementKind::Details { children, .. }
            | node::ElementKind::Div { children }
            | node::ElementKind::Form { children }
            | node::ElementKind::Iframe { children, .. }
            | node::ElementKind::Dialog { children, .. } => set_first_plain_text(children, text),
            node::ElementKind::Label { control, .. } => {
                set_first_plain_text(std::slice::from_mut(control), text)
            }
            _ => false,
        };
        if updated {
            return true;
        }
    }
    false
}

#[cfg(feature = "sandboxed-scene-js")]
fn has_first_plain_text(nodes: &[HtmlNode]) -> bool {
    nodes.iter().any(|node| match &node.kind {
        node::ElementKind::PlainText { .. } => true,
        node::ElementKind::Element { children, .. }
        | node::ElementKind::Details { children, .. }
        | node::ElementKind::Div { children }
        | node::ElementKind::Form { children }
        | node::ElementKind::Iframe { children, .. }
        | node::ElementKind::Dialog { children, .. } => has_first_plain_text(children),
        node::ElementKind::Label { control, .. } => {
            has_first_plain_text(core::slice::from_ref(control.as_ref()))
        }
        _ => false,
    })
}

fn configure_select_node(
    nodes: &mut [HtmlNode],
    id: &str,
    options: &[String],
    selected: usize,
) -> bool {
    for node in nodes {
        if node.id_attr.as_deref() == Some(id)
            && let node::ElementKind::Select {
                options: current,
                selected: current_selected,
            } = &mut node.kind
        {
            *current = options.to_vec();
            *current_selected = selected.min(current.len() - 1);
            node.open = false;
            return true;
        }
        let configured = match &mut node.kind {
            node::ElementKind::Element { children, .. }
            | node::ElementKind::Details { children, .. }
            | node::ElementKind::Div { children }
            | node::ElementKind::Form { children }
            | node::ElementKind::Iframe { children, .. }
            | node::ElementKind::Dialog { children, .. } => {
                configure_select_node(children, id, options, selected)
            }
            node::ElementKind::Label { control, .. } => {
                configure_select_node(std::slice::from_mut(control), id, options, selected)
            }
            _ => false,
        };
        if configured {
            return true;
        }
    }
    false
}

fn activate_select(nodes: &mut [HtmlNode], x: f32, y: f32) -> Option<SelectInteraction> {
    for node in nodes {
        if let node::ElementKind::Select { options, selected } = &mut node.kind {
            if node.bounds.contains(x, y) {
                if !node.open {
                    node.open = true;
                    return Some(SelectInteraction { selected: None });
                }
                let row = ((y - node.bounds.y) / crate::gpu_ui::geometry::CONTROL_H)
                    .floor()
                    .max(0.0) as usize;
                node.open = false;
                if let Some(option_index) =
                    row.checked_sub(1).filter(|index| *index < options.len())
                {
                    let changed = (*selected != option_index).then_some(option_index);
                    *selected = option_index;
                    return Some(SelectInteraction { selected: changed });
                }
                return Some(SelectInteraction { selected: None });
            }
            if node.open {
                node.open = false;
                return Some(SelectInteraction { selected: None });
            }
        }
        let interaction = match &mut node.kind {
            node::ElementKind::Element { children, .. }
            | node::ElementKind::Details { children, .. }
            | node::ElementKind::Div { children }
            | node::ElementKind::Form { children }
            | node::ElementKind::Iframe { children, .. }
            | node::ElementKind::Dialog { children, .. } => activate_select(children, x, y),
            node::ElementKind::Label { control, .. } => {
                activate_select(std::slice::from_mut(control), x, y)
            }
            _ => None,
        };
        if interaction.is_some() {
            return interaction;
        }
    }
    None
}

fn toggle_details_recursive(nodes: &mut [HtmlNode], x: f32, y: f32) -> bool {
    for node in nodes.iter_mut() {
        if hit_test_details_summary(node, x, y) {
            node.open = !node.open;
            return true;
        }
        if toggle_details_in_children(node, x, y) {
            return true;
        }
    }
    false
}

fn toggle_details_in_children(node: &mut HtmlNode, x: f32, y: f32) -> bool {
    match &mut node.kind {
        node::ElementKind::Details { children, .. } if node.open => {
            toggle_details_recursive(children, x, y)
        }
        node::ElementKind::Element { children, .. }
        | node::ElementKind::Div { children }
        | node::ElementKind::Form { children }
        | node::ElementKind::Iframe { children, .. }
        | node::ElementKind::Dialog { children, .. } => toggle_details_recursive(children, x, y),
        node::ElementKind::Label { control, .. } => {
            toggle_details_recursive(std::slice::from_mut(control), x, y)
        }
        _ => false,
    }
}

pub fn collect_batch(document: &Document, scale: f32, batch: &mut RenderBatch) {
    batch.clear();
    paint_document(
        &document.nodes,
        document.scroll_y,
        &document.dom.style_index,
        &mut batch.shapes,
        &mut batch.text,
    );
    crate::gpu_ui::shapes::scale_shape_instances(&mut batch.shapes, scale);
    crate::gpu_ui::text::scale_text_batch(&mut batch.text, scale);
}

#[cfg(test)]
mod parity_baseline {
    use rust_qjs_dom::DomEngine;

    use super::{Document, HtmlNode, RenderBatch, ScrollbarSide, collect_batch};
    use crate::gpu_ui::html::node::ElementKind;

    fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x100000001b3);
        }
    }

    fn find_by_html_id<'a>(nodes: &'a [HtmlNode], id: &str) -> Option<&'a HtmlNode> {
        for node in nodes {
            if node.id_attr.as_deref() == Some(id) {
                return Some(node);
            }
            let found = match &node.kind {
                ElementKind::Element { children, .. }
                | ElementKind::Details { children, .. }
                | ElementKind::Div { children }
                | ElementKind::Form { children }
                | ElementKind::Iframe { children, .. }
                | ElementKind::Dialog { children, .. } => find_by_html_id(children, id),
                ElementKind::Label { control, .. } => {
                    find_by_html_id(std::slice::from_ref(control), id)
                }
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }

    #[test]
    fn current_demo_keeps_its_nested_frame_render_digest() {
        let page = crate::gpu_ui::loader::load_page(None).expect("load current demo");
        assert_eq!(page.title, "HTML Only Visual Elements");
        let mut document = Document::from_dom(page.artifact, page.dom_engine, 960.0)
            .expect("adapt current demo DOM");
        assert_eq!(document.dom().schema_version, 2);
        assert_eq!(
            document
                .js_mut()
                .eval_json("21 * 2", "<solara-parity>")
                .expect("retained QuickJS runtime evaluates JavaScript")
                .as_i64(),
            Some(42)
        );
        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        let mut hash = 0xcbf29ce484222325_u64;
        for shape in &batch.shapes {
            for value in shape.pos_size.into_iter().chain(shape.color) {
                hash_bytes(&mut hash, &value.to_bits().to_le_bytes());
            }
            hash_bytes(&mut hash, &shape.shape_type.to_le_bytes());
        }
        for section in &batch.text.sections {
            for value in [
                section.x,
                section.y,
                section.width,
                section.height,
                section.font_size,
            ] {
                hash_bytes(&mut hash, &value.to_bits().to_le_bytes());
            }
            for value in section.color {
                hash_bytes(&mut hash, &value.to_bits().to_le_bytes());
            }
            hash_bytes(&mut hash, section.text.as_bytes());
        }
        assert_eq!(batch.shapes.len(), 147);
        assert_eq!(batch.text.sections.len(), 79);
        // Replaced images now occupy their authored CSS extent rather than a
        // placeholder-only eight-pixel block margin.
        assert_eq!(document.content_height.to_bits(), 0x45582000);
        assert_eq!(hash, 0x224f53c5f36223bb);
    }

    #[test]
    fn authored_lightning_css_reaches_the_active_paint_batch() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                "<style>main { color: #123456 }</style><main>Styled by Lightning CSS</main>",
                "https://solara.test/",
            )
            .expect("document parses");
        let document = Document::from_dom(artifact, engine, 960.0).expect("document adapts");
        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        let styled = batch
            .text
            .sections
            .iter()
            .find(|section| section.text.contains("Styled by Lightning CSS"))
            .expect("styled text section");
        assert_eq!(
            styled.color,
            [
                0x12 as f32 / 255.0,
                0x34 as f32 / 255.0,
                0x56 as f32 / 255.0,
                1.0
            ]
        );
    }

    #[test]
    fn dom_selects_the_rust_owned_scrollbar_side() {
        let parse = |source: &str| {
            let mut engine = DomEngine::new().expect("engine starts");
            let artifact = engine
                .parse(source, "https://solara.test/scrollbar")
                .expect("document parses");
            Document::from_dom(artifact, engine, 320.0)
                .expect("document adapts")
                .scrollbar_side()
        };

        assert_eq!(parse("<main>default</main>"), ScrollbarSide::Right);
        assert_eq!(
            parse("<html data-solara-scrollbar=' LEFT '><body>x</body></html>"),
            ScrollbarSide::Left
        );
        assert_eq!(
            parse(
                "<html data-solara-scrollbar='left'><body data-solara-scrollbar-side='right'>x</body></html>"
            ),
            ScrollbarSide::Right
        );
        assert_eq!(
            parse("<body data-solara-scrollbar='diagonal'>x</body>"),
            ScrollbarSide::Right
        );
        assert_eq!(
            parse(
                "<html data-solara-scrollbar='left'><body data-solara-scrollbar='diagonal'>x</body></html>"
            ),
            ScrollbarSide::Left
        );
    }

    #[test]
    fn dom_selects_the_rust_owned_bottom_right_resize_handle() {
        let parse = |source: &str| {
            let mut engine = DomEngine::new().expect("engine starts");
            let artifact = engine
                .parse(source, "https://solara.test/resize-handle")
                .expect("document parses");
            Document::from_dom(artifact, engine, 320.0)
                .expect("document adapts")
                .resize_handle_enabled()
        };

        assert!(!parse("<main>default</main>"));
        assert!(parse(
            "<body data-solara-resize-handle='bottom-right'>x</body>"
        ));
        assert!(!parse(
            "<html data-solara-resize-handle='on'><body data-solara-resize-handle='off'>x</body></html>"
        ));
    }

    #[test]
    fn dom_preserves_and_resolves_image_requests() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                "<img src='assets/icon.png' width='64' height='32' alt='icon'>",
                "https://solara.test/docs/page.html",
            )
            .expect("document parses");
        let document = Document::from_dom(artifact, engine, 320.0).expect("document adapts");
        let images = document.image_requests();
        assert_eq!(images.len(), 1);
        assert_eq!(
            images[0].source_url,
            "https://solara.test/docs/assets/icon.png"
        );
        assert_eq!((images[0].rect.width, images[0].rect.height), (64.0, 32.0));
    }

    #[test]
    fn solara_select_opens_and_reports_one_changed_option() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                "<main><select id='format'><option>480p</option></select></main>",
                "https://solara.test/select",
            )
            .expect("document parses");
        let mut document = Document::from_dom(artifact, engine, 960.0).expect("document adapts");
        assert!(document.configure_select(
            "format",
            vec!["480p MP4".to_owned(), "360p MP4".to_owned()],
            0,
        ));
        let closed = find_by_html_id(&document.nodes, "format")
            .expect("select exists")
            .bounds;
        let opened = document
            .activate_select_at(closed.x + 4.0, closed.y + 4.0)
            .expect("closed select consumes click");
        assert_eq!(opened.selected, None);

        let open = find_by_html_id(&document.nodes, "format")
            .expect("open select exists")
            .bounds;
        assert!(open.height > closed.height);
        let changed = document
            .activate_select_at(
                open.x + 4.0,
                open.y + crate::gpu_ui::geometry::CONTROL_H * 2.5,
            )
            .expect("option consumes click");
        assert_eq!(changed.selected, Some(1));
    }

    #[test]
    fn mixed_native_css_font_sizes_reach_layout_scene_and_glyph_scale() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                r#"
                    <style>
                      #small { font-size: 12px; }
                      #large { font-size: 30px; line-height: 42px; }
                      #wrapped { font-size: 20px; line-height: 36px; }
                    </style>
                    <main>
                      <p id="small">small scene text</p>
                      <p id="large">large scene text</p>
                      <p id="wrapped">abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789</p>
                      <h2 id="native-heading">native heading text</h2>
                    </main>
                "#,
                "https://solara.test/typography",
            )
            .expect("document parses");
        let document = Document::from_dom(artifact, engine, 320.0).expect("document adapts");
        let small_node = find_by_html_id(&document.nodes, "small").expect("small layout node");
        let large_node = find_by_html_id(&document.nodes, "large").expect("large layout node");
        let wrapped_node =
            find_by_html_id(&document.nodes, "wrapped").expect("wrapped layout node");
        assert!(large_node.bounds.height > small_node.bounds.height);
        assert!(wrapped_node.bounds.height >= 72.0);

        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        let section = |needle: &str| {
            batch
                .text
                .sections
                .iter()
                .find(|section| section.text == needle)
                .unwrap_or_else(|| panic!("text section for {needle}"))
        };
        let small = section("small scene text");
        let large = section("large scene text");
        let heading = section("native heading text");

        assert_eq!(small.font_size, 12.0);
        assert_eq!(large.font_size, 30.0);
        assert_eq!(heading.font_size, 22.0);
        assert!(large.height >= 42.0);
        assert_eq!(
            solara_wgpu_shim::TextRun::scale(large),
            solara_wgpu_shim::font_metrics(30.0).glyph_scale
        );
        assert!(solara_wgpu_shim::TextRun::scale(large) > solara_wgpu_shim::TextRun::scale(small));

        let wrapped = batch
            .text
            .sections
            .iter()
            .filter(|section| section.font_size == 20.0)
            .collect::<Vec<_>>();
        assert!(wrapped.len() >= 2);
        assert!((wrapped[1].y - wrapped[0].y - 36.0).abs() < 0.001);
    }

    #[cfg(feature = "sandboxed-scene-js")]
    #[test]
    fn opt_in_inline_script_recompiles_the_headless_scene_projection() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                r#"
                    <html data-solara-feature-step="sandboxed-scene-js">
                      <style>h1 { color: #123456; }</style>
                      <body>
                        <h1>before script</h1>
                        <p>first paragraph</p>
                        <script>
                          __solara.scenePatch({
                            op: "set-primary-heading",
                            text: "after script"
                          });
                        </script>
                      </body>
                    </html>
                "#,
                "https://solara.test/script-step",
            )
            .expect("document parses");
        let mut document = Document::from_dom(artifact, engine, 640.0).expect("document adapts");

        let report = document
            .execute_opt_in_inline_scene_scripts(super::SceneScriptBudget::default())
            .expect("scene script succeeds");
        assert_eq!((report.scripts, report.patches), (1, 1));

        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        let heading = batch
            .text
            .sections
            .iter()
            .find(|section| section.text == "after script")
            .expect("patched heading reaches the paint batch");
        assert_eq!(
            heading.color,
            [
                0x12 as f32 / 255.0,
                0x34 as f32 / 255.0,
                0x56 as f32 / 255.0,
                1.0,
            ]
        );
    }

    #[cfg(feature = "sandboxed-scene-js")]
    #[test]
    fn failed_script_batch_keeps_the_previous_coherent_projection() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                r#"
                    <html data-solara-feature-step="sandboxed-scene-js">
                      <body>
                        <h1>before script</h1>
                        <script>
                          __solara.scenePatch({
                            op: "set-primary-heading",
                            text: "must not publish"
                          });
                        </script>
                        <script>throw new Error("stop the feature step")</script>
                      </body>
                    </html>
                "#,
                "https://solara.test/script-rollback",
            )
            .expect("document parses");
        let mut document = Document::from_dom(artifact, engine, 640.0).expect("document adapts");

        let error = document
            .execute_opt_in_inline_scene_scripts(super::SceneScriptBudget::default())
            .expect_err("later script aborts the batch");
        assert!(error.contains("stop the feature step"));

        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        assert!(
            batch
                .text
                .sections
                .iter()
                .any(|section| section.text == "before script")
        );
        assert!(
            !batch
                .text
                .sections
                .iter()
                .any(|section| section.text == "must not publish")
        );
    }

    #[cfg(feature = "sandboxed-scene-js")]
    #[test]
    fn unmarked_document_keeps_inline_scripts_inert() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                "<h1>static</h1><script>globalThis.pageScriptRan = true</script>",
                "https://solara.test/script-inert",
            )
            .expect("document parses");
        let mut document = Document::from_dom(artifact, engine, 640.0).expect("document adapts");

        let report = document
            .execute_opt_in_inline_scene_scripts(super::SceneScriptBudget::default())
            .expect("unmarked document needs no script run");
        assert_eq!(report, super::SceneScriptReport::default());
        assert_eq!(
            document
                .js_mut()
                .eval_json("globalThis.pageScriptRan === true", "<script-proof>")
                .expect("proof evaluates"),
            serde_json::json!(false)
        );
        assert_eq!(
            document
                .js_mut()
                .eval_json("typeof globalThis.__solara.scenePatch", "<isolation-proof>")
                .expect("parser-runtime proof evaluates"),
            serde_json::json!("undefined")
        );
    }

    #[cfg(feature = "sandboxed-scene-js")]
    #[test]
    fn only_inline_classic_scripts_are_eligible_for_the_feature_step() {
        let mut engine = DomEngine::new().expect("engine starts");
        let artifact = engine
            .parse(
                r#"
                    <html data-solara-feature-step="sandboxed-scene-js">
                      <body>
                        <h1>static heading</h1>
                        <script type="module">
                          __solara.scenePatch({ op: "set-primary-heading", text: "module" });
                        </script>
                        <script type="application/ld+json">
                          { "op": "set-primary-heading", "text": "data" }
                        </script>
                        <script src="/external.js">
                          __solara.scenePatch({ op: "set-primary-heading", text: "external" });
                        </script>
                        <script>
                          __solara.scenePatch({ op: "set-primary-heading", text: "classic" });
                        </script>
                      </body>
                    </html>
                "#,
                "https://solara.test/script-types",
            )
            .expect("document parses");
        let mut document = Document::from_dom(artifact, engine, 640.0).expect("document adapts");

        let report = document
            .execute_opt_in_inline_scene_scripts(super::SceneScriptBudget::default())
            .expect("classic script succeeds");
        assert_eq!((report.scripts, report.patches), (1, 1));

        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        assert!(
            batch
                .text
                .sections
                .iter()
                .any(|section| section.text == "classic")
        );
        for rejected in ["static heading", "module", "data", "external"] {
            assert!(
                !batch
                    .text
                    .sections
                    .iter()
                    .any(|section| section.text == rejected),
                "rejected script type unexpectedly changed the projection: {rejected}"
            );
        }
    }
}
