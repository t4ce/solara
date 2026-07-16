# Architecture and proof boundary

RustQJSDom has two public library layers and one optional process boundary.

```text
                  +-------------------------------+
Rust host --------| JsEngine                      |
                  | - owns JSRuntime + JSContext  |
                  | - classic script evaluation  |
                  | - embedded ESM loader         |
                  | - JSON calls/results          |
                  +---------------+---------------+
                                  |
                                  v
                  +-------------------------------+
HTML + URL -------| DomEngine                     |
                  | - Parse5                      |
                  | - Lightning CSS + cascade     |
                  | - acyclic normalized DOM      |
                  | - asset/media request index   |
                  | - TrueSurfer widlib           |
                  | - style/script metadata       |
                  +---------------+---------------+
                                  |
                      typed DomArtifact / JSONL
                                  |
                                  v
                       external layout/renderer
```

## Ownership

`JsEngine` is deliberately `!Send` and `!Sync`. A QuickJS runtime/context pair
stays on its creating thread and is destroyed in context-before-runtime order.
All public results are owned Rust strings or `serde_json::Value` instances.
QuickJS values are freed before each public call returns.

Rust host functions use a heap-stable context state and integer callback IDs.
Arguments and results cross that boundary as JSON. Callback panics are caught
and converted to JavaScript exceptions; Rust never unwinds through QuickJS C
frames. Argument conversion finishes before the callback state is mutably
borrowed so user-defined `toJSON` code cannot re-enter an aliased Rust borrow.

`DomEngine` owns one `JsEngine`. Browser code can use `DomEngine::js_mut()` to
install application JavaScript into that same runtime instead of creating a
second engine. The private `__rustQjsDom*` globals are reserved for the DOM
pipeline.

## Embedded modules

`build.rs` scans `js/**/*.mjs` and creates an embedded module table. The module
loader has no filesystem or network fallback. A missing import fails with a
QuickJS exception, making the runtime graph reproducible and usable offline once
Cargo dependencies are cached.

The QuickJS tokenizer reads one sentinel byte past its explicit source length.
Every Rust-provided source buffer therefore includes a readable trailing NUL;
the NUL is not included in the evaluated length.

## Stable boundary

`DomArtifact` validates both `schema == "rustqjsdom.artifact"` and
`schemaVersion == 2` before it reaches a caller. The normalized Parse5 graph
omits parent pointers while retaining:

- document and doctype nodes;
- HTML, SVG, and MathML namespaces;
- namespaced attributes;
- comments and text;
- template document fragments;
- parser-corrected HTML5 tree structure.

Widget descriptors intentionally remain flexible JSON. Their registry surface
is still evolving and should not force churn in the stable normalized DOM
types.

Linked CSS is supplied by a browser-owned callback after Parse5 and before the
cascade. Asset/media references are emitted as metadata only; their raw URL,
base context, initiator, element, attribute, and kind let the browser apply its
own resolution and fetch policy without introducing networking into this crate.

## Excluded boundary

The embedded graph must not import:

- TrueSurfer render-tree or layout modules;
- paint-plan construction;
- TRUEOS host modules, filesystem, networking, Embassy, or GPU state.

Page `<script>` contents are metadata and are not evaluated by `DomEngine`.
Script execution is an explicit browser-host action through `JsEngine`.

## Proof suite

The test suite proves:

- generic QuickJS evaluation, JSON calls, Rust callbacks, and exception stacks;
- repeated use of one runtime across a document batch;
- HTML5 error correction, duplicate attributes, entities, template content,
  foster parenting, SVG namespaces, and XLink attributes;
- page scripts remain inert during parsing;
- renderer keys do not cross the artifact boundary;
- linked/inline CSS cascade order and `!important` behavior;
- HTML, CSS, image, media, and favicon request discovery;
- the CLI and persistent JSONL recovery path;
- embedded source imports remain on the allowed side of the boundary;
- the checked-in schema identity matches the Rust constants.

Use the full acceptance gate:

```sh
./scripts/prove.sh
```
