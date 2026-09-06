# Solara

Solara is a TRUEOS browser project. The current milestone parses HTML/CSS and
computes retained layout using Blitz, Stylo, Taffy and Parley. It also has a
bounded first page-script proof and a Shell2 `surf` launch path. On TRUEOS it
now presents a native text-and-lines view in UI4 through the existing indexed
render path used by PotatoStamps.

The goal is faithful rendering of selected modern CSS frameworks and a useful
browser API surface on TRUEOS/Solara. This is not a cross-platform browser shell
or a plan to reproduce every Chrome/Firefox feature.

> Linux host probe: Solara initializes one QuickJS-backed RustQJSDom engine,
> parses five embedded documents through Parse5 and Lightning CSS, logs each
> validated handoff, executes at most the first supported classic script per
> page, imports the original DOM into Blitz, resolves styles and measured
> boxes/glyphs at 1280x800, and exits. Every script tag remains preserved; later
> scripts, modules, import maps, and data scripts remain inert.

When Shell2 launches Solara through `surf <url>`, it supplies a one-shot run
script containing the canonical page URL and the TRUEOSFS path of the staged
HTML. Solara validates the HTTP(S) URL with the Rust `url` crate, reads that
single document, parses and lays it out without executing network page scripts,
and opens a UI4 frame. Its resource loader still serves only the embedded corpus; unavailable
resources are reported and this path is not a complete network page render.

## Current boundary

The default `spec-layout` feature adds a live Blitz document to the validated
`rustqjsdom.artifact/v2` handoff. Parse5 constructs the tree; original style
elements, inline CSS and stylesheet links feed Stylo. Taffy and Parley provide
measured boxes and shaped text. The parser's `styleIndex` remains a diagnostic
snapshot and is not flattened into inline styles. `layout-ready` logs actual
layout counts separately from parsing. On TRUEOS, `native_paint.rs` reads the
resolved document directly and `native_window.rs` submits its geometry to UI4.

See [`docs/spec-layout.md`](docs/spec-layout.md) for the Rust boundary,
dependency pins, host resource contract, validation and remaining native work.
Use `--no-default-features` to run the original parser-only probe.

On the Linux host, a direct run keeps the five-page headless corpus. On TRUEOS,
a direct run opens four independent, tiled UI4 frames: `FrameworkLayout.html`,
`TextAndBorders.html`, `DivsAndPanels.html`, and `FlowAndForms.html`. A `surf`
launch opens the staged page through exactly the same drawing path.

The first view uses pale glyph meshes and cyan box edges on a dark background.
Glyph positions, font bytes, sizes, variation coordinates and synthetic slant
come from Parley. Outlines use the existing Skrifa dependency; the small TRUEOS
path fill helper is copied locally for Blueprint packaging. Native line lists
close each box. Geometry stays resident while idle; wheel/middle-button pan
updates the projection, and UI4 resize events trigger Blitz reflow with cached
glyph outlines. Only viewport-intersecting primitives are uploaded, with compact
line vertices to avoid the broker copying text into line draws. Draws are split
into at most 12,288 indices each so the broker can use small contiguous DMA
allocations, still within one native batch submission per frame. A fatal draw
error is reported per window and leaves the other windows live. No FontCanvas, glyph sprites, WGPU or Winit are used.

This is a diagnostic view: authored fills/colors, rounded borders, nested overflow
clipping, form-control text, and full paint/compositing order are not implemented.
Text uses the existing native triangle rasterizer without a new antialiasing
pass. QJS interaction and continuous animation are subsequent work. The window
loop only draws when geometry, image readiness or scrolling changes.

JPEG `<img>` assets (`.jpg`/`.jpeg`, including URL query strings) now use async
kernel HTTP fetch and `vmedia` decoding. Solara does not include a JPEG decoder.
Decoded dimensions enter Blitz's normal image completion/reflow, and the painter
submits position+UV triangles through the existing sampled Picasso shader. Images
respect the content box, transforms and `object-fit` (fill/contain/cover/none/
scale-down); simple percentage/pixel `object-position` is supported. Background
images, masks, nested overflow clipping and full DOM painter order remain outside
this diagnostic view. Images currently follow the text/line batch.

The kernel's `INDEXED_DRAW_LOAD_COLOR` continuation preserves that batch and keeps
an immutable sampled texture cached for each MAP_WRITE-only pixel buffer. A pixel
write invalidates the cache, and buffer destruction releases it. This first path
uses the decoder's RGBA readback once and retains it in Blitz plus a GPU buffer;
it does not yet use the separate vmedia Render1 retained-texture handle contract.
Scrolling changes geometry, not pixels. The renderer's proven nearest/repeat
sampler is used. A matching kernel is required for the continuation flag.

