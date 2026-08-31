# Solara

Solara is a small experimental browser built with Rust and QuickJS.

The goal is to use Rust for the browser shell, resource loading, document model, and rendering pipeline, while QuickJS executes page JavaScript through a lightweight host API that connects the script runtime to the browser environment.

> Current status: RustQJSDom supplies Solara's QuickJS runtime, Parse5 DOM, Lightning CSS cascade, and asset-request index. Solara retains that canonical artifact/runtime pair, resolves favicon and resource URLs, and hands authored computed styles to its existing paint pipeline. TRUEOS compiles this through a headless retained Picasso scene before UI4 presents it. Page-script DOM bindings are still under development; the separately opt-in `sandboxed-scene-js` step exposes only a bounded scene-patch capability.

## Goals

- Implement the core flow in Rust for better control over memory, concurrency, and system integration.
- Use QuickJS as the embedded JavaScript engine for a lightweight ECMAScript runtime.
- Build a simplified browser model focused on URL loading, HTML/CSS parsing, DOM construction, script execution, and basic rendering.
- Keep the codebase small, readable, and experimental, making it useful for learning how browsers work internally.

## Non-Goals

Solara is not a replacement for Chrome, Firefox, or Safari. In its early stages, it does not aim for full web standards compatibility, advanced optimization, or a production-grade security sandbox.

## Requirements

- Rust nightly
- Cargo
- A C compiler for the vendored QuickJS runtime
- Python 3 and Node.js for the isolated YouTube media-cache bridge
- Linux GStreamer runtime plugins for `decodebin`, `videoconvert`, `videoscale`,
  and `fdsink`, plus a decoder matching the cached YouTube representation

The project uses Rust 2024 edition.

## Quick Start

Clone the repository:

```bash
git clone https://github.com/t4ce/solara.git
```

Then run the following command from the project root:

```bash
cargo build --locked
```

Run the one-window playback ladder. It shows the current YouTube target URL, an
advertised-resolution dropdown, and a size-respecting `<video>` box that
conditionally plays its cached media:

```bash
cargo run
```

Use a different YouTube watch URL as the single optional program argument:

```bash
cargo run -- 'https://www.youtube.com/watch?v=nXvnof8fTBc'
```

Run checks:

```bash
cargo check --locked
cargo test --locked
```

On Linux, Solara leaves WGPU 30's Vulkan validation layer disabled because its
current swapchain path reuses an acquire fence without resetting it. Set
`WGPU_VALIDATION=1` when explicitly debugging the backend.

The default `docs/video_demo.html` is parsed by RustQJSDom/Parse5 and styled by
its Lightning CSS stage before Solara builds layout nodes. The exact watch
response, player JavaScript, signed SABR URL, and ustreamer capsule are refreshed under
`solara/media/youtube/<video-id>` in the same cache root. Separately, a pinned,
SHA-256-verified yt-dlp zipapp uses its embedded-player client profile and EJS
support to cache one MP4 representation as `video-<itag>.mp4`. Solara offers one
MP4 choice per advertised resolution. Startup prefers 1440p and falls back once
to the highest lower resolution when unavailable; changing the Solara-rendered
dropdown issues one request for that selection. Each file is checked against
the current watch response's byte length and ISO BMFF header. A complete atomic
cache artifact satisfies the current conservative ten-second readiness
estimate; only then does Linux GStreamer attempt playback. Missing media or
codec support leaves the video box empty without affecting the window. Solara
retains decoded RGBA frames, computes the video box, and composites them in its
own WGPU window.
`docs/demoui.html` remains the render-digest integration fixture. See [the
engine handoff notes](docs/engine-handoff.md) for the boundary and update
workflow.

For the headless compiler boundary, the first sandboxed JavaScript feature
step, and the separation between Solara's SceneDB shadow and Picasso's hosted
redb asset store, see the [headless-to-Picasso map](docs/headless-picasso-map.md).

The static scene supports mixed CSS font sizes end to end. Computed `font-size`
and `line-height` values—including inherited sizes, heading defaults, relative
`em`/`rem`/percentage values, and absolute CSS units—drive both layout and each
glyph run. This is an upfront, single-scene calculation; it does not add font
animation or a retained animation pass.

Solara currently uses Inconsolata as its single layout face. Both the desktop
WGPU renderer and the TRUEOS UI4 text scene paint with that same face, so the
monospaced advances used for wrapping remain identical across backends.

## Project Structure

```text
.
├── Cargo.toml
├── Cargo.lock
├── LICENSE
├── README.md
├── crates
│   └── solara-wgpu-shim
├── docs
│   └── engine-handoff.md
├── src
│   ├── main.rs
│   └── gpu_ui
└── vendor
    └── RustQJSDom
```

`crates/solara-wgpu-shim` is the sole direct owner of WGPU and the glyph stack.
It exposes the full upstream APIs plus Solara's shared GPU context, per-window
surface, acquired frame, and painter composition. The root application depends
only on this shim. The shim is always present; there is no separate no-WGPU
build mode.

Run the text-only renderer, which keeps the WGPU surface and glyph path but
compiles out Solara's shape pipeline:

```bash
cargo run --features gpu-text-only
```

The shim publishes and tests the exact WGPU call inventory for this mode as
`text_only::API_SUBSET`; each matching call is tagged `TEXT_ONLY_WGPU_API` in
the source.

Enable the optional visual GPU activity rail with:

```bash
cargo run --features gpu-visual-debug
```

The five markers show surface configuration, frame acquisition, shape upload,
glyph upload, and submit/present activity. The feature is disabled by default
and adds no overlay state or drawing work to normal builds.

## Roadmap

- Build the basic application entry point and command-line arguments.
- Expose a reduced `window` / `document` API to JavaScript.
- Synchronize JavaScript DOM mutations into the renderer projection.
- Implement resource loading, navigation, and error handling.
- Expand layout, painting, and browser compatibility.

## License

Solara is licensed under the [MIT License](LICENSE). The vendored RustQJSDom component retains its own license and third-party notices under `vendor/RustQJSDom`.
