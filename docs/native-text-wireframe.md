# Native text and wireframe integration

Update: the default `spec-layout` feature now implements the retained
Parse5-to-Blitz document boundary and delegates CSS/layout/text to
Stylo/Taffy/Parley. See [spec-layout.md](spec-layout.md) for the current code
and validation. The source audit below describes the earlier parser-only
revision; Picasso painting and native glyph-resource publication remain proposed.

Status: proposed implementation sequence, source audit 2026-09-06.
Solara revisions inspected: `true` at `df7ba43`, local `main` at `f4c3014`.
This document does not claim that the renderer below is implemented.

The first visual target is an HTML/CSS document whose text uses retained glyph
meshes and whose layout boxes use native line geometry, rendered by Picasso
through the TRUEOS engine. Canvas, WebGL, WASM rendering, video, and audio are
outside this milestone.

## What the code actually provides

| Component | Checked state | Consequence |
| --- | --- | --- |
| Parse5, QuickJS, Lightning CSS | Active through vendored RustQJSDom | Keep the typed artifact as the parsing/cascade input. |
| Yoga | No Yoga or Taffy reference in either branch's manifest, lockfile, or Solara source | Layout is custom Rust, not Yoga. |
| Layout | `src/gpu_ui/html/layout.rs` is a small custom layout implementation; the active binary never compiles it | Reintroduce and extend layout deliberately. Successful parsing is not successful layout. |
| CSS consumption | The old style adapter consumes fonts, colors, borders, and limited positioning/dimensions | Retaining flex, grid, gradient, radius, or transform declarations does not implement their semantics. |
| Text | Old layout uses character count times an estimated advance | Replace measurement and wrapping with the same glyph metrics/positions used for rendering. |
| Page logic | Active probe executes at most one supported classic script; no live DOM/CSSOM, animation clock, or invalidation bridge | QuickJS is available, but live page mutation and animation still need integration. |
| Asset loading | Active stylesheet/script callbacks recognize embedded fixtures | An asset request in the artifact does not mean it has been fetched. |
| Old Picasso adapter | Inactive V0 scene compiler emits font lookups and a FontCanvas marker | Its text lowering is not the desired mesh path. |
| Linux video | Local `main` contains a YouTube-specific download/cache bridge and Linux GStreamer-to-WGPU presentation | Useful separate proof; not evidence of general browser media API support. It was not rerun in this audit. |

