# RustQJSDom engine handoff

Solara pins RustQJSDom as the `vendor/RustQJSDom` git submodule. The dependency owns the embedded QuickJS runtime, loads the TrueSurfer DOM pipeline, and uses Parse5 to produce a typed, versioned `DomArtifact`.

## Runtime boundary

The page flow is:

```text
HTML bytes
  -> RustQJSDom QuickJS + Parse5
  -> linked CSS loader -> Lightning CSS cascade
  -> typed DomArtifact v2 + style/asset indexes + retained DomEngine/JsEngine
  -> Solara HtmlNode projection
  -> layout -> paint -> WGPU
```

`LoadedPage` creates one `DomEngine` with a browser-owned linked-stylesheet callback. Parse5 runs once; Lightning CSS then computes the artifact style table. `Document` owns both the artifact and that same engine. The renderer adapter in `src/gpu_ui/html/parser.rs` copies per-node style references, and paint consumes authored properties from `styleIndex` without reparsing CSS.

RustQJSDom also enumerates HTML, CSS `url(...)`, image, `srcset`, media,
iframe, script, preload, and favicon requests. Solara resolves them with the
`url` crate and logs rich request records at `solara::assets=trace`. Except for
linked stylesheets, this is intentionally `action=log-only no_fetch=1`: no
decode, cache, media stream, image upload, or paint binding is part of this
migration. The resolved favicon URL remains on `LoadedPage`/`GpuUiApp` for a
future window-icon loader.

The previous Solara `CssEngine`, Stylo dependencies, and duplicate stylesheet
collector have been removed. RustQJSDom/Lightning CSS is the sole CSS path.

Parsing does not execute page `<script>` elements. The retained `JsEngine` is ready for Solara's future `window` and `document` host bindings without creating a second JavaScript runtime.

## Visual parity proof

The current `docs/demoui.html` is the integration fixture. Before removing `scraper`, its 960-pixel render batch was recorded as:

- 131 shape instances
- 81 text sections
- content-height bits `0x453c8000`
- FNV-1a render digest `3376d634311d33eb`

`current_demo_keeps_its_pre_migration_render_digest` parses that same file through RustQJSDom, verifies the retained QuickJS runtime, and asserts all four values. A second integration test proves authored Lightning CSS reaches the active text paint batch. Run both with the full suite:

```bash
cargo test --locked
```

To inspect the asset boundary:

```bash
RUST_LOG=solara::assets=trace cargo run -- https://example.com/
```

## Submodule workflow

Initialize a checkout with:

```bash
git submodule update --init --recursive
```

To test a newer engine revision, check out the intended commit inside `vendor/RustQJSDom`, run its proof suite, then run Solara's checks and commit the updated gitlink:

```bash
git -C vendor/RustQJSDom checkout <commit>
vendor/RustQJSDom/scripts/prove.sh
cargo test --locked
git add vendor/RustQJSDom Cargo.lock
```

The relative submodule URL resolves beside the Solara repository on GitHub. RustQJSDom must be published at that location before a fresh remote clone can initialize it.

## Packaging and licenses

The Cargo dependency includes both a version and a local path. Local builds use the pinned submodule; a crates.io package resolves `rust-qjs-dom = 0.1.0` from the registry. Publish RustQJSDom before publishing a Solara release that contains this dependency.

Solara remains MIT licensed. RustQJSDom retains its own source-available license, and QuickJS retains its upstream license. The submodule's `LICENSE` and `THIRD_PARTY_NOTICES.md` are authoritative for that component.
