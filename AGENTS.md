# Repository Guidelines

> Branch note (`true`): the active Solara binary is a renderer-free five-page
> RustQJSDom parse probe. Do not assume the legacy WGPU, Linux window, UI4
> frame, or Picasso presentation paths are active.

## Project Structure & Module Organization

Solara is a Rust 2024 experimental browser project. `src/main.rs` and
`src/parser_probe.rs` embed five documents, reuse one RustQJSDom runtime, and
report when each validated artifact is ready for a future handoff. Historical
browser work remains under `src/gpu_ui/`, including HTML parsing, layout, and
painting experiments, but it is not declared as a module or compiled. The
former WGPU shim crate and shader sources have been removed; its bundled font
remains inactive for future text work.

Reference material and demo inputs live in `docs/`, notably `demoui.html`, `demoui.css`, and `elements.md`. Cargo build output belongs in `target/` and must not be committed.

## Build, Test, and Development Commands

- `cargo build --locked`: compile using the committed dependency lockfile.
- `cargo run --locked`: parse all five embedded pages, print timings, and exit without opening a window.
- `cargo check --locked`: run a fast type and borrow check without producing a binary.
- `cargo test --locked`: run all unit and integration tests; this is also the publish workflow's test command.
- `cargo fmt --all -- --check`: verify standard Rust formatting.
- `cargo clippy --all-targets --all-features -- -D warnings`: catch common Rust mistakes and reject warnings.

Use the current Rust nightly toolchain.

## Coding Style & Naming Conventions

Use `rustfmt` defaults (four-space indentation) and keep modules focused on one rendering responsibility. Follow Rust naming conventions: `snake_case` for modules, functions, and variables; `CamelCase` for structs, enums, and traits; `SCREAMING_SNAKE_CASE` for constants. Prefer explicit error handling over `unwrap()` in runtime paths. Add comments only where rendering, layout, or GPU lifetime constraints are not evident from the code.

## Testing Guidelines

Add focused unit tests in a colocated `#[cfg(test)] mod tests`. Use descriptive
names and add integration tests under `tests/` when behavior crosses module
boundaries. Until a renderer is deliberately reintroduced, validate that all
five artifacts reach the handoff and that the dependency graph remains free of
WGPU.

## Commit & Pull Request Guidelines

Recent history follows Conventional Commit-style prefixes such as `feat:`, `fix:`, `chore:`, and `docs:`. Keep the subject imperative and scoped to one logical change, for example `fix: prevent text overlap`.

Pull requests should explain the behavior change, list verification commands, and link relevant issues. Include before/after screenshots for visual changes. Keep `Cargo.lock` synchronized with dependency changes, and call out any `Cargo.toml` version update because changes on `main` trigger the crates.io publish workflow.