Lightning CSS describes itself as a CSS parser, transformer, bundler, and
minifier, not a layout engine ([upstream](https://lightningcss.dev/)). Yoga is a
separate embeddable flexbox layout engine
([upstream](https://github.com/react/yoga)); adding it alone would not provide
general inline text or CSS Grid layout.

Code anchors: [active modules](../src/main.rs),
[artifact handoff](../src/parser_probe.rs),
[old layout](../src/gpu_ui/html/layout.rs),
[old style adapter](../src/gpu_ui/html/style.rs),
[old text measurement](../src/gpu_ui/text.rs), and
[script lifecycle](javascript-css-lifecycle.md).

`headless-picasso-map.md` describes an earlier compiled feature configuration.
Its ownership discussion remains useful, but its feature table and "today"
claims must not be used to infer the active `true` runtime.

## Ownership and retained data

```text
HTML + CSS                      QuickJS mutations / frame clock
    |                                       |
    v                                       v
DOM + style -> invalidation -> text shaping + layout -> paint fragments
                                                          |
                                               coherent scene publication
                                                          |
                                                          v
Picasso: glyph/line resources + instances + transforms + clips + paint order
                                                          |
                                                          v
TRUEOS render engine -> released UI4 target -> presentation
```

Follow the existing [Solara–Picasso display contract](../../../../TRUEOS/tools/docs/PICASSO_DOM_SCENEDB_CONTRACT.md):
Solara owns browser semantics and is the single visual-scene writer; Picasso
consumes coherent visual facts. Preserve stable fragment/resource identities,
clip ancestry, paint order, and resource generations. Do not copy DOM or CSS
decisions into the render engine.

The diagram is a logical boundary, not a newly specified packed ABI. Native
transaction publication and the Blueprint transport must be implemented or
verified against that contract before calling the integration complete.

## Text is the first resource path

1. Resolve a font face and shape a text run into glyph IDs, advances, offsets,
   and clusters. Layout and rendering consume that same result. Line breaking
   can feed back into shaping; character counting is not an adequate substitute.
2. Extract outlines in font units and tessellate glyph fills, preserving holes
   and contour winding. Cache by font revision, glyph ID, variation/style, and
   tessellation quality. The engine already has outline and tessellation code
   in [font.rs](../../../../TRUEOS/crates/trueos-graphics/font.rs), but its
   per-call transient mesh storage is not a retained Blueprint font resource API.
3. Publish immutable vertex/index resources once. Retain positioned glyph
   instances or run geometry referencing those resources; do not regenerate a
   page-sized RGBA text image for transform-only changes.
4. Keep color, baseline placement, clip, opacity, and transform independent of
   canonical outline geometry. Repeated glyphs should share geometry where the
   execution path supports it; avoid one host submission per glyph.
5. Validate edge coverage at normal reading sizes early. Triangle interiors
   alone do not solve antialiasing. Use a supported coverage/AA path without
   reintroducing page-sized text raster production. Large zoom may need a finer
   tessellation quality; a fixed coarse mesh is not infinitely scalable.

The first font fixture may use one bundled face and a declared script subset.
It must still measure the actual selected glyphs. A full browser later needs
fallback, shaping across scripts, bidi, selection, and font-loading invalidation.

## Native wireframe, with the actual backend limits

[PotatoStamps](../../../../PotatoStamps/src/main.rs) uploads retained geometry
and submits mixed native topologies. Its circular examples use line lists and
line strips, not a demonstrated `LineLoop` submission.

- Picasso's [core descriptor](../../../../TRUEOS-Picasso/src/core.rs) includes
  `LineLoop`.
- The [V2 draw ABI](../../../crates/trueos-v/src/vgpu.rs) carries topology and
  per-draw color but no stroke width. It permits at most 600 draws per batch.
- The [batch broker](../../../../TRUEOS/src/r/io/vgpu_cabi.rs) used by this
  submission route accepts line lists/strips but omits `LineLoop`.
- The [native pipeline](../../../../TRUEOS/src/intel/render/pipeline.rs)
  currently programs a fixed three-pixel line width.

For the first wireframe, author closed contours as line strips with a repeated
first index, or explicit line-list edges. These are real line primitives.
Generate contours from actual layout fragments, including rounded contours
where supported; do not substitute a polygon-mode view of text triangles for
the page's box wireframe.

Arbitrary CSS stroke widths need a versioned draw-state extension with validated
bounds and defined width units. Caps, joins, dashes, per-side borders, corner
overlap, and antialiasing also require explicit semantics. Fixed native lines
are useful for the diagnostic demo; they are not already a complete CSS border
renderer. Preserve authored topology rather than silently triangulating lines.

## Projection and updates

Use document coordinates in CSS pixels and an orthographic projection with
explicit top-left/Y-down mapping. Picasso already has an orthographic camera
descriptor; its presence does not prove that the required runtime transform
path is connected. For viewport dimensions W/H, pixel coordinates can map to
clip coordinates as `x = 2*x_px/W - 1`, `y = 1 - 2*y_px/H`.

Preserve CSS painter order and clipping. Do not globally split all opaque and
transparent elements into separate passes if that changes overlap semantics.
Group opacity may require an intermediate compositing target; it is not always
equivalent to multiplying every child primitive's alpha.

| Change | Expected work |
| --- | --- |
| Scroll, visual zoom, translate, rotate | Update spatial/projection state; reuse glyph and contour resources where quality permits. |
| Text or font change | Reshape affected runs, load missing glyph resources, relayout affected content. |
| Layout viewport resize or browser zoom that changes CSS viewport | Recompute dependent layout, line breaks, and responsive styles; reuse glyph resources. |
| Color change | Update paint data without rebuilding glyph geometry. |

Smooth visual scaling is not the same cost as responsive reflow. QJS-driven
transform changes should enter the same invalidation/publication path as other
mutations. CSS timelines can be sampled by the browser scheduler without
executing JavaScript for every animated property.

## Implementation sequence and acceptance

1. Establish shaped text measurement and retained glyph-resource publication,
   starting with `TextAndBorders.html`. Check glyph counters, baselines, mixed
   sizes, fractional placement, clipping, and overlapping text/panels.
2. Connect layout fragments to native contour geometry and the coherent
   Picasso scene handoff. Render text plus an optional box wireframe from the
   same layout result.
3. Build the dashboard fixture with precompiled Tailwind CSS or equivalent
   authored CSS. Implement the actual required flex/grid/sizing subset; do not
   assume the existing `DivsAndPanels.html` is laid out because it parses.
4. Add a bounded QJS mutation/frame-clock bridge and demonstrate retained
   translate/rotate/scale updates. Then exercise responsive viewport resizing.
5. Measure on the target: cold shaping/tessellation/upload separately from warm
   CPU frame work, layout, GPU time, draw count, upload bytes, and presentation
   wait. Report p50/p95/p99 against the display's frame budget. Warm transform
   frames should show zero glyph tessellation and zero glyph-geometry upload.

The low geometry workload makes this a promising performance target, not an
FPS guarantee. Per-glyph submissions, full-scene rebuilding, synchronization,
clipping, and edge coverage can dominate a scene with very few triangles.

## Audit validation

`cargo run --locked` completed on the local Linux host: five of five artifacts
validated, zero CSS load errors, one embedded script executed, and the summary
reported `ui4_frame=0 wgpu=0`. This validates the current parser boundary only.
No native rendering, visual correctness, hardware FPS, or Linux video playback
was measured by this audit.
