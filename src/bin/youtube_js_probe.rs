use std::collections::BTreeSet;
use std::time::Duration;

use rust_qjs_dom::{DomEngine, JsEngine};
use serde_json::Value;
use url::Url;

const DEFAULT_WATCH_URL: &str = "https://www.youtube.com/watch?v=nXvnof8fTBc";
const SCRIPT_TIMEOUT: Duration = Duration::from_millis(500);
const SCOUT_SCRIPT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_SCOUT_GLOBALS: usize = 128;
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const SOLARA_INPUT_BOOTSTRAP: &str = include_str!("../gpu_ui/input_bootstrap.js");

const BROWSER_PROBE_BOOTSTRAP: &str = r#"
(function (G) {
    'use strict';
    const events = [];
    const record = (kind, name) => {
        if (events.length < 4096) events.push({ kind: String(kind), name: String(name) });
    };

    class ProbeEventTarget {
        addEventListener(type) { record('addEventListener', type); }
        removeEventListener(type) { record('removeEventListener', type); }
        dispatchEvent(event) {
            record('dispatchEvent', event && event.type ? event.type : 'unknown');
            return true;
        }
    }

    class ProbeEvent {
        constructor(type, init) {
            this.type = String(type || '');
            this.bubbles = !!(init && init.bubbles);
            this.cancelable = !!(init && init.cancelable);
            this.defaultPrevented = false;
        }
        preventDefault() { if (this.cancelable) this.defaultPrevented = true; }
        stopPropagation() {}
        stopImmediatePropagation() {}
    }

    class ProbeCustomEvent extends ProbeEvent {
        constructor(type, init) {
            super(type, init);
            this.detail = init ? init.detail : undefined;
        }
    }

    class ProbeNode extends ProbeEventTarget {
        constructor() {
            super();
            this.nodeType = 0;
            this.parentNode = null;
            this.childNodes = [];
        }
        appendChild(child) { this.childNodes.push(child); child.parentNode = this; return child; }
        removeChild(child) {
            const index = this.childNodes.indexOf(child);
            if (index >= 0) this.childNodes.splice(index, 1);
            child.parentNode = null;
            return child;
        }
        insertBefore(child) { return this.appendChild(child); }
        cloneNode() { return this; }
        contains(node) { return node === this || this.childNodes.includes(node); }
    }
    ProbeNode.ELEMENT_NODE = 1;
    ProbeNode.TEXT_NODE = 3;
    ProbeNode.CDATA_SECTION_NODE = 4;
    ProbeNode.PROCESSING_INSTRUCTION_NODE = 7;
    ProbeNode.COMMENT_NODE = 8;
    ProbeNode.DOCUMENT_NODE = 9;
    ProbeNode.DOCUMENT_FRAGMENT_NODE = 11;

    class ProbeCharacterData extends ProbeNode {
        constructor(data, nodeType, nodeName) {
            super();
            this.data = String(data || '');
            this.textContent = this.data;
            this.nodeValue = this.data;
            this.length = this.data.length;
            this.nodeType = nodeType;
            this.nodeName = nodeName;
        }
    }
    class ProbeText extends ProbeCharacterData {
        constructor(data) { super(data, ProbeNode.TEXT_NODE, '#text'); }
    }
    class ProbeComment extends ProbeCharacterData {
        constructor(data) { super(data, ProbeNode.COMMENT_NODE, '#comment'); }
    }
    class ProbeCDATASection extends ProbeText {
        constructor(data) {
            super(data);
            this.nodeType = ProbeNode.CDATA_SECTION_NODE;
            this.nodeName = '#cdata-section';
        }
    }
    class ProbeProcessingInstruction extends ProbeCharacterData {
        constructor(target, data) {
            super(data, ProbeNode.PROCESSING_INSTRUCTION_NODE, String(target || ''));
            this.target = String(target || '');
        }
    }
    class ProbeDocumentFragment extends ProbeNode {
        constructor() {
            super();
            this.nodeType = ProbeNode.DOCUMENT_FRAGMENT_NODE;
            this.nodeName = '#document-fragment';
        }
        querySelector() { return null; }
        querySelectorAll() { return []; }
    }

    class ProbeElement extends ProbeNode {
        constructor(tagName) {
            super();
            this.tagName = String(tagName || '').toUpperCase();
            this.nodeName = this.tagName;
            this.nodeType = 1;
            this.style = Object.create(null);
            this.children = [];
            this.childNodes = this.children;
            this.dataset = Object.create(null);
            this.classList = { add() {}, remove() {}, contains() { return false; } };
        }
        appendChild(child) { this.children.push(child); return child; }
        removeChild(child) { return child; }
        insertBefore(child) { this.children.push(child); return child; }
        setAttribute(name) { record('setAttribute', name); }
        getAttribute() { return null; }
        hasAttribute() { return false; }
        querySelector() { return null; }
        querySelectorAll() { return []; }
        getBoundingClientRect() {
            return { x: 0, y: 0, top: 0, right: 0, bottom: 0, left: 0, width: 0, height: 0 };
        }
        getContext(kind) {
            record('element.getContext', kind);
            if (this.tagName !== 'CANVAS' || kind !== '2d') return null;
            return {
                fillStyle: '#000000',
                fillRect() {},
                clearRect() {},
                getImageData() { return { data: new Uint8ClampedArray([0, 0, 0, 255]) }; },
            };
        }
    }

    const element = (tagName) => new ProbeElement(tagName);

    const location = {
        href: 'https://www.youtube.com/watch?v=nXvnof8fTBc',
        origin: 'https://www.youtube.com',
        protocol: 'https:',
        host: 'www.youtube.com',
        hostname: 'www.youtube.com',
        port: '',
        pathname: '/watch',
        search: '?v=nXvnof8fTBc',
        hash: '',
        assign(url) { record('location.assign', url); },
        replace(url) { record('location.replace', url); },
        reload() { record('location.reload', ''); },
        toString() { return this.href; },
    };

    const root = element('html');
    const head = element('head');
    const body = element('body');
    const document = new ProbeNode();
    Object.assign(document, {
        URL: location.href,
        documentURI: location.href,
        location,
        readyState: 'loading',
        visibilityState: 'visible',
        hidden: false,
        compatMode: 'CSS1Compat',
        characterSet: 'UTF-8',
        cookie: '',
        documentElement: root,
        head,
        body,
        currentScript: null,
        implementation: { createHTMLDocument() { return document; } },
        createElement(tag) { record('document.createElement', tag); return element(tag); },
        createElementNS(_namespace, tag) { record('document.createElementNS', tag); return element(tag); },
        createTextNode(text) { return new ProbeText(text); },
        createComment(text) { return new ProbeComment(text); },
        createCDATASection(text) { return new ProbeCDATASection(text); },
        createProcessingInstruction(target, data) { return new ProbeProcessingInstruction(target, data); },
        createDocumentFragment() { return new ProbeDocumentFragment(); },
        createEvent(kind) {
            record('document.createEvent', kind);
            return String(kind).toLowerCase().includes('custom')
                ? new G.CustomEvent('')
                : new G.Event('');
        },
        createTreeWalker() { return { currentNode: root, nextNode() { return null; } }; },
        getElementById() { return null; },
        getElementsByTagName(tag) {
            if (String(tag).toLowerCase() === 'head') return [head];
            if (String(tag).toLowerCase() === 'body') return [body];
            return [];
        },
        querySelector() { return null; },
        querySelectorAll() { return []; },
    });

    G.window = G;
    G.self = G;
    G.top = G;
    G.parent = G;
    G.location = location;
    G.document = document;
    G.EventTarget = ProbeEventTarget;
    G.Event = ProbeEvent;
    G.CustomEvent = ProbeCustomEvent;
    G.Node = ProbeNode;
    G.Document = ProbeNode;
    G.DocumentFragment = ProbeDocumentFragment;
    G.CharacterData = ProbeCharacterData;
    G.Text = ProbeText;
    G.Comment = ProbeComment;
    G.CDATASection = ProbeCDATASection;
    G.ProcessingInstruction = ProbeProcessingInstruction;
    G.NodeFilter = {
        SHOW_ALL: 0xFFFFFFFF,
        SHOW_ELEMENT: 0x1,
        SHOW_TEXT: 0x4,
        FILTER_ACCEPT: 1,
        FILTER_REJECT: 2,
        FILTER_SKIP: 3,
    };
    G.Element = ProbeElement;
    G.HTMLElement = ProbeElement;
    G.MutationObserver = class {
        constructor(callback) { this.callback = callback; }
        observe() { record('MutationObserver.observe', ''); }
        disconnect() {}
        takeRecords() { return []; }
    };
    G.IntersectionObserver = class {
        constructor(callback) { this.callback = callback; }
        observe() { record('IntersectionObserver.observe', ''); }
        unobserve() {}
        disconnect() {}
        takeRecords() { return []; }
    };
    G.ResizeObserver = class {
        constructor(callback) { this.callback = callback; }
        observe() { record('ResizeObserver.observe', ''); }
        unobserve() {}
        disconnect() {}
    };
    G.navigator = {
        userAgent: 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/126.0.0.0 Safari/537.36',
        language: 'en-US',
        languages: ['en-US', 'en'],
        platform: 'Linux x86_64',
        vendor: 'Google Inc.',
        cookieEnabled: false,
        onLine: true,
        maxTouchPoints: 0,
    };
    const navigationStartedAt = Date.now();
    G.performance = {
        now: () => Date.now() - navigationStartedAt,
        timeOrigin: navigationStartedAt,
        timing: {
            navigationStart: navigationStartedAt,
            fetchStart: navigationStartedAt,
            requestStart: navigationStartedAt,
            responseStart: navigationStartedAt,
            responseEnd: navigationStartedAt,
        },
        getEntriesByType() { return []; },
        mark(name) { record('performance.mark', name); },
        measure(name) { record('performance.measure', name); },
    };
    G.addEventListener = ProbeEventTarget.prototype.addEventListener;
    G.removeEventListener = ProbeEventTarget.prototype.removeEventListener;
    G.dispatchEvent = ProbeEventTarget.prototype.dispatchEvent;
    G.setTimeout = (callback, delay) => { record('setTimeout', delay); return 1; };
    G.clearTimeout = () => {};
    G.setInterval = (callback, delay) => { record('setInterval', delay); return 1; };
    G.clearInterval = () => {};
    G.requestAnimationFrame = () => { record('requestAnimationFrame', ''); return 1; };
    G.cancelAnimationFrame = () => {};
    G.getComputedStyle = () => ({ getPropertyValue() { return ''; } });
    G.__solaraProbeEvents = events;
})(globalThis);
"#;

