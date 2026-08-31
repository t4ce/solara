# Solara

Solara is currently a TRUEOS-first integration placeholder.

The repository retains the browser experiments for later work, but this branch
has one deliberate job: prove the TRUEOS Blueprint entry and dependency
boundary before reintroducing UI.

> Current status: the browser and renderer sources are retained as inactive research material, but the active binary is deliberately inert. The TRUEOS branch currently requests no UI4 frame, text canvas, shape pipeline, Picasso lowering, WGPU surface, Linux window, QuickJS runtime, or DOM compilation.

## Current boundary

The active `solara` binary is inert on Linux and on TRUEOS. Linux prints a
headless placeholder and exits. TRUEOS logs the same state and returns. It does
not request a UI4 `Frame`, text rows/canvas, shapes, Picasso lowering, or
presentation. The `trueos-first` feature is intentionally empty and exists to
make Blueprint selection explicit.

## Requirements

- Rust nightly and Cargo

The project uses Rust 2024 edition.

## Quick Start

Clone the repository:

```bash
git clone https://github.com/t4ce/solara.git
```

Then run the following command from the project root. It prints an inert host
placeholder and opens no window:

```bash
cargo build --locked
```

Run checks:

```bash
cargo check --locked
cargo test --locked
```

The `trueos-first` feature is intentionally empty and documents the branch
selection used by Blueprint packaging. No WGPU or `wgpu_text` dependency is in
the active graph.

Browser fixtures, RustQJSDom, and the previous engine notes remain in the tree
as inactive research material and are not dependencies of this branch.

## Project Structure

The active source surface is just `src/main.rs`.

The old `crates/solara-wgpu-shim` crate and shader sources are removed on this
branch. Its font assets remain as inactive research material. The dependency
lockfile contains no WGPU, Winit, or RustQJSDom packages for the active graph.

## Roadmap

- Build the basic application entry point and command-line arguments.
- Expose a reduced `window` / `document` API to JavaScript.
- Synchronize JavaScript DOM mutations into the renderer projection.
- Implement resource loading, navigation, and error handling.
- Expand layout, painting, and browser compatibility.

## License

Solara is licensed under the [MIT License](LICENSE). The vendored RustQJSDom component retains its own license and third-party notices under `vendor/RustQJSDom`.
