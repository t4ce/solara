# Solara

Solara is currently a TRUEOS-first, renderer-free DOM/CSS handoff probe.

The repository retains the browser experiments for later work, but this branch
has one deliberate job: measure how quickly a fixed five-page corpus becomes a
validated RustQJSDom artifact before layout or presentation is reintroduced.

> Current status: Solara initializes one QuickJS-backed RustQJSDom engine,
> parses five embedded documents through Parse5 and Lightning CSS, logs each
> validated handoff and exits. Page scripts are retained in the artifacts but
> are not executed.

## Current boundary

The active `solara` binary performs only DOM/CSS compilation on Linux and
TRUEOS. A page is "handoff ready" after the typed `rustqjsdom.artifact/v2`
contract validates. It does not request a UI4 `Frame`, text rows/canvas,
shapes, Picasso lowering, WGPU surface, Linux window, or presentation.

The embedded corpus is:

- `TrueOsHome.html`: a static snapshot of `https://trueos.eu/`; its visual
  script remains present but inert.
- `demoui.html`: the existing all-HTML-elements fixture.
- `TextAndBorders.html`: text sizes, border variants, and fixed coordinates.
- `DivsAndPanels.html`: nested grid/flex panels, intrinsic tracks, overflow,
  sticky, and absolute positioning.
- `FlowAndForms.html`: ordinary flow, columns, tables, form controls, logical
  properties, and responsive tracks.

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
prints per-page and aggregate timings, and opens no window:

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
parse/validation time, and the internal Parse5/Lightning CSS fields carried by
the artifact. The same engine is reused sequentially for all five pages.

## Project Structure

The active source surface is `src/main.rs` plus `src/parser_probe.rs`.

The old `crates/solara-wgpu-shim` crate and shader sources are removed on this
branch. Its font assets remain as inactive research material. The dependency
lockfile contains no WGPU or Winit packages for the active graph. The vendored
RustQJSDom component is the only active browser stage.

## Roadmap

- Build the basic application entry point and command-line arguments.
- Expose a reduced `window` / `document` API to JavaScript.
- Synchronize JavaScript DOM mutations into the renderer projection.
- Implement resource loading, navigation, and error handling.
- Expand layout, painting, and browser compatibility.

## License

Solara is licensed under the [MIT License](LICENSE). The vendored RustQJSDom component retains its own license and third-party notices under `vendor/RustQJSDom`.