const SCOUT_BOOTSTRAP: &str = r#"
(function (G) {
    'use strict';
    const observations = new Map();
    const recent = [];
    const phantomCache = new Map();
    const wrappedObjects = new WeakMap();
    const proxyObjects = new WeakSet();
    let currentScript = -1;

    const propertyName = (property) => {
        if (typeof property === 'symbol') return property.description || property.toString();
        return String(property);
    };
    const childPath = (path, property) => path + '.' + propertyName(property);
    const record = (operation, path) => {
        recent.push({ operation: String(operation), path: String(path), script: currentScript });
        if (recent.length > 64) recent.shift();
        const key = operation + '\u0000' + path;
        const prior = observations.get(key);
        if (prior) {
            prior.count += 1;
            return;
        }
        if (observations.size < 4096) {
            observations.set(key, {
                operation: String(operation),
                path: String(path),
                script: currentScript,
                count: 1,
            });
        }
    };

    const makePhantom = (path) => {
        if (phantomCache.has(path)) return phantomCache.get(path);
        const target = function SolaraScoutPhantom() {};
        const assignedValues = new Map();
        const proxy = new Proxy(target, {
            get(inner, property, receiver) {
                if (property === Symbol.toPrimitive) {
                    return (hint) => {
                        record('opaque-coerce', path);
                        return hint === 'number' ? 0 : '';
                    };
                }
                if (property === Symbol.iterator) {
                    return () => {
                        record('opaque-iterate', path);
                        return { next() { return { done: true, value: undefined }; } };
                    };
                }
                if (property === Symbol.hasInstance) {
                    return () => {
                        record('opaque-instanceof', path);
                        return false;
                    };
                }
                if (property === 'then') {
                    record('opaque-read', childPath(path, property));
                    return undefined;
                }
                if (property === 'toJSON') {
                    return () => '[Solara scout phantom: ' + path + ']';
                }
                const next = childPath(path, property);
                if (assignedValues.has(property)) {
                    record('opaque-read-assigned', next);
                    return wrap(assignedValues.get(property), next);
                }
                const descriptor = Reflect.getOwnPropertyDescriptor(inner, property);
                if (descriptor && !descriptor.configurable && !descriptor.writable) {
                    return Reflect.get(inner, property, receiver);
                }
                if (descriptor && !descriptor.configurable && 'get' in descriptor && descriptor.get === undefined) {
                    return undefined;
                }
                record('opaque-read', next);
                return makePhantom(next);
            },
            set(inner, property, value) {
                const next = childPath(path, property);
                record('opaque-write', next);
                const descriptor = Reflect.getOwnPropertyDescriptor(inner, property);
                if (descriptor && !descriptor.configurable && !descriptor.writable) {
                    const written = Reflect.set(inner, property, value);
                    if (!written) record('opaque-blocked-write', next);
                    return written;
                }
                assignedValues.set(property, value);
                return true;
            },
            apply(_inner, _thisValue, _arguments) {
                record('opaque-call', path);
                return makePhantom(path + '()');
            },
            construct() {
                record('opaque-construct', path);
                return makePhantom('new ' + path);
            },
            has(_inner, property) {
                record('opaque-has', childPath(path, property));
                return true;
            },
            ownKeys(inner) {
                record('opaque-keys', path);
                return Array.from(new Set([...Reflect.ownKeys(inner), ...assignedValues.keys()]));
            },
            getOwnPropertyDescriptor(inner, property) {
                const descriptor = Reflect.getOwnPropertyDescriptor(inner, property);
                if (descriptor) return descriptor;
                if (assignedValues.has(property)) {
                    return {
                        configurable: true,
                        enumerable: true,
                        writable: true,
                        value: assignedValues.get(property),
                    };
                }
                record('opaque-descriptor', childPath(path, property));
                return {
                    configurable: true,
                    enumerable: true,
                    writable: true,
                    value: makePhantom(childPath(path, property)),
                };
            },
        });
        phantomCache.set(path, proxy);
        proxyObjects.add(proxy);
        return proxy;
    };

    const wrap = (value, path) => {
        if ((typeof value !== 'object' && typeof value !== 'function') || value === null) {
            return value;
        }
        if (proxyObjects.has(value)) return value;
        if (wrappedObjects.has(value)) return wrappedObjects.get(value);
        const proxy = new Proxy(value, {
            get(target, property, receiver) {
                const next = childPath(path, property);
                if (!Reflect.has(target, property)) {
                    record('missing-read', next);
                    const phantom = makePhantom(next);
                    Reflect.set(target, property, phantom, receiver);
                    return phantom;
                }
                record('read', next);
                const descriptor = Reflect.getOwnPropertyDescriptor(target, property);
                if (descriptor && !descriptor.configurable && !descriptor.writable) {
                    return Reflect.get(target, property, receiver);
                }
                if (descriptor && !descriptor.configurable && 'get' in descriptor && descriptor.get === undefined) {
                    return undefined;
                }
                return wrap(Reflect.get(target, property, receiver), next);
            },
            set(target, property, nextValue, receiver) {
                const next = childPath(path, property);
                record('write', next);
                const written = Reflect.set(target, property, nextValue, receiver);
                if (!written) record('blocked-write', next);
                return written;
            },
            apply(target, thisValue, argumentsList) {
                record('call', path);
                return wrap(Reflect.apply(target, thisValue, argumentsList), path + '()');
            },
            construct(target, argumentsList, newTarget) {
                record('construct', path);
                return wrap(Reflect.construct(target, argumentsList, newTarget), 'new ' + path);
            },
            has(target, property) {
                const present = Reflect.has(target, property);
                record(present ? 'has' : 'missing-has', childPath(path, property));
                if (!present) Reflect.set(target, property, makePhantom(childPath(path, property)));
                return true;
            },
        });
        wrappedObjects.set(value, proxy);
        proxyObjects.add(proxy);
        return proxy;
    };

    const roots = [
        'document', 'location', 'navigator', 'performance',
        'EventTarget', 'Event', 'CustomEvent', 'UIEvent', 'MouseEvent', 'WheelEvent',
        'Node', 'Document',
        'Element', 'HTMLElement', 'MutationObserver',
        'IntersectionObserver', 'ResizeObserver',
    ];
    for (const name of roots) {
        if (name in G) G[name] = wrap(G[name], name);
    }
    const windowView = wrap(G, 'window');
    G.window = windowView;
    G.self = windowView;
    G.top = windowView;
    G.parent = windowView;

    G.__solaraScoutInstallGlobals = (names) => {
        for (const name of names) {
            if (!(name in G)) G[name] = makePhantom(name);
        }
    };
    G.__solaraScoutSetScript = (order) => { currentScript = Number(order); };
    G.__solaraScoutReport = () => ({
        distinct: observations.size,
        recent: recent.slice(),
        observations: Array.from(observations.values())
            .sort((left, right) => right.count - left.count || left.path.localeCompare(right.path))
            .slice(0, 96),
    });
})(globalThis);
"#;

