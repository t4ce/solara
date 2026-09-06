# Retained spec layout for TRUEOS / Solara

This milestone implements layout in Rust. The product target is selected modern
CSS frameworks rendered faithfully on TRUEOS, with a useful browser API surface.
It does not add a desktop browser shell or attempt complete browser replication.

## Active integration

The default `spec-layout` feature imports the existing RustQJSDom artifact into
one retained `blitz_dom::BaseDocument`. Parse5 remains the only HTML tree builder
in this path. The importer preserves element/attribute namespaces, text,
comments, inline CSS, original style elements, linked stylesheet order and
template contents. It does not project `styleIndex` winners into inline CSS.

Blitz connects Stylo's selector/cascade/computed-style machinery to Taffy's
block/flex/grid layout and Parley's text shaping and line breaking. Solara
does not add a second implementation of those algorithms.

The reusable boundary is `solara::spec_layout::SpecLayout`:

| API | Contract |
| --- | --- |
| `from_artifact(artifact, config)` | Import once. Supply an explicit viewport and host resources/fonts through Blitz's `DocumentConfig`. |
| `source_node(path)` | Map an original artifact path to a versioned Blitz NodeId. Paths describe the initial tree; retain NodeIds for live operations. |
| `source_url()` | Original page identity, separate from the effective resource base URL. |
| `mutate()` | Use Blitz's invalidating Rust mutation API. `set_node_text` takes a text node; element text replacement uses the child mutation APIs. |
| `set_viewport(viewport)` | Update physical size, DPI, zoom and color scheme while retaining the document. |
| `resolve(time_seconds)` | Resolve style/layout at the host's animation time. `false` means critical stylesheets are pending; no new generation is published. |
| `document()` | Inspect the actual resolved boxes, inline fragments, computed styles and positioned Parley glyph runs. |
| `summary()` | Counts from that document plus its last completed resolution generation. This is not a paint list or an FPS measurement. |

Keep the owner alongside the page runtime across frames. Read layout after a
successful `resolve`, following any mutations or resource completions. The
current corpus binary retains it through the page handoff and then exits.

`DocumentConfig` accepts the host's `NetProvider`, `ShellProvider`, navigation
provider and font context. This implementation forces sequential Stylo traversal
and disables Blitz's default features, so it selects no desktop windowing,
system-font discovery, HTTP client or renderer. Dependencies still use Rust
`std`; sequential traversal is not a claim of a thread-free dependency graph.

## Original CSS and resources

Original rules and their document order survive the import. Stylo can therefore
reevaluate selectors, cascade layers, custom properties, media queries and CSS
animations as inputs change. Parse5's first `base href` supplies the initial
effective resource URL; the original document URL is retained separately.
Dynamic changes to `base` are not wired in this milestone.

The probe's `CorpusResources` adapter serves the same two embedded stylesheets
as the parser. Other requests are counted as unavailable, receive an empty-byte
completion because Blitz's `NetHandler` lacks an error callback, and are not
claimed as loaded resources. This adapter is only for the existing corpus. A
network-capable TRUEOS page host must provide real resource loading and error
handling. In particular, `layout-ready` does not mean every requested asset or
every authored CSS feature was supported.

The bundled Inconsolata face is registered directly from bytes and mapped to
generic families for a deterministic measurement proof. It is not a substitute
for a designer's chosen fonts. The production font context must supply those
faces/fallbacks; painting must consume the same selected font, glyph IDs,
variation coordinates, sizes and positions as Parley.

## Small integration adaptations

At the pinned Blitz revision, appending children to a detached `<style>` element
still queues it for stylesheet processing. During initial template import,
Solara links only fresh inert nodes through the public document tree escape
hatch. Normal `append_children` subsequently activates that subtree if it is
mounted. The regression test verifies both inert import and later activation.
Live edits to styles inside detached template contents still require addressing
this upstream behavior before exposing a general JavaScript template API.

Blitz currently uses standards mode; doctypes are not imported as live nodes
and quirks-mode compatibility is not claimed.

## Dependency pins

- Blitz DOM/traits: `a50cb8971a03fb4cac697b763f8f5d01ee83cefb`
  (`0.3.0-beta.2`, upstream workspace).
- Stylo: `0.20.0`, via Blitz, patched to
  [`t4ce/stylo@3786da941f0042c923fb5981a0edda86b8d38dba`](https://github.com/t4ce/stylo/commit/3786da941f0042c923fb5981a0edda86b8d38dba).
- Taffy: `1b918bafcab101dd234ebeb27da0443e24fd9de2`, selected by that Blitz revision.
- Parley: `0.11.1`.

`Cargo.lock` is committed. The Solara package version remains `0.0.6`.
No WGPU, Winit, `blitz-shell` or `reqwest` package is in the active graph.
Keep Blitz packages on the same revision when updating the integration.

All eleven Stylo workspace crates use that fork revision to preserve shared
type identity across Blitz and Stylo. The fork excludes TRUEOS/legacy `zkvm`
from Unix pthread-handle extensions and types the Servo thread-count sentinel.
The Blueprint packer carries these full-commit Git patches into its audited
source overlay using Cargo's locked checkout; it does not substitute a
registry Stylo copy. The SDK's Rayon vendor preserves upstream's
`std::io::Result` spawn callback API. Parallel style traversal still needs an
explicit TRUEOS worker adapter with capacity and shutdown ownership.

## Validation

Run in the existing TRUEOS checkout layout with the native path dependencies
available:

```sh
cargo check --locked
cargo test --locked
cargo run --locked
cargo test --locked --no-default-features
cargo run --locked --no-default-features
```

The five-page corpus test now requires measured boxes and positioned glyphs
for every page. The separate integration fixture/tests check:

- Grid/flex geometry, cascade layers and custom properties.
- Responsive reflow at 1200 and 640 CSS pixels, preserving NodeIds/font resources.
- Text-node and class mutations, wrapping and restyle without reimport.
- Linked CSS completion, author source order and relative URLs through `base`.
- Namespaces, artifact references, and inert/activated template contents.
- Host-clock CSS width animation at 0, 0.5 and 1 second.
- Rejection of invalid viewport dimensions, scales and animation times.

`FrameworkLayout.html` is authored coverage for framework CSS mechanisms, not
generated Tailwind and not a claim of complete framework or pixel equivalence.
The host test harness does not create another platform's browser integration.

The implementation is checked on a Linux host. TRUEOS custom-target compilation,
hardware execution and native performance require the TRUEOS toolchain/runtime;
they are not established by these host tests. The parser-only fallback remains
available using `--no-default-features`.

## Initial native view

The TRUEOS binary now reads this resolved document directly in `native_paint.rs`
and submits text triangles and box line lists through `native_window.rs` to UI4.
The headless host probe and tests stay separate. See the README for the exact
diagnostic scope; this is not full CSS painting or native SceneDB publication.

Remaining painting work: extend the native view for the document's painting operations. Retain
glyph/path geometry, stable fragment identities, clip ancestry and painter
order. Include small-text antialiasing in the first visual proof. CSS borders,
group opacity and general affine transforms must be lowered according to the
actual native capabilities.

The Rust document already supports mutation and clock-driven CSS resolution.
QuickJS is still the bounded first-script proof and has no binding to this live
document. Connecting a selected JS API surface and the TRUEOS frame scheduler
is subsequent work. Likewise, a sampled transform is not yet a demonstrated
Picasso transform-only frame: geometry reuse and upload behavior remain to be
measured after that backend exists.
