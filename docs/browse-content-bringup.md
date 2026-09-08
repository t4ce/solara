# Browse-content rendering frontier

The target is [JoinPeerTube's browse-content page](https://joinpeertube.org/browse-content),
with usable text/layout first, search/results next, and eventually a positioned
video element. Playback is outside this effort. Runtime code contains no
PeerTube selectors, resource substitutions, or site-specific rendering rules.

## This increment

- Removed diagnostic box outlines from the native rendering path.
- Paint authored foreground colors per Parley run, solid backgrounds, rounded
  background outlines, and borders using computed CSS widths and colors.
- Consume Blitz's retained paint-child order, including hoisted stacking
  contexts, instead of node allocation order. Preserve adjacent color runs
  through viewport culling and GPU upload.
- Clip solid/glyph geometry against ancestor overflow rectangles and legacy
  `clip: rect(...)`. Rectangles are transformed into document coordinates;
  clipped accessibility labels no longer leak into the visible page.
- Render input values from Blitz's existing Parley editor layouts.
- Enable Blitz's pure-Rust WOFF/WOFF2 decoder. Late font completion uses normal
  resource invalidation/reflow. Supply proportional sans-serif/system and serif
  fallback fonts rather than assigning monospace to every generic family.
- Add a bounded capture tool and a headless native-mesh SVG probe.

Stylo already receives the original stylesheets, including scoped selectors,
custom properties, media queries, and framework rules. The older Lightning CSS
artifact style table is diagnostic. Its `limitCssObjects` currently imposes no
limit. The tested CSS resources fit the existing 8 MiB per-resource native
limit. No evidence justified raising that cap, the 16 MiB HTML cap, or the
parser's 256 MiB QuickJS heap / 8 MiB stack in this increment.

## Reproduce

Run from the Solara directory. Use a new capture directory each time:

```sh
python3 tools/capture_page.py https://joinpeertube.org/browse-content /tmp/solara-browse-capture
cargo run --locked --example page_probe -- /tmp/solara-browse-capture /tmp/solara-browse.svg 1280 800
cargo run --locked --example page_probe -- /tmp/solara-browse-capture /tmp/solara-browse-small.svg 800 512
cargo test --locked
```

Capture downloads initial HTML, linked CSS, and CSS-referenced resources. It
records every script tag but does not execute or fetch script bundles. It does
not mirror the domain. Limits are 128 resource requests, 8 MiB per resource,
16 MiB HTML, and 64 MiB total; failures are explicit in `capture.json`. The CSS
URL scanner is a diagnostic convenience, not a replacement CSS parser. Full
URLs, including hosts and queries, key the resource manifest. Each response's
final URL is retained for relative CSS resources after redirects.

The probe reads only that capture, uses the real Parse5/Stylo/Taffy/Parley and
native mesh paths, and reports omitted resources plus the script inventory.
SVG output represents solid/glyph geometry; it is **not** a native GPU
screenshot, image-texture proof, or JavaScript compatibility result.

On 2026-09-08 the captured page had six scripts: three module scripts and three
`nomodule` legacy scripts. At 1280×800, the updated probe measured 72 boxes,
550 shaped glyphs, 1,246 px document height, 25,526 visible triangles, and 19
adjacent color runs. Five requested resources were served from the capture;
two HTML image resources were absent. Successful byte delivery does not imply
SVG/image painting. These measurements describe this response, not fixed
assertions about a changing website. The live HTML/CSS/font responses are not
committed as fixtures.

Host regressions cover colored text/input values, overlapping panels, DOM
reordering, z-index, rounded outer edges, overflow/hidden labels, viewport
culling, and delayed WOFF2 completion followed by resize without refetching.
The WOFF2 fixture is a lossless repack of the bundled Inconsolata font; its
existing `assets/fonts/OFL.txt` applies. DejaVu's license is bundled separately.

The native build is checked with `TRUEOS_BLUEPRINT_SKIP_APPS_PUBLISH=1 cargo bp
solara` from TRUEOS-Blueprints. It produces `dist/solara.bp`; building does not
establish physical-rig visual correctness.

Validation for this increment: all 33 host tests and the native Blueprint build
passed. Clippy completes with existing warnings in `src/favicon.rs`; strict
`-D warnings` remains blocked by those warnings. Changed Rust files pass
rustfmt. Whole-package formatting reports the existing module order in
`src/main.rs`; `cargo fmt --all` also encounters the existing nested Crossterm
workspace metadata error.

## Next boundaries

1. **Remaining static fidelity.** PNG/SVG artwork, placeholders, inline
   decorations, gradients/shadows, dashed/dotted/double borders, and curved
   inner border joins remain incomplete. Geometry clipping does not yet apply
   to textured images. Images still follow the complete solid/text batch; a
   mixed textured/solid retained submission is needed for full overlap order.
   Group opacity and all CSS stacking/clip cases also need further work.
2. **Page JavaScript and modules.** The [BIOS increment](bios-bringup.md) adds
   a separate bounded classic-script realm, Promise jobs, limited DOM bindings
   and GET requests. It does not yet provide URL-based ESM dependency loading,
   Vue hydration or a full browser security/scheduling model. Continue from the
   actual missing-API diagnostics rather than increasing execution time alone.
3. **DOM and interaction.** The captured page has server-rendered markup, but
   its search action is Vue-driven. Vue hydration needs live DOM objects,
   traversal/mutation, events, and scheduling; the search bundle also uses
   network requests. Connect those changes to the existing invalidating
   `SpecLayout` document and native frame lifecycle before claiming working
   search. Do not replace the site's application with a PeerTube-specific UI.
4. **Results and media placement.** Once search works, lay out linked result
   cards and thumbnails through the same DOM/CSS path. Preserve an authored
   video/poster rectangle on the next page. Stop before playback.

The bounded goal remains a worthwhile modern-page bring-up. A successful parse,
resource load, script return, layout, native mesh, and actual presented frame
are separate pieces of evidence.
