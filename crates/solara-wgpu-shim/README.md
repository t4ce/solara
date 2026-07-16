# Solara WGPU shim

This crate is Solara's only direct dependency boundary for WGPU and glyph
rendering.

| API | Ownership |
|---|---|
| `GpuContext` | Shared instance, adapter, device, and queue |
| `WindowSurface` | Per-window surface and swapchain configuration |
| `SurfaceFrame` | Acquired surface texture and texture view |
| `GpuPainter` | Shape pipeline, screen uniform, and glyph brush |
| `Renderer` | Convenience composition used by Solara's event loop |
| `Shape` / `TextRun` | Renderer-neutral records accepted from Solara |
| `FontMetrics` / `font_metrics` | CSS-em to glyph-scale conversion plus bundled-font metrics |
| `wgpu` / `wgpu_text` | Complete upstream re-exports for escape-hatch use |

The decomposed API supports rendering into an acquired window frame or any
compatible texture view. The convenience `Renderer` preserves Solara's current
acquire, encode, submit, and present flow.

## Text-only rendering

The `text-only` feature compiles out the shape trait, shader, uniform and
instance buffers, render pipeline, and shape draw calls. The surface lifecycle
and glyph brush remain, so the renderer clears each frame and draws only the
supplied text runs.

Every operation used by that mode is marked with a `TEXT_ONLY_WGPU_API` source
tag and listed in `text_only::API_SUBSET`. A Rust test compares the tags with
the inventory and separately rejects the known shape-pipeline operations:

```bash
cargo test -p solara-wgpu-shim --features text-only \
  text_only::tests -- --nocapture
```

| WGPU subsection | Operations used by text-only mode |
|---|---|
| Setup | Instance, surface, adapter, device, and surface capabilities |
| Frame lifecycle | Configure, acquire, create view, submit, and present |
| Encoding | Create encoder, begin render pass, and finish |
| Glyph rendering | Build/resize brush, queue glyphs, and draw |
| Shape rendering | None |

`text_only::TAG` also labels the text-only command encoder and render pass in
GPU captures. The upstream re-exports remain complete; the inventory records
the narrower API that the shim itself calls in this mode.

## Visual GPU activity

The optional `visual-debug` feature adds a compact activity rail in the upper
right corner. It is implemented entirely in this crate and leaves Solara's
document paint batch unchanged.

Because the rail is made of shapes, enabling `text-only` takes precedence when
both features are selected and omits the rail.

| Marker | Colour | Work boundary |
|---|---|---|
| 1 | Violet | Surface configure or resize |
| 2 | Cyan | Swapchain frame acquire |
| 3 | Amber | Shape instance upload |
| 4 | Magenta | Glyph queue and atlas upload |
| 5 | Green | Command submit and presentation |

An active marker grows into a coloured bar for that frame; inactive markers
remain as short grey ticks. External shim users can add future indications via
`VisualDebug::indicate`, `Renderer::indicate_visual_debug`, and
`GpuPainter::encode_with_visual_debug`.