#[derive(Debug)]
struct PageScript {
    order: usize,
    source_url: Option<Url>,
    source: String,
    media_type: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProbeMode {
    Strict,
    Scout,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("youtube-js-probe: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let (mode, watch_url) = parse_arguments()?;
    let watch_url = Url::parse(&watch_url).map_err(|error| format!("invalid URL: {error}"))?;
    if !matches!(watch_url.scheme(), "http" | "https") {
        return Err(String::from("the probe accepts only HTTP(S) watch URLs"));
    }

    println!("probe mode={mode:?} page={watch_url}");
    let html = fetch_text(&watch_url)?;
    println!("html bytes={}", html.len());

    let mut dom_engine = DomEngine::new().map_err(|error| error.to_string())?;
    let artifact = dom_engine
        .parse(&html, watch_url.as_str())
        .map_err(|error| format!("DOM parse failed: {error}"))?;
    let script_values = artifact.extracted.scripts.clone();
    println!(
        "dom schema={} version={} scripts={} script_bytes={} parse_ms={}",
        artifact.schema,
        artifact.schema_version,
        artifact.extracted.script_count,
        artifact.extracted.script_bytes,
        artifact.timings.total_ms,
    );

    let mut scripts = Vec::new();
    let mut skipped = 0usize;
    for value in script_values {
        match load_script(&watch_url, &value)? {
            Some(script) => scripts.push(script),
            None => skipped += 1,
        }
    }
    println!(
        "script_inventory classic={} skipped={skipped}",
        scripts.len()
    );

    run_strict(dom_engine.js_mut(), &watch_url, &scripts, skipped)?;
    if mode == ProbeMode::Scout {
        run_scout(&watch_url, &scripts, skipped)?;
    }
    Ok(())
}

fn parse_arguments() -> Result<(ProbeMode, String), String> {
    let mut mode = ProbeMode::Strict;
    let mut watch_url = None;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--scout" => mode = ProbeMode::Scout,
            "--strict" => mode = ProbeMode::Strict,
            "--help" | "-h" => {
                println!("usage: youtube-js-probe [--strict|--scout] [HTTP(S) watch URL]");
                std::process::exit(0);
            }
            _ if argument.starts_with('-') => {
                return Err(format!("unknown option: {argument}"));
            }
            _ if watch_url.is_some() => {
                return Err(String::from("only one watch URL may be supplied"));
            }
            _ => watch_url = Some(argument),
        }
    }
    Ok((
        mode,
        watch_url.unwrap_or_else(|| String::from(DEFAULT_WATCH_URL)),
    ))
}

