# Solara

Solara is a small experimental browser built with Rust and QuickJS.

The goal is to use Rust for the browser shell, resource loading, document model, and rendering pipeline, while QuickJS executes page JavaScript through a lightweight host API that connects the script runtime to the browser environment.

> Current status: this project is still in an early skeleton stage. The browser core, QuickJS integration, and page rendering features are under development.

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

The project uses Rust 2024 edition.

## Quick Start

After cloning the repository, run the following command from the project root:

```bash
cargo build
```

Run the default page from `docs/demoui.html`:

```bash
cargo run
```

Load a local file or remote URL:

```bash
cargo run -- ./docs/demoui.html
cargo run -- https://example.com/
```

Run checks:

```bash
cargo check
```

## Project Structure

```text
.
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
└── src
    └── main.rs
```

## Roadmap

- Build the basic application entry point and command-line arguments.
- Add the QuickJS runtime and wrap script execution contexts.
- Implement minimal HTML parsing and DOM data structures.
- Expose a reduced `window` / `document` API to JavaScript.
- Implement resource loading, navigation, and error handling.
- Add a basic layout and painting pipeline.
- Add unit tests and sample pages.

## License

Solara is licensed under the [MIT License](LICENSE).
