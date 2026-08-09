# RustQJSDom engine handoff

Solara owns RustQJSDom directly under `vendor/RustQJSDom`. The vendored crate owns the embedded QuickJS runtime, loads the TrueSurfer DOM pipeline, and uses Parse5 to produce a typed, versioned `DomArtifact`.

## Runtime boundary

The page flow is:

```text
HTML bytes
  -> RustQJSDom QuickJS + Parse5
  -> linked CSS loader -> Lightning CSS cascade
  -> typed DomArtifact v2 + style/asset indexes + retained DomEngine/JsEngine
  -> Solara HtmlNode projection
  -> layout -> paint -> WGPU
```

`LoadedPage` creates one `DomEngine` with a browser-owned linked-stylesheet callback. Parse5 runs once; Lightning CSS then computes the artifact style table. `Document` owns both the artifact and that same engine. The renderer adapter in `src/gpu_ui/html/parser.rs` copies per-node style references, and paint consumes authored properties from `styleIndex` without reparsing CSS.

RustQJSDom also enumerates HTML, CSS `url(...)`, image, `srcset`, media,
iframe, script, preload, and favicon requests. Except for linked stylesheets,
Solara preserves this index as metadata only: it is not yet connected to a
cache, network media stream, image upload, or paint binding. The resolved
favicon URL remains on `LoadedPage`/`GpuUiApp` for a future window-icon loader.

## Passive YouTube watch boundary

The default Linux session fetches the pinned watch page once and extracts a
typed `YoutubeWatchBootstrap`. It retains the video ID and title, player
JavaScript URL, adaptive-format descriptions, expiry, and the exact opaque
`serverAbrStreamingUrl` and `videoPlaybackUstreamerConfig` values supplied by
YouTube. Solara also atomically caches the exact server responses and opaque
values under `media/youtube/<video-id>/`:

- `watch.html`
- `player-url.txt`
- `player.js`
- `server-abr-url.txt`
- `video-playback-ustreamer-config.bin`
- `video-<itag>.mp4` (selected MP4 media-cache bridge output)

Each launch replaces these snapshots with the mutually current watch session;
signed and expiring values are not treated as permanent. Extraction and cache
identity are fixture-tested and do not rewrite or interpret the stored bytes. A
failed watch fetch or session-cache write is non-fatal and leaves the local
playback proof available.

The watch material remains deliberately passive. On the pinned live response,
a plain ranged `GET` to the supplied SABR URL returned `403` and no body. A
reference-compatible SABR POST reached an `application/vnd.yt-ump` response and
format initialization metadata, but stream protection remained pending and it
returned no media parts. Solara therefore does not invent a SABR exchange,
alter the capsule, or treat either opaque value as a direct URL.

There is one isolated filesystem-before-decoder fast path. On the first default
run, Solara downloads the pinned yt-dlp 2026.07.04 zipapp, verifies its fixed
SHA-256 digest before execution, and runs it through Python 3 with Node-backed
EJS support and the `web_embedded` client profile. Solara derives one MP4 choice
per resolution from the current response, preferring AV1 when multiple MP4
codecs describe the same height. The static startup preference is 1440p; if it
is absent, Solara requests the highest lower advertised resolution once. The
resulting `video-<itag>.mp4` is accepted only when its size equals the selected
format's `contentLength` and its header identifies an ISO BMFF asset. Later runs
validate and reuse the same file without executing the bridge. Tool or
media-fetch failure remains non-fatal to the empty video presentation.

This bridge is a replaceable YouTube-specific black box, not browser media API
emulation and not a second interpretation of SABR. Its output path is the sole
desktop playback source. The supplied target URL becomes the document's one
heading. The only additional page UI is a Solara-painted HTML `<select>` filled
from those already captured format descriptions.

## Local video presentation boundary