fn run_strict(
    js: &mut JsEngine,
    watch_url: &Url,
    scripts: &[PageScript],
    skipped: usize,
) -> Result<(), String> {
    js.eval_void(BROWSER_PROBE_BOOTSTRAP, "<solara-youtube-probe-bootstrap>")
        .map_err(|error| format!("browser probe bootstrap failed: {error}"))?;
    js.eval_void(SOLARA_INPUT_BOOTSTRAP, "<solara-input-bootstrap>")
        .map_err(|error| format!("Solara input bootstrap failed: {error}"))?;

    let mut executed = 0usize;
    for script in scripts {
        let filename = script_filename(watch_url, script);
        println!(
            "strict script order={} bytes={} type={} source={}",
            script.order,
            script.source.len(),
            script.media_type,
            filename,
        );
        match execute_script(js, &script.source, &filename) {
            Ok(()) => {
                executed += 1;
                println!("strict script order={} result=ok", script.order);
            }
            Err(error) => {
                println!(
                    "strict script order={} result=error\n{}",
                    script.order, error
                );
                print_probe_events(js);
                println!(
                    "strict_summary executed={} skipped={} stopped_at={} reason=first-script-error",
                    executed, skipped, script.order
                );
                return Ok(());
            }
        }
    }

    print_probe_events(js);
    println!("strict_summary executed={executed} skipped={skipped} result=all-scripts-returned");
    Ok(())
}

