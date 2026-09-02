# Solara

Solara is currently a TRUEOS-first, renderer-free DOM/CSS handoff probe with a
bounded first page-script step.

The repository retains the browser experiments for later work, but this branch
has one deliberate job: measure how quickly a fixed five-page corpus becomes a
validated RustQJSDom artifact before layout or presentation is reintroduced,
then prove that the retained QuickJS runtime can execute one selected script.

> Current status: Solara initializes one QuickJS-backed RustQJSDom engine,
> parses five embedded documents through Parse5 and Lightning CSS, logs each
> validated handoff, executes at most the first supported classic script per
> page, and exits. Every script tag remains preserved in the artifact; later
> scripts, modules, import maps, and data scripts remain inert.

## Current boundary

The active `solara` binary performs DOM/CSS compilation plus at most one
bounded classic script evaluation per page on Linux and TRUEOS. A page is
"handoff ready" after the typed `rustqjsdom.artifact/v2` contract validates. It
does not request a UI4 `Frame`, text rows/canvas, shapes, Picasso lowering, WGPU
surface, Linux window, or presentation.

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

Clone the repository:

```bash
git clone https://github.com/t4ce/solara.git
```

Then run the following command from the project root. It parses all five pages,
executes the one embedded external-script proof, prints per-page and aggregate
timings, and opens no window:

```bash
cargo run --locked
```

Run checks:

```bash
cargo check --locked
cargo test --locked
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

The active source surface is `src/main.rs`, `src/parser_probe.rs`, and
`src/page_script.rs`.

The old `crates/solara-wgpu-shim` crate and shader sources are removed on this
branch. Its font assets remain as inactive research material. The dependency
lockfile contains no WGPU or Winit packages for the active graph. The vendored
RustQJSDom component is the only active browser parsing stage; Solara owns the
new page-script loading and execution policy.

## Roadmap

- Expose a reduced `window` / `document` API to JavaScript.
- Synchronize JavaScript DOM mutations into style invalidation and the renderer
  projection.
- Add an event loop, timers, `requestAnimationFrame`, and an animation timeline.
- Implement general resource loading, navigation, and error handling.
- Expand layout, painting, and browser compatibility.

## License

Solara is licensed under the [MIT License](LICENSE). The vendored RustQJSDom component retains its own license and third-party notices under `vendor/RustQJSDom`.
