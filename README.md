# Solara

Solara is a small experimental browser built with Rust and QuickJS.

The goal is to use Rust for the browser shell, resource loading, document model, and rendering pipeline, while QuickJS executes page JavaScript through a lightweight host API that connects the script runtime to the browser environment.

> Current status: RustQJSDom supplies Solara's QuickJS runtime, Parse5 DOM, Lightning CSS cascade, and asset-request index. Solara retains that canonical artifact/runtime pair, resolves favicon and resource URLs, and hands authored computed styles to its existing paint pipeline. Page-script DOM bindings are still under development.

## Goals

- Implement the core flow in Rust for better control over memory, concurrency, and system integration.
- Use QuickJS as the embedded JavaScript engine for a lightweight ECMAScript runtime.
- Build a simplified browser model focused on URL loading, HTML/CSS parsing, DOM construction, script execution, and basic rendering.
- Keep the codebase small, readable, and experimental, making it useful for learning how browsers work internally.

## Non-Goals

Solara is not a replacement for Chrome, Firefox, or Safari. In its early stages, it does not aim for full web standards compatibility, advanced optimization, or a production-grade security sandbox.

## Requirements

- Rust 1.85 or later
- Cargo
- A C compiler for the vendored QuickJS runtime

The project uses Rust 2024 edition.

## Quick Start

Clone with the engine submodule, or initialize it in an existing checkout:

```bash
git clone --recurse-submodules https://github.com/t4ce/solara.git
# Existing checkout:
git submodule update --init --recursive
```

Then run the following command from the project root:

```bash
cargo build --locked
```

Run the two-window demo. It opens the bundled `docs/demoui.html` and the
repository-root `preview.html` as independent browser windows:

```bash
cargo run
```

Load a local file or remote URL:

```bash
cargo run -- ./docs/demoui.html
cargo run -- https://example.com/
```

Inspect log-only image/media/asset handoff records without fetching or decoding
those resources:

```bash
RUST_LOG=solara::assets=trace cargo run -- https://example.com/
```

Run checks:

```bash
cargo check --locked
cargo test --locked
```

The default `docs/demoui.html` is parsed by RustQJSDom/Parse5 and styled by its Lightning CSS stage before Solara builds layout nodes. A render-digest regression test locks the no-author-CSS handoff to the approved visual output. See [the engine handoff notes](docs/engine-handoff.md) for the boundary and update workflow.

## Project Structure

```text
.
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
├── docs
│   └── engine-handoff.md
├── src
│   ├── main.rs
│   └── gpu_ui
└── vendor
    └── RustQJSDom (git submodule)
```

## Roadmap

- Build the basic application entry point and command-line arguments.
- Expose a reduced `window` / `document` API to JavaScript.
- Synchronize JavaScript DOM mutations into the renderer projection.
- Implement resource loading, navigation, and error handling.
- Expand layout, painting, and browser compatibility.

## License

Solara is licensed under the [MIT License](LICENSE). The RustQJSDom submodule is a separately licensed component; see its own `LICENSE` and third-party notices after initializing the submodule.