fn run_scout(watch_url: &Url, scripts: &[PageScript], skipped: usize) -> Result<(), String> {
    let mut learned_globals = BTreeSet::new();
    let mut replay = 0usize;

    'replay: loop {
        replay += 1;
        let mut js = JsEngine::new().map_err(|error| error.to_string())?;
        js.eval_void(BROWSER_PROBE_BOOTSTRAP, "<solara-youtube-probe-bootstrap>")
            .map_err(|error| format!("scout browser bootstrap failed: {error}"))?;
        js.eval_void(SOLARA_INPUT_BOOTSTRAP, "<solara-input-bootstrap>")
            .map_err(|error| format!("scout input bootstrap failed: {error}"))?;
        js.eval_void(SCOUT_BOOTSTRAP, "<solara-youtube-scout-bootstrap>")
            .map_err(|error| format!("scout membrane bootstrap failed: {error}"))?;
        let globals_json = serde_json::to_string(&learned_globals)
            .map_err(|error| format!("failed to encode scout globals: {error}"))?;
        js.eval_void(
            &format!("globalThis.__solaraScoutInstallGlobals({globals_json});"),
            "<solara-youtube-scout-globals>",
        )
        .map_err(|error| format!("failed to install scout globals: {error}"))?;

        let mut executed = 0usize;
        for script in scripts {
            js.eval_void(
                &format!("globalThis.__solaraScoutSetScript({});", script.order),
                "<solara-youtube-scout-script-marker>",
            )
            .map_err(|error| format!("failed to mark scout script: {error}"))?;
            let filename = script_filename(watch_url, script);
            match execute_script_with_timeout(
                &mut js,
                &script.source,
                &filename,
                SCOUT_SCRIPT_TIMEOUT,
            ) {
                Ok(()) => executed += 1,
                Err(error) => {
                    if let Some(name) = missing_global_from_error(&error)
                        && !learned_globals.contains(&name)
                        && learned_globals.len() < MAX_SCOUT_GLOBALS
                    {
                        println!(
                            "scout learn_global={name} script={} replay={replay} action=clean-replay",
                            script.order
                        );
                        learned_globals.insert(name);
                        continue 'replay;
                    }

                    println!(
                        "scout_frontier script={} bytes={} replay={} error={}",
                        script.order,
                        script.source.len(),
                        replay,
                        error
                    );
                    print_scout_report(&mut js);
                    println!(
                        "scout_summary executed={} skipped={} learned_globals={} replays={} result=semantic-frontier",
                        executed,
                        skipped,
                        learned_globals.len(),
                        replay
                    );
                    print_learned_globals(&learned_globals);
                    return Ok(());
                }
            }
        }

        print_scout_report(&mut js);
        println!(
            "scout_summary executed={} skipped={} learned_globals={} replays={} result=all-scripts-returned",
            executed,
            skipped,
            learned_globals.len(),
            replay
        );
        print_learned_globals(&learned_globals);
        return Ok(());
    }
}