The default Linux-only presentation remains intentionally narrower than a media
API. `docs/video_demo.html` contributes the target heading and a specialized
`<video>` layout box. Once the atomically published cache file satisfies a
best-effort ten-second buffer estimate, a bounded mailbox carries 768x432 RGBA
frames from Linux GStreamer to the winit event loop, and Solara uploads those
frames into one retained WGPU texture:

```text
validated YouTube cache artifact -> File-backed EncodedVideoStream
  -> Linux GStreamer decodebin -> RGBA mailbox
  -> retained Solara texture -> computed <video> rectangle -> Solara surface
```

The rectangle follows the document width and preserves 16:9 across relayouts.
The media bridge downloads to yt-dlp's `.part` path and publishes the final
filename only after completion. Solara currently treats that conservative
atomic boundary as more than ten seconds buffered; the estimator is retained
so a later progressive-cache producer can expose an earlier safe point. Opening
the HTML select expands rows in Solara layout and paint. Choosing a different
row cancels the current playback generation, makes exactly one background cache
request, and starts that generation only after validation. There is no embedded
fallback: absent media, a failed validation, or a failed decode leaves the black
rectangle empty. `decodebin` makes codec discovery best effort, and missing host
codec support is non-fatal. This boundary does not connect `src`, JavaScript
media methods, audio, or TRUEOS hardware decode.

The previous Solara `CssEngine`, Stylo dependencies, and duplicate stylesheet
collector have been removed. RustQJSDom/Lightning CSS is the sole CSS path.

Parsing does not execute page `<script>` elements. The retained `JsEngine` now
owns Solara's first browser host binding: mouse input. Page-script execution can
use that same context without creating a second JavaScript runtime.

## Mouse input boundary

Both native producers normalize into `gpu_ui::input::MouseInput`:

```text
Linux winit WindowEvent ─┐
                        ├─> MouseInput -> retained QuickJS document -> window bubble
TrueOS HidHut -> UI4 ───┘
```

The QuickJS host supplies `EventTarget`, `Event`, `CustomEvent`, `UIEvent`,
`MouseEvent`, and `WheelEvent`. Movement, button, wheel, modifier, client, and
screen fields cross the native boundary. Listener exceptions are contained like
browser event-handler exceptions, while `preventDefault()` crosses back to Rust
so the desktop host can suppress native defaults such as scrolling.

Solara does not yet expose its `HtmlNode` projection as live JavaScript element
objects. Until that binding exists, hardware events target `document` and then
bubble to `window`; they are not falsely attributed to a hit-tested element.
TrueOS frame selection and local-coordinate hit testing remain owned by UI4.

## Visual parity proof

The current `docs/demoui.html` is the integration fixture. Its 960-pixel render batch, including the contained `srcdoc` iframe and floating dialogs, is recorded as:

- 141 shape instances
- 83 text sections
- content-height bits `0x45574000`
- FNV-1a render digest `16330a01cc729939`

`current_demo_keeps_its_nested_frame_render_digest` parses that same file through RustQJSDom, verifies the retained QuickJS runtime, and asserts all four values. A second integration test proves authored Lightning CSS reaches the active text paint batch. Run both with the full suite:

```bash
cargo test --locked
```

## Vendored engine workflow

Edit RustQJSDom directly inside the Solara repository. Prove the engine and then run Solara's checks before committing both sides together:

```bash
vendor/RustQJSDom/scripts/prove.sh
cargo test --locked
git add vendor/RustQJSDom Cargo.lock
```

A normal Solara clone contains the complete engine source; no submodule initialization or second repository is required.

## Packaging and licenses

The Cargo dependency includes both a version and a local path. Repository builds use the vendored crate; a crates.io package resolves `rust-qjs-dom = 0.1.0` from the registry.

Solara remains MIT licensed. RustQJSDom retains its own source-available license, and QuickJS retains its upstream license. The vendored component's `LICENSE` and `THIRD_PARTY_NOTICES.md` are authoritative for that code.
