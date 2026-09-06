//! UI4 owns the window; this adapter submits the document's native text/lines.
use rust_qjs_dom::DomEngine;
use solara::{
    native_paint::{PageMesh, Painter},
    spec_layout::{SpecLayout, Viewport},
};
use trueos::ui4_scene::{Damage, Error as UiError, Frame, ResizeEvent};
use trueos::vgpu::{self, Buffer, Device, Queue, QueueClass, RenderPipeline, ShaderModule};

const DEMOS: [(&str, &str); 4] = [
    (
        "FrameworkLayout.html",
        include_str!("../docs/FrameworkLayout.html"),
    ),
    (
        "TextAndBorders.html",
        include_str!("../docs/TextAndBorders.html"),
    ),
    (
        "DivsAndPanels.html",
        include_str!("../docs/DivsAndPanels.html"),
    ),
    (
        "FlowAndForms.html",
        include_str!("../docs/FlowAndForms.html"),
    ),
];
const BACKGROUND: u32 = u32::from_le_bytes([13, 18, 27, 255]);
const INK: u32 = u32::from_le_bytes([229, 237, 248, 255]);
const CONTOUR: u32 = u32::from_le_bytes([65, 151, 174, 255]);

struct Window {
    frame: Frame,
    layout: SpecLayout,
    painter: Painter,
    mesh: PageMesh,
    device: Device,
    queue: Queue,
    shader: ShaderModule,
    pipeline: RenderPipeline,
    vertices: Option<Buffer>,
    indices: Option<Buffer>,
    scroll_y: f32,
    resize: Option<ResizeEvent>,
    dirty: bool,
    uploaded: bool,
    draws: Vec<vgpu::IndexedBatchDrawV2>,
    failed: bool,
}

#[derive(Debug)]
enum Error {
    Ui(UiError),
    Gpu(i32),
    Other(String),
}
impl From<UiError> for Error {
    fn from(v: UiError) -> Self {
        Self::Ui(v)
    }
}
impl From<i32> for Error {
    fn from(v: i32) -> Self {
        Self::Gpu(v)
    }
}
impl From<String> for Error {
    fn from(v: String) -> Self {
        Self::Other(v)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ui(e) => write!(f, "UI4: {e:?}"),
            Self::Gpu(e) => write!(f, "GPU: {e}"),
            Self::Other(e) => f.write_str(e),
        }
    }
}
impl Error {
    fn busy(&self) -> bool {
        matches!(self, Self::Ui(UiError::Busy)) || matches!(self,Self::Gpu(v) if *v==vgpu::ERR_BUSY)
    }
}