fn script_filename(watch_url: &Url, script: &PageScript) -> String {
    script
        .source_url
        .as_ref()
        .map(Url::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{}#inline-script-{}", watch_url, script.order))
}

fn missing_global_from_error(error: &str) -> Option<String> {
    let reference_error = error
        .lines()
        .find(|line| line.contains("ReferenceError:"))?;
    if let Some(rest) = reference_error
        .split_once("ReferenceError: '")
        .map(|pair| pair.1)
        && let Some((name, _)) = rest.split_once("' is not defined")
        && !name.is_empty()
    {
        return Some(name.to_owned());
    }
    let rest = reference_error.split_once("ReferenceError: ")?.1;
    let (name, _) = rest.split_once(" is not defined")?;
    (!name.is_empty()).then(|| name.trim_matches(['\'', '"']).to_owned())
}

fn print_learned_globals(globals: &BTreeSet<String>) {
    match serde_json::to_string(globals) {
        Ok(globals) => println!("scout_learned_globals={globals}"),
        Err(error) => println!("scout_learned_globals_error={error}"),
    }
}

fn print_scout_report(js: &mut JsEngine) {
    match js.eval_json("globalThis.__solaraScoutReport()", "<scout-report>") {
        Ok(report) => println!("scout_api_report={report}"),
        Err(error) => println!("scout_api_report_error={error}"),
    }
}

fn execute_script(js: &mut JsEngine, source: &str, filename: &str) -> Result<(), String> {
    execute_script_with_timeout(js, source, filename, SCRIPT_TIMEOUT)
}

fn execute_script_with_timeout(
    js: &mut JsEngine,
    source: &str,
    filename: &str,
    timeout: Duration,
) -> Result<(), String> {
    js.eval_void_with_timeout(source, filename, timeout)
        .map_err(|error| error.to_string())
}

fn load_script(document_url: &Url, value: &Value) -> Result<Option<PageScript>, String> {
    let object = value
        .as_object()
        .ok_or_else(|| String::from("script artifact is not an object"))?;
    let order = object.get("order").and_then(Value::as_u64).unwrap_or(0) as usize;
    let tag_html = object.get("tagHtml").and_then(Value::as_str).unwrap_or("");
    let media_type = html_attribute(tag_html, "type")
        .unwrap_or_else(|| String::from("text/javascript"))
        .to_ascii_lowercase();
    if !matches!(
        media_type.as_str(),
        "" | "text/javascript" | "application/javascript" | "text/ecmascript"
    ) {
        println!("script order={order} result=skip type={media_type}");
        return Ok(None);
    }

    let src = object.get("src").and_then(Value::as_str).unwrap_or("");
    if src.is_empty() {
        return Ok(Some(PageScript {
            order,
            source_url: None,
            source: object
                .get("scriptText")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            media_type,
        }));
    }

    let source_url = document_url
        .join(src)
        .map_err(|error| format!("script {order} has invalid src {src:?}: {error}"))?;
    let source =
        fetch_text(&source_url).map_err(|error| format!("script {order} fetch failed: {error}"))?;
    Ok(Some(PageScript {
        order,
        source_url: Some(source_url),
        source,
        media_type,
    }))
}

fn fetch_text(url: &Url) -> Result<String, String> {
    let mut response = ureq::get(url.as_str())
        .header("User-Agent", USER_AGENT)
        .header("Accept-Language", "en-US,en;q=0.9")
        .call()
        .map_err(|error| format!("failed to fetch {url}: {error}"))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("failed to read {url}: {error}"))
}

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