The FrameworkLayout demo reuses one embedded JPEG above and below the fold,
plus the W3C JPEG-format example fetched over HTTPS. Native
frame logs include geometry upload, render and total frame timings. Idle windows
do not submit frames; resize triggers layout while scrolling reuses glyph meshes.

The embedded corpus is:

- `TrueOsHome.html`: a static snapshot of `https://trueos.eu/`; its import map
  and module script remain present but inert.
- `demoui.html`: the existing all-HTML-elements fixture.
- `TextAndBorders.html`: text sizes, border variants, and fixed coordinates.
- `DivsAndPanels.html`: nested grid/flex panels, intrinsic tracks, overflow,
  sticky, and absolute positioning.
- `FlowAndForms.html`: ordinary flow, columns, tables, form controls, logical
  properties, responsive tracks, and the first external classic-script proof.

## First page-script step

RustQJSDom still preserves the full script inventory without executing it while
parsing. After an artifact validates, Solara scans that inventory in document
order and evaluates only the first supported classic JavaScript tag in the same
retained QuickJS runtime. Inline source is used directly. External source must
also appear as a `kind=script` request in `assetIndex`; Solara then supplies its
text through a browser-owned loader analogous to the linked-CSS loader. The
active corpus loader recognizes only the trusted, repository-embedded proof
file; it does not add network access. Because this milestone shares the retained
parser runtime, arbitrary network page code remains out of scope until a page
realm or equivalent protection isolates parser-private globals. The execution
deadline is 500 ms.

This stage deliberately does not implement parser-blocking, `async`, `defer`,
module loading, a browser event loop, or multiple-script ordering. It also does
not expose a live `window`, DOM, CSSOM, timers, or animation clock, so script
side effects are currently limited to QuickJS state and do not rewrite the
already-produced DOM or `styleIndex` snapshot.

See [`docs/javascript-css-lifecycle.md`](docs/javascript-css-lifecycle.md) for
the execution contract and the relationship between JavaScript mutations, CSS
transitions/keyframes, style invalidation, and the browser frame lifecycle.

## Requirements

- Rust nightly and Cargo
- A C compiler for the vendored QuickJS runtime

The project uses Rust 2024 edition.

## Quick Start

Build inside the TRUEOS checkout layout (`TRUEOS-Blueprints/apps/solara`), with
the existing `../../api`, Blueprint crates and sibling TRUEOS dependencies
available. A standalone clone still needs those native path dependencies for
Cargo manifest resolution, even when testing on a host.

Clone Solara's branch into that location:

```bash
git clone --branch true https://github.com/t4ce/solara.git apps/solara
```

Then run the following command from the project root. It parses all five pages,
executes the one embedded external-script proof, computes layout, prints
per-page timings, and opens no window:

```bash
cargo run --locked
```

Run checks:

```bash
cargo check --locked
cargo test --locked
cargo run --locked --no-default-features
```

The `trueos-first` feature documents the branch selection used by Blueprint
packaging. Solara is listed in the repository-level `buildins.json`, so the
latest `dist/solara.bp` is seeded into TRUEOS `app.db` by the next kernel build.
No WGPU or `wgpu_text` dependency is in the active graph.

The timing lines distinguish QuickJS engine initialization, measured host-side
parse/validation time, first-script selection/evaluation time, and the internal
Parse5/Lightning CSS fields carried by the artifact. The same engine is reused
sequentially for all five pages.

## Project Structure

The active source includes `src/spec_layout/` (retained document import and
layout), `src/layout_probe.rs` (corpus host adapter), `src/parser_probe.rs`,
`src/page_script.rs`, and `src/run_script.rs`. The bundled proof font is in
`assets/fonts/`. Historical `src/gpu_ui/` code remains inactive.

The active dependency graph has no WGPU, Winit, desktop shell or HTTP client.
Host-side Rust tests validate layout behavior; TRUEOS execution still requires
the platform's custom toolchain and native build/presentation validation.

## Roadmap

- Expose a reduced `window` / `document` API to JavaScript.
- Synchronize JavaScript DOM mutations into style invalidation and the renderer
  projection.
- Connect the TRUEOS frame clock and event loop to Blitz's existing animation
  sampling and expose the selected APIs to QuickJS.
- Implement general resource loading, navigation, and error handling.
- Lower the resolved document through a retained Picasso paint adapter.
- Verify selected framework fixtures against their intended geometry and fonts.

## License

Solara is licensed under the [MIT License](LICENSE). The vendored RustQJSDom component retains its own license and third-party notices under `vendor/RustQJSDom`.

## Native visual check

On 2026-09-06 the four default documents all published native GPU frames on the
physical rig, and a fresh WD post-blend screenshot confirmed the four tiled
text/contour views. The Blueprint build and 15 host tests passed. The internal
`solara-native` app entry runs this visual build without replacing the installed
Solara entry. The earlier large FlowAndForms draw hit GPU `-12`; compact
viewport geometry and bounded indexed draws allowed all four frames to render.
