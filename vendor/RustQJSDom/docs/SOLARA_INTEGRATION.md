# Solara integration

Solara pins RustQJSDom at `vendor/RustQJSDom` and consumes artifact v2.

## Repository placement

RustQJSDom is ordinary tracked source inside the Solara repository. A normal
Solara clone includes `vendor/RustQJSDom`; there is no submodule or separate
repository setup.

Use Cargo's dual `version` + `path` form:

```toml
[dependencies]
rust-qjs-dom = { version = "0.1.0", path = "vendor/RustQJSDom" }
```

Repository builds use the vendored path. When Solara is packaged, Cargo can
replace the path with the matching registry release because Solara's current
package include-list does not ship `vendor/`.

## Integrated boundary

1. Solara instantiates one `DomEngine` and supplies its linked-stylesheet loader.
2. Parse5 builds the canonical DOM, then Lightning CSS computes `styleIndex`.
3. `assetIndex` records resource requests without fetching or decoding them.
4. Solara adapts `DomArtifact.document` into its existing layout nodes and uses
   authored computed styles during paint.
5. Solara retains the same `DomEngine`/`JsEngine` for future page bindings.

The integration consumes the normalized document, never the old TrueSurfer
render tree. RustQJSDom owns CSS parsing/cascade; Solara owns URL resolution,
resource policy, layout, paint, WGPU, window state, and input.

## Suggested adapter boundary

```rust
use rust_qjs_dom::{DomArtifact, DomEngine, DomError};

pub struct BrowserDom {
    engine: DomEngine,
}

impl BrowserDom {
    pub fn parse(&mut self, html: &str, url: &str) -> Result<DomArtifact, DomError> {
        self.engine.parse(html, url)
    }
}
```

Keep this adapter outside RustQJSDom. Solara's `HtmlNode`, geometry, dormant
Stylo experiment, and renderer types must not become parser-library dependencies.

## Release gates

- Decide the licensing of extracted first-party TrueSurfer modules. This
  repository currently carries the TRUEOS source-available license, while
  Solara declares MIT and publishes to crates.io; that mismatch must be handled
  explicitly before a public combined release.
- Verify Linux and macOS builds. The current native host is 64-bit Unix-oriented.
- Run this repository's full proof suite and Solara's locked checks.
- Record a fixture mapping Solara's current `HtmlNode` output to the new typed
  DOM before deleting the existing parser.

## Solara acceptance commands

Once wired:

```sh
cargo check --locked
cargo test --locked
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo package --locked
```