fn print_probe_events(js: &mut JsEngine) {
    match js.eval_json("globalThis.__solaraProbeEvents", "<probe-events>") {
        Ok(events) => println!("browser_probe_events={events}"),
        Err(error) => println!("browser_probe_events_error={error}"),
    }
}

#[cfg(test)]
mod tests {
    use rust_qjs_dom::JsEngine;

    use super::{
        BROWSER_PROBE_BOOTSTRAP, SCOUT_BOOTSTRAP, SOLARA_INPUT_BOOTSTRAP, html_attribute,
        missing_global_from_error,
    };

    #[test]
    fn extracts_quoted_and_unquoted_script_attributes() {
        assert_eq!(
            html_attribute("<script type=application/ld+json>", "type").as_deref(),
            Some("application/ld+json")
        );
        assert_eq!(
            html_attribute("<script nonce='x' src=\"/base.js\">", "src").as_deref(),
            Some("/base.js")
        );
        assert_eq!(html_attribute("<script>", "src"), None);
    }

    #[test]
    fn extracts_quickjs_missing_global_name() {
        assert_eq!(
            missing_global_from_error(
                "JavaScript exception: ReferenceError: 'MouseEvent' is not defined\n    at <eval>"
            )
            .as_deref(),
            Some("MouseEvent")
        );
        assert_eq!(
            missing_global_from_error("ReferenceError: MediaSource is not defined").as_deref(),
            Some("MediaSource")
        );
        assert_eq!(missing_global_from_error("TypeError: not a function"), None);
    }

