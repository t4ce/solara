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
