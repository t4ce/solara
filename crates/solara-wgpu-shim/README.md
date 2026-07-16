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
| `wgpu` / `wgpu_text` | Complete upstream re-exports for escape-hatch use |

The decomposed API supports rendering into an acquired window frame or any
compatible texture view. The convenience `Renderer` preserves Solara's current
acquire, encode, submit, and present flow.

## Visual GPU activity

The optional `visual-debug` feature adds a compact activity rail in the upper
right corner. It is implemented entirely in this crate and leaves Solara's
document paint batch unchanged.

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
