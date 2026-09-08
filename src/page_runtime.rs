//! Isolated classic-script realm and a bounded DOM mutation journal.
//! No guest filesystem, VM bindings, parser globals or native module imports.
use crate::spec_layout::{NodeId, SpecLayout};
use blitz_dom::QualName;
use rust_qjs_dom::{DomArtifact, DomEngine, DomNode, JsEngine};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    time::Duration,
};
use url::Url;

const SLICE: Duration = Duration::from_secs(2);
const MAX_BYTES: usize = 4 * 1024 * 1024;
#[derive(Clone, Debug)]
pub enum Script {
    Inline(String),
    External(String),
}
#[derive(Clone, Debug)]
pub struct FetchRequest {
    pub id: u64,
    pub url: String,
}

pub struct PageRuntime {
    js: JsEngine,
    journal: Rc<RefCell<Vec<Value>>>,
    requests: Rc<RefCell<VecDeque<FetchRequest>>>,
    journal_bytes: Rc<Cell<usize>>,
    nodes: BTreeMap<String, NodeId>,
    pub scripts: VecDeque<Script>,
    ready: bool,
}
impl PageRuntime {
    pub fn new(artifact: &DomArtifact, layout: &SpecLayout) -> Result<Self, String> {
        let page = Url::parse(&artifact.source.url).map_err(|e| e.to_string())?;
        if !matches!(page.scheme(), "http" | "https") {
            return Err("Page scripts need an HTTP(S) document".into());
        }
        let base = artifact
            .asset_index
            .base_href
            .as_deref()
            .and_then(|href| page.join(href).ok())
            .unwrap_or(page.clone());
        let mut js = JsEngine::new().map_err(|e| e.to_string())?;
        // A page realm must not import the embedded parser's modules either.
        js.disable_module_loading();
        let journal = Rc::new(RefCell::new(Vec::new()));
        let commands = journal.clone();
        let journal_bytes = Rc::new(Cell::new(0usize));
        let bytes = journal_bytes.clone();
        js.register_json_function("__pageRecord", 1, move |args| {
            let mut commands = commands.borrow_mut();
            if commands.len() >= 65536 {
                return Err("DOM mutation slice limit reached".into());
            }
            let batch = args
                .first()
                .and_then(Value::as_array)
                .ok_or("Missing mutation batch")?;
            if commands.len().saturating_add(batch.len()) > 65536 {
                return Err("DOM mutation slice limit reached".into());
            }
            let length = serde_json::to_string(batch)
                .map_err(|e| e.to_string())?
                .len();
            if bytes.get().saturating_add(length) > 16 * 1024 * 1024 {
                return Err("DOM mutation byte budget reached".into());
            }
            bytes.set(bytes.get() + length);
            commands.extend(batch.iter().cloned());
            Ok(Value::Null)
        })
        .map_err(|e| e.to_string())?;
        let mut parser = DomEngine::new().map_err(|e| e.to_string())?;
        js.register_json_function("__pageFragment", 3, move |args| {
            if args
                .first()
                .and_then(Value::as_str)
                .ok_or("Missing HTML")?
                .len()
                > 256 * 1024
            {
                return Err("HTML fragment limit reached".into());
            }
            parser
                .js_mut()
                .call_global_json("__rustQjsDomFragment", args)
                .map_err(|e| e.to_string())
        })
        .map_err(|e| e.to_string())?;
        let requests = Rc::new(RefCell::new(VecDeque::new()));
        let queue = requests.clone();
        let mut total_requests = 0usize;
        js.register_json_function("__pageFetch", 3, move |args| {
            let id = args
                .first()
                .and_then(Value::as_u64)
                .ok_or("Missing fetch id")?;
            let raw = args.get(1).and_then(Value::as_str).ok_or("Missing URL")?;
            let options = args.get(2).ok_or("Missing options")?;
            if options["method"]
                .as_str()
                .is_some_and(|m| !m.eq_ignore_ascii_case("GET"))
                || !options["body"].is_null()
            {
                return Err("Only GET fetch is supported".into());
            }
            // This stage has no CORS, credentials, custom headers or writes.
            let url = base.join(raw).map_err(|e| e.to_string())?;
            if !matches!(url.scheme(), "http" | "https")
                || url.origin() != page.origin()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err("Only same-origin fetch is supported".into());
            }
            total_requests += 1;
            if total_requests > 128 || queue.borrow().len() >= 16 {
                return Err("Page fetch limit reached".into());
            }
            queue.borrow_mut().push_back(FetchRequest {
                id,
                url: url.into(),
            });
            Ok(Value::Null)
        })
        .map_err(|e| e.to_string())?;
        js.eval_void_with_timeout(
            &format!(
                "globalThis.__pageTree = {};",
                serde_json::to_string(&artifact.document).map_err(|e| e.to_string())?
            ),
            "<page-tree>",
            SLICE,
        )
        .map_err(|e| e.to_string())?;
        js.eval_void_with_timeout(include_str!("page_dom.js"), "<page-dom>", SLICE)
            .map_err(|e| e.to_string())?;
        let mut nodes = BTreeMap::new();
        fn map(
            node: &DomNode,
            path: String,
            layout: &SpecLayout,
            out: &mut BTreeMap<String, NodeId>,
        ) {
            if let Some(id) = layout.source_node(&path) {
                out.insert(path.clone(), id);
            }
            for (i, child) in node.children.iter().enumerate() {
                map(child, format!("{path}.{i}"), layout, out);
            }
        }
        map(&artifact.document, "root".into(), layout, &mut nodes);
        let mut scripts = VecDeque::new();
        fn collect(node: &DomNode, page: &Url, base: &Url, scripts: &mut VecDeque<Script>) {
            if node.tag_name.as_deref() == Some("script") {
                let attr = |name| {
                    node.attrs
                        .iter()
                        .find(|a| a.name == name)
                        .map(|a| a.value.as_str())
                        .unwrap_or("")
                };
                let mime = attr("type")
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase();
                if matches!(
                    mime.as_str(),
                    "" | "text/javascript"
                        | "application/javascript"
                        | "text/ecmascript"
                        | "application/ecmascript"
                ) && scripts.len() < 32
                {
                    if attr("src").is_empty() {
                        scripts.push_back(Script::Inline(
                            node.children
                                .iter()
                                .filter_map(|n| n.value.as_deref())
                                .collect(),
                        ));
                    } else if let Ok(url) = base.join(attr("src"))
                        && url.origin() == page.origin()
                    {
                        scripts.push_back(Script::External(url.into()));
                    }
                }
            }
            for child in &node.children {
                collect(child, page, base, scripts);
            }
        }
        let page = Url::parse(&artifact.source.url).map_err(|e| e.to_string())?;
        let base = artifact
            .asset_index
            .base_href
            .as_deref()
            .and_then(|href| page.join(href).ok())
            .unwrap_or(page.clone());
        collect(&artifact.document, &page, &base, &mut scripts);
        Ok(Self {
            js,
            journal,
            journal_bytes,
            requests,
            nodes,
            scripts,
            ready: false,
        })
    }
    pub fn execute(&mut self, source: &str, filename: &str) -> Result<(), String> {
        if source.len() > MAX_BYTES {
            return Err("Page script size limit reached".into());
        }
        self.js
            .eval_void_with_timeout(source, filename, SLICE)
            .map_err(|e| e.to_string())
    }
    pub fn take_request(&mut self) -> Option<FetchRequest> {
        self.requests.borrow_mut().pop_front()
    }
    pub fn complete(&mut self, id: u64, result: Result<String, String>) -> Result<(), String> {
        let (body, error) = match result {
            Ok(body) if body.len() <= MAX_BYTES => (body, Value::Null),
            Ok(_) => (String::new(), json!("Fetch body limit reached")),
            Err(e) => (String::new(), json!(e)),
        };
        self.execute(
            &format!("__solara.response({id},{},{});", json!(body), error),
            "<fetch-complete>",
        )
    }
    pub fn click(&mut self, node: NodeId) -> Result<(), String> {
        if let Some((key, _)) = self.nodes.iter().find(|(_, id)| **id == node) {
            self.execute(&format!("__solara.click({});", json!(key)), "<click>")?;
        }
        Ok(())
    }
    pub fn tick(&mut self, now_ms: u64, layout: &mut SpecLayout) -> Result<bool, String> {
        if !self.ready && self.scripts.is_empty() {
            self.execute("__solara.ready();", "<ready>")?;
            self.ready = true;
        }
        self.execute(&format!("__solara.tick({now_ms});"), "<timers>")?;
        self.js.pump_jobs(128, SLICE).map_err(|e| e.to_string())?;
        self.execute("__solara.flush();", "<dom-mutations>")?;
        self.apply(layout)
    }
    fn apply(&mut self, layout: &mut SpecLayout) -> Result<bool, String> {
        let commands = std::mem::take(&mut *self.journal.borrow_mut());
        self.journal_bytes.set(0);
        let changed = !commands.is_empty();
        let mut dom = layout.mutate();
        let attr_name = |name: &str| QualName::new(None, "".into(), name.into());
        for command in commands {
            let op = command[0].as_str().ok_or("Invalid mutation")?;
            let key = command[1].as_str().ok_or("Invalid node key")?;
            if op == "create" {
                if self.nodes.len() >= 32768 || self.nodes.contains_key(key) {
                    return Err("Invalid or excessive node allocation".into());
                }
                let name = command[3].as_str().ok_or("Missing node name")?;
                let text = command[4].as_str().unwrap_or("");
                let id = match command[2].as_u64() {
                    Some(1) => dom.create_element(
                        QualName::new(
                            None,
                            command[5]
                                .as_str()
                                .unwrap_or("http://www.w3.org/1999/xhtml")
                                .into(),
                            name.into(),
                        ),
                        Vec::new(),
                    ),
                    Some(3) => dom.create_text_node(text),
                    Some(8) => dom.create_comment_node(text),
                    _ => return Err("Unsupported node type".into()),
                };
                self.nodes.insert(key.into(), id);
                continue;
            }
            let id = *self.nodes.get(key).ok_or("Unknown DOM node")?;
            match op {
                "text" => dom.set_node_text(id, command[2].as_str().unwrap_or("")),
                "attr" => {
                    if !dom.doc.get_node(id).is_some_and(|n| n.is_element()) {
                        return Err("Attribute target is not an element".into());
                    }
                    let name = attr_name(command[2].as_str().ok_or("Missing attribute")?);
                    if let Some(value) = command[3].as_str() {
                        dom.set_attribute(id, name, value);
                    } else {
                        dom.clear_attribute(id, name);
                    }
                }
                "remove" => dom.remove_node(id),
                "insert" => {
                    let child = *self
                        .nodes
                        .get(command[2].as_str().ok_or("Missing child")?)
                        .ok_or("Unknown child")?;
                    // The JS mirror is untrusted, including writable Node properties.
                    if dom
                        .doc
                        .get_node(id)
                        .is_none_or(|n| !n.is_element() && n.id != dom.doc.root_node().id)
                    {
                        return Err("Invalid insertion parent".into());
                    }
                    let mut ancestor = Some(id);
                    let mut depth = 0;
                    while let Some(node) = ancestor {
                        if node == child || depth >= 256 {
                            return Err("Cyclic or excessive DOM depth".into());
                        }
                        ancestor = dom.doc.get_node(node).and_then(|n| n.parent);
                        depth += 1;
                    }
                    if child == dom.doc.root_node().id {
                        return Err("Cannot move the document".into());
                    }
                    if let Some(anchor) = command[3].as_str() {
                        let anchor = *self.nodes.get(anchor).ok_or("Unknown anchor")?;
                        if dom.doc.get_node(anchor).and_then(|n| n.parent) != Some(id) {
                            return Err("Invalid insertion anchor".into());
                        }
                        if anchor != child {
                            dom.insert_nodes_before(anchor, &[child]);
                        }
                    } else {
                        dom.append_children(id, &[child]);
                    }
                }
                _ => return Err("Unsupported DOM mutation".into()),
            }
        }
        Ok(changed)
    }
}
