# RustQJSDom

RustQJSDom is a standard-library QuickJS library and development host for the
renderer-free portion of the TRUEOS TrueSurfer pipeline.

It embeds QuickJS and the existing TrueSurfer JavaScript modules, accepts HTML,
and emits a stable JSON artifact containing:

- an acyclic normalized Parse5 document;
- a Lightning CSS-backed computed-style table with per-node references;
- renderer-neutral HTML, CSS, image, media, and favicon requests;
- TrueSurfer `widlib` widget descriptors and statistics;
- title, favicon, body-outline, stylesheet, and script metadata;
- source identity and parser timings.

It does **not** construct a render tree, perform layout, create paint jobs,
fetch assets, execute page scripts, or draw pixels.

## Boundary

```text
HTML + source URL
        |
        v
   QuickJS host
        |
        +-- Parse5 HTML parsing
        +-- Lightning CSS cascade/style references
        +-- normalized JSON-safe DOM
        +-- asset/media request index
        +-- TrueSurfer widlib descriptors
        +-- raw style/script extraction
        |
        v
 rustqjsdom.artifact/v2
        |
        |  excluded from this repository
        v
 render tree -> layout -> paint plan -> TRUEOS renderer
```

The input corresponds to the current TRUEOS handoff of `(html, url)` into
`__trueosTruesurfer.setHtml()`. The output stops after `domToWidgets()` and
before `createRenderTreeTrace()`.

## Build

Requirements:

- a 64-bit Linux, macOS, or other Unix-like host;
- Rust 1.85 or newer;
- a C compiler and archiver usable by the `cc` crate.

QuickJS is vendored, so its source is never fetched by the build. As usual,
Cargo dependencies must already be cached when building fully offline.

```sh
cargo build
cargo test
```

## CLI

Parse a file and pretty-print the artifact:

```sh
cargo run -- page.html --url https://example.test/page
```

Parse stdin and emit compact JSON:

```sh
printf '<main>Hello</main>' | cargo run -- --compact
```

Write directly to a file:

```sh
cargo run -- page.html -o page.dom.json
```

The persistent JSONL mode reuses one QuickJS runtime across documents. Each
input line must contain `html`; `url` is optional:

```sh
printf '%s\n' \
  '{"url":"https://one.test/","html":"<h1>One</h1>"}' \
  '{"url":"https://two.test/","html":"<button>Two</button>"}' \
  | cargo run -- --jsonl
```

Malformed JSONL requests produce a versioned `rustqjsdom.error` record and do
not terminate the process.

## Rust API

### Typed DOM

```rust
use rust_qjs_dom::{DOM_ARTIFACT_SCHEMA, DomEngine};

let mut engine = DomEngine::new()?;
let artifact = engine.parse(
    "<!doctype html><title>Hello</title><main>World</main>",
    "https://example.test/",
)?;

assert_eq!(artifact.schema, DOM_ARTIFACT_SCHEMA);
assert_eq!(artifact.extracted.title.as_deref(), Some("Hello"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

`DomEngine` is intentionally single-thread-owned because a QuickJS runtime and
context must remain on their owning thread. Create one engine per worker and
reuse it for many sequential documents.

Default limits are 256 MiB of QuickJS-managed memory, an 8 MiB JS stack, and
16 MiB per HTML input. They can be changed through `EngineOptions`.

### JavaScript engine

The same crate exposes the owned QuickJS layer independently:

```rust
use rust_qjs_dom::JsEngine;
use serde_json::json;

let mut js = JsEngine::new()?;
let result = js.eval_json("({ answer: 6 * 7 })", "example.js")?;
assert_eq!(result, json!({ "answer": 42 }));
# Ok::<(), Box<dyn std::error::Error>>(())
```

`JsEngine` supports classic-script evaluation, string or JSON results,
embedded ESM entrypoints, JSON calls into global JavaScript functions, and Rust
host callbacks through `register_json_function`. It returns owned Rust values
only; raw QuickJS pointers never cross the public API.

## Artifact contract

The top-level shape is:

```json
{
  "schema": "rustqjsdom.artifact",
  "schemaVersion": 2,
  "source": { "url": "...", "bytes": 0, "lines": 1 },
  "timings": { "parse5Ms": 0, "lightningCssMs": 0, "totalMs": 0 },
  "document": {},
  "styleIndex": {},
  "assetIndex": {},
  "widgetTree": {},
  "widgetStats": {},
  "extracted": {}
}
```

The normalized document deliberately excludes Parse5 `parentNode` references,
making it serializable and suitable as a process/repository boundary. Template
contents, namespaces, attributes, doctypes, comments, and text are retained.

The `styleIndex` is computed immediately after Parse5. Lightning CSS parses and
minifies author CSS, while the renderer-neutral TrueSurfer cascade emits typed
computed styles and an `authoredProperties` provenance list. A browser-owned
callback may synchronously supply linked stylesheets before the cascade.

The `assetIndex` enumerates resource requests from HTML, `srcset`, inline and
embedded CSS, and loaded external CSS. It carries raw URL/base context and does
not perform network I/O, decoding, caching, or painting. Those remain explicit
browser-host policy.

Changes to the artifact require incrementing `schemaVersion` when consumers
cannot read the old shape safely.

The checked-in JSON Schema is
[`schema/dom-artifact-v2.schema.json`](schema/dom-artifact-v2.schema.json).

## Source layout

- `src/lib.rs` — safe owning API, QuickJS lifecycle, and embedded ESM loader.
- `src/js_engine.rs` — standalone QuickJS runtime API.
- `src/dom_engine.rs` — TrueSurfer pipeline orchestration.
- `src/artifact.rs` — typed artifact and normalized DOM contract.
- `src/main.rs` — file/stdin and persistent JSONL CLI.
- `js/entry.mjs` — standalone HTML-to-artifact boundary.
- `js/parse5/` and `js/cdn/` — local Parse5 7.2.1 entry and bundled payload.
- `js/truesurfer/truesurfer_extract.mjs` — TrueSurfer document metadata extraction.
- `js/truesurfer/css.mjs` — renderer-neutral cascade and computed-style table.
- `js/truesurfer/assets.mjs` — renderer-neutral resource request discovery.
- `js/widlib/` — renderer-agnostic DOM-to-widget descriptors.
- `vendor/quickjs/` — pinned QuickJS source.
- `tests/` — contract and repeat-runtime fixtures.

The initial TrueSurfer modules were extracted from TRUEOS commit
`6be0203d09ab376e14b006c17f9ff0ed72fa984c`. They are now local to this
repository so DOM work can proceed without rebuilding the operating system.

The Solara-owned vendoring layout and integration boundary are documented in
[`docs/SOLARA_INTEGRATION.md`](docs/SOLARA_INTEGRATION.md). Architecture and
proof boundaries are described in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Deliberately excluded

- `rendertree/renderToTree.mjs`, `rendertree/layout.mjs`, and render themes;
- asset fetching/decoding, media streaming, browser queues, hosted surfaces, and input events;
- TRUEOS C ABI, filesystem/network module loading, and kernel shims.

## Licenses

First-party extracted code is covered by [LICENSE](LICENSE). QuickJS and parse5
remain under their respective MIT licenses; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and `licenses/`.