impl Window {
    fn open(
        mut layout: SpecLayout,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        layout.set_viewport(Viewport {
            window_size: (width, height),
            ..Default::default()
        })?;
        if !layout.resolve(0.0)? {
            return Err(Error::Other("stylesheets still pending".into()));
        }
        let frame = Frame::open_streaming(x, y, width, height)?;
        let device = Device::open(vgpu::Capabilities::DEFAULT.union(vgpu::Capabilities::PRESENT))?;
        let queue = device.create_queue(QueueClass::Render)?;
        let shader = device
            .create_shader_module(vgpu::SHADER_PACKAGE_CLIP_POSITION3_IMMEDIATE_RGBA_FNV1A64)?;
        let pipeline = device.create_render_pipeline(shader, 12, 0)?;
        let mut painter = Painter::default();
        let mesh = painter.paint(layout.document())?;
        Ok(Self {
            frame,
            layout,
            painter,
            mesh,
            device,
            queue,
            shader,
            pipeline,
            vertices: None,
            indices: None,
            scroll_y: 0.0,
            resize: None,
            dirty: true,
            uploaded: false,
            draws: Vec::new(),
            failed: false,
        })
    }
    fn tick(&mut self) -> Result<(), Error> {
        while let Some(event) = self.frame.take_resize_event()? {
            self.resize = Some(event);
        }
        if let Some(event) = self.resize {
            self.frame.resize(event.width, event.height)?;
            self.resize = None;
            self.layout.set_viewport(Viewport {
                window_size: (event.width, event.height),
                ..Default::default()
            })?;
            if !self.layout.resolve(0.0)? {
                return Err(Error::Other("resize awaiting stylesheets".into()));
            }
            self.mesh = self.painter.paint(self.layout.document())?;
            self.dirty = true;
            self.uploaded = false;
        }
        let previous = self.scroll_y;
        while let Some(event) = self.frame.take_pointer_event()? {
            self.scroll_y -= event.wheel as f32 * 48.0;
        }
        while let Some(event) = self.frame.take_pan_event()? {
            self.scroll_y -= event.dy as f32;
        }
        self.scroll_y = self.scroll_y.clamp(
            0.0,
            (self.mesh.height - self.frame.height() as f32).max(0.0),
        );
        if self.scroll_y != previous {
            self.dirty = true;
            self.uploaded = false;
        }
        if !self.dirty {
            return Ok(());
        }
        if !self.uploaded {
            self.upload()?;
            self.uploaded = true;
        }
        self.frame.begin_gpu_frame()?;
        let surface = self.device.acquire_ui4_surface(self.frame.window_id())?;
        let mut batch = vgpu::IndexedDrawBatchV2 {
            clear_rgba8_srgb: BACKGROUND,
            ..Default::default()
        };
        batch.draw_count = self.draws.len() as u32;
        batch.draws[..self.draws.len()].copy_from_slice(&self.draws);
        // Empty documents still get a clear via one degenerate triangle.
        if batch.draw_count == 0 {
            batch.draw_count = 1;
            batch.draws[0] = vgpu::IndexedBatchDrawV2 {
                index_count: 3,
                rgba8_srgb: BACKGROUND,
                topology: vgpu::PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
                ..Default::default()
            };
        }
        let point = self.device.submit_ui4_indexed_batch_v2(
            self.queue,
            surface,
            self.pipeline,
            self.vertices.ok_or_else(|| "missing vertices".to_owned())?,
            self.indices.ok_or_else(|| "missing indices".to_owned())?,
            batch,
        )?;
        self.device.wait(self.queue, point.value)?;
        self.frame
            .publish(Damage::full(self.frame.width(), self.frame.height()))?;
        crate::parser_probe::report_info(format_args!(
            "solara: native-frame window={} boxes={} glyphs={} cached_glyphs={} vertices={} triangles={} lines={} scroll_y={} timeline={}",
            self.frame.window_id(),
            self.mesh.boxes,
            self.mesh.glyphs,
            self.painter.cached_glyphs(),
            self.mesh.vertices.len(),
            self.mesh.triangles.len() / 3,
            self.mesh.lines.len() / 2,
            self.scroll_y,
            point.value
        ));
        self.dirty = false;
        Ok(())
    }
    fn upload(&mut self) -> Result<(), Error> {
        let w = self.frame.width() as f32;
        let h = self.frame.height() as f32;
        let visible = self.mesh.viewport(w, h, self.scroll_y);
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut draws = Vec::new();
        // The broker materializes each draw in contiguous DMA storage. Bound
        // each allocation, and use base_vertex so it copies only that draw.
        for (source, color, topology) in [
            (&visible.lines, CONTOUR, vgpu::PRIMITIVE_TOPOLOGY_LINE_LIST),
            (
                &visible.triangles,
                INK,
                vgpu::PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
            ),
        ] {
            for chunk in source.chunks(12_288) {
                if draws.len() == vgpu::MAX_INDEXED_BATCH_V2_DRAWS {
                    return Err(Error::Other(
                        "visible page exceeds native draw capacity".into(),
                    ));
                }
                let base_vertex = (vertices.len() / 12) as i32;
                let first_index = (indices.len() / 4) as u32;
                let mut remap = std::collections::BTreeMap::new();
                for index in chunk {
                    let next = remap.len() as u32;
                    let mapped = *remap.entry(*index).or_insert_with(|| {
                        let [x, y] = visible.vertices[*index as usize];
                        for v in [2.0 * x / w - 1.0, 1.0 - 2.0 * (y - self.scroll_y) / h, 0.0] {
                            vertices.extend_from_slice(&v.to_le_bytes());
                        }
                        next
                    });
                    indices.extend_from_slice(&mapped.to_le_bytes());
                }
                draws.push(vgpu::IndexedBatchDrawV2 {
                    index_count: chunk.len() as u32,
                    first_index,
                    base_vertex,
                    rgba8_srgb: color,
                    topology,
                    reserved: 0,
                });
            }
        }
        self.draws = draws;
        crate::parser_probe::report_info(format_args!(
            "solara: native-upload window={} draws={} vertex_bytes={} index_bytes={}",
            self.frame.window_id(),
            self.draws.len(),
            vertices.len(),
            indices.len()
        ));
        if indices.is_empty() {
            vertices = vec![0; 12];
            indices = vec![0; 12];
        }
        let vb = self.device.create_buffer(
            vertices.len(),
            vgpu::BUFFER_USAGE_MAP_WRITE | vgpu::BUFFER_USAGE_VERTEX,
        )?;
        let ib = match self.device.create_buffer(
            indices.len(),
            vgpu::BUFFER_USAGE_MAP_WRITE | vgpu::BUFFER_USAGE_INDEX,
        ) {
            Ok(v) => v,
            Err(e) => {
                let _ = self.device.destroy_buffer(vb);
                return Err(e.into());
            }
        };
        let result = (|| -> Result<(), Error> {
            if self.device.write_buffer(vb, 0, &vertices)? != vertices.len()
                || self.device.write_buffer(ib, 0, &indices)? != indices.len()
            {
                return Err(Error::Other("short geometry upload".into()));
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = self.device.destroy_buffer(vb);
            let _ = self.device.destroy_buffer(ib);
            return Err(error);
        }
        if let Some(old) = self.vertices.replace(vb) {
            self.device.destroy_buffer(old)?;
        }
        if let Some(old) = self.indices.replace(ib) {
            self.device.destroy_buffer(old)?;
        }
        Ok(())
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        if let Some(v) = self.vertices.take() {
            let _ = self.device.destroy_buffer(v);
        }
        if let Some(v) = self.indices.take() {
            let _ = self.device.destroy_buffer(v);
        }
        let _ = self.device.destroy_render_pipeline(self.pipeline);
        let _ = self.device.destroy_shader_module(self.shader);
        let _ = self.device.destroy_queue(self.queue);
        let _ = self.device.close();
    }
}

pub(crate) fn run(page: Option<(&str, &str)>) -> Result<(), String> {
    let mut engine =
        DomEngine::with_stylesheet_loader(crate::parser_probe::load_embedded_stylesheet)
            .map_err(|e| e.to_string())?;
    let (output_w, output_h) =
        trueos::ui4_scene::output_dimensions().map_err(|e| format!("output: {e:?}"))?;
    let multiple = page.is_none();
    let width = if multiple {
        (output_w / 2).saturating_sub(24).max(320)
    } else {
        output_w.saturating_sub(80).max(320)
    };
    let height = if multiple {
        (output_h / 2).saturating_sub(24).max(240)
    } else {
        output_h.saturating_sub(100).max(240)
    };
    let sources: Vec<(String, &str)> = match page {
        Some((url, html)) => vec![(url.into(), html)],
        None => DEMOS
            .iter()
            .map(|(name, html)| (format!("trueos://solara/docs/{name}"), *html))
            .collect(),
    };
    let mut windows = Vec::new();
    for (index, (url, html)) in sources.iter().enumerate() {
        let artifact = engine.parse(html, url).map_err(|e| e.to_string())?;
        let (layout, missing) = crate::layout_probe::layout_artifact(&artifact)?;
        let x = 12 + (index as u32 % 2) * (width + 12);
        let y = 12 + (index as u32 / 2) * (height + 12);
        let window =
            Window::open(layout, x as i32, y as i32, width, height).map_err(|e| e.to_string())?;
        crate::parser_probe::report_info(format_args!(
            "solara: native-open url={url} window={} size={width}x{height} unavailable_resources={missing}",
            window.frame.window_id()
        ));
        windows.push(window);
    }
    loop {
        trueos::vsys::poll_once();
        for window in &mut windows {
            if window.failed {
                continue;
            }
            if let Err(error) = window.tick() {
                if !error.busy() {
                    crate::parser_probe::report_error(format_args!(
                        "solara: native-window failed window={} error={error}; other windows remain live",
                        window.frame.window_id()
                    ));
                    window.failed = true;
                }
            }
        }
        trueos::vsys::sleep_ms(16);
    }
}