    #[test]
    fn scout_phantoms_support_constructor_and_property_discovery() {
        let mut js = JsEngine::new().expect("engine starts");
        js.eval_void(BROWSER_PROBE_BOOTSTRAP, "strict-bootstrap.js")
            .expect("strict bootstrap installs");
        js.eval_void(SCOUT_BOOTSTRAP, "scout-bootstrap.js")
            .expect("scout bootstrap installs");
        js.eval_void(
            r#"
            __solaraScoutInstallGlobals(['MouseEvent']);
            __solaraScoutSetScript(8);
            class SyntheticMouseEvent extends MouseEvent {}
            new SyntheticMouseEvent('click');
            document.createElement('video').futureDecoder.open();
            window.futureBrowserSurface.start();
            "#,
            "scout-subject.js",
        )
        .expect("opaque surface allows census to continue");
        let report = js
            .eval_json("__solaraScoutReport()", "scout-report.js")
            .expect("report serializes");
        assert!(report["distinct"].as_u64().is_some_and(|count| count > 0));
        assert!(
            report["observations"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["script"] == 8))
        );
    }

    #[test]
    fn probe_supplies_legacy_document_nodes_and_real_created_events() {
        let mut js = JsEngine::new().expect("engine starts");
        js.eval_void(BROWSER_PROBE_BOOTSTRAP, "strict-bootstrap.js")
            .expect("strict bootstrap installs");
        js.eval_void(SOLARA_INPUT_BOOTSTRAP, "input-bootstrap.js")
            .expect("input bootstrap installs");
        let result = js
            .eval_json(
                r#"
                (() => {
                    const fragment = document.createDocumentFragment();
                    const text = document.createTextNode('ready');
                    fragment.appendChild(text);
                    const event = document.createEvent('CustomEvent');
                    event.initCustomEvent('solara-ready', true, true, { ok: true });
                    return {
                        fragment: fragment instanceof DocumentFragment,
                        text: text instanceof Text,
                        detail: event.detail.ok,
                        dispatched: document.dispatchEvent(event),
                    };
                })()
                "#,
                "legacy-document-probe.js",
            )
            .expect("legacy document values work together");
        assert_eq!(result["fragment"], true);
        assert_eq!(result["text"], true);
        assert_eq!(result["detail"], true);
        assert_eq!(result["dispatched"], true);
    }
}
