//! UI4 owns the window; this adapter submits the document's native text/lines.
use crate::native_images::{Images, Resources};
use rust_qjs_dom::DomEngine;
use solara::{
    native_paint::{PageMesh, Painter},
    spec_layout::{SpecLayout, Viewport},
};
use std::sync::Arc;
use trueos::ui4_scene::{Damage, Error as UiError, Frame, ResizeEvent};
use trueos::vgpu::{self, Buffer, Device, Queue, QueueClass, RenderPipeline, ShaderModule};

#[cfg(feature = "native-demos")]
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
    resources: Arc<Resources>,
    images: Images,
    image_shader: ShaderModule,
    image_pipeline: RenderPipeline,
    image_vertices: Buffer,
    image_indices: Buffer,
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
        resources: Arc<Resources>,
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
        let image_shader =
            device.create_shader_module(vgpu::SHADER_PACKAGE_CLIP_POSITION3_UV_TEXTURE_FNV1A64)?;
        let image_pipeline = device.create_render_pipeline(image_shader, 20, 0)?;
        let image_vertices =
            device.create_buffer(80, vgpu::BUFFER_USAGE_MAP_WRITE | vgpu::BUFFER_USAGE_VERTEX)?;
        let image_indices =
            device.create_buffer(24, vgpu::BUFFER_USAGE_MAP_WRITE | vgpu::BUFFER_USAGE_INDEX)?;
        let mut image_index_bytes = Vec::new();
        for i in [0u32, 1, 2, 0, 2, 3] {
            image_index_bytes.extend_from_slice(&i.to_le_bytes());
        }
        if device.write_buffer(image_indices, 0, &image_index_bytes)? != 24 {
            return Err(Error::Other("short image index upload".into()));
        }
        let mut painter = Painter::default();
        let mesh = painter.paint(layout.document())?;
        Ok(Self {
            resources,
            images: Images::default(),
            image_shader,
            image_pipeline,
            image_vertices,
            image_indices,
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
        let started = trueos::clock::Instant::now();
        let resources_changed = self.resources.poll()?;
        if self
            .images
            .poll(&self.resources, self.device, &mut self.layout)?
            || resources_changed
        {
            if !self.layout.resolve(0.0)? {
                return Ok(());
            }
            self.mesh = self.painter.paint(self.layout.document())?;
            self.dirty = true;
            self.uploaded = false;
        }
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
        let upload_started = trueos::clock::Instant::now();
        if !self.uploaded {
            self.upload()?;
            self.uploaded = true;
        }
        let upload_us = upload_started.elapsed().as_micros();
        let render_started = trueos::clock::Instant::now();
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
        let image_draws = self.draw_images()?;
        let render_us = render_started.elapsed().as_micros();
        self.frame
            .publish(Damage::full(self.frame.width(), self.frame.height()))?;
        crate::parser_probe::report_info(format_args!(
            "solara: native-frame window={} boxes={} glyphs={} cached_glyphs={} vertices={} triangles={} lines={} scroll_y={} timeline={} image_draws={} upload_us={} render_us={} frame_us={}",
            self.frame.window_id(),
            self.mesh.boxes,
            self.mesh.glyphs,
            self.painter.cached_glyphs(),
            self.mesh.vertices.len(),
            self.mesh.triangles.len() / 3,
            self.mesh.lines.len() / 2,
            self.scroll_y,
            point.value,
            image_draws,
            upload_us,
            render_us,
            started.elapsed().as_micros()
        ));
        self.dirty = false;
        Ok(())
    }
    fn draw_images(&mut self) -> Result<usize, Error> {
        let w = self.frame.width() as f32;
        let h = self.frame.height() as f32;
        let mut count = 0;
        for image in &self.mesh.images {
            if !image.visible(w, h, self.scroll_y) {
                continue;
            }
            let Some(texture) = self.images.textures.get(&image.url) else {
                continue;
            };
            let mut bytes = Vec::with_capacity(80);
            for (position, uv) in image.corners.iter().zip(image.uv) {
                for value in [
                    2.0 * position[0] / w - 1.0,
                    1.0 - 2.0 * (position[1] - self.scroll_y) / h,
                    0.0,
                    uv[0],
                    uv[1],
                ] {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
            }
            if self.device.write_buffer(self.image_vertices, 0, &bytes)? != bytes.len() {
                return Err(Error::Other("short image quad upload".into()));
            }
            let surface = self.device.acquire_ui4_surface(self.frame.window_id())?;
            let point = self.device.submit_ui4_indexed(
                self.queue,
                surface,
                self.image_pipeline,
                self.image_vertices,
                self.image_indices,
                vgpu::IndexedDraw {
                    index_count: 6,
                    sampled_texture: texture.buffer.raw(),
                    texture_width: texture.width,
                    texture_height: texture.height,
                    texture_pitch: texture.pitch,
                    sampler_flags: vgpu::SAMPLER_ADDRESS_U_REPEAT | vgpu::SAMPLER_ADDRESS_V_REPEAT,
                    texture_reserved: vgpu::INDEXED_DRAW_LOAD_COLOR,
                    ..Default::default()
                },
            )?;
            self.device.wait(self.queue, point.value)?;
            count += 1;
        }
        Ok(count)
    }
    fn upload(&mut self) -> Result<(), Error> {
        let w = self.frame.width() as f32;
        let h = self.frame.height() as f32;
        let visible = self.mesh.viewport(w, h, self.scroll_y);
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut draws = Vec::new();
        let mut remap = vec![(u32::MAX, 0u32); visible.vertices.len()];
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
                let stamp = draws.len() as u32;
                let mut next = 0u32;
                for index in chunk {
                    let slot = &mut remap[*index as usize];
                    if slot.0 != stamp {
                        let [x, y] = visible.vertices[*index as usize];
                        for v in [2.0 * x / w - 1.0, 1.0 - 2.0 * (y - self.scroll_y) / h, 0.0] {
                            vertices.extend_from_slice(&v.to_le_bytes());
                        }
                        *slot = (stamp, next);
                        next += 1;
                    }
                    indices.extend_from_slice(&slot.1.to_le_bytes());
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
        self.images.release(self.device);
        let _ = self.device.destroy_buffer(self.image_vertices);
        let _ = self.device.destroy_buffer(self.image_indices);
        let _ = self.device.destroy_render_pipeline(self.image_pipeline);
        let _ = self.device.destroy_shader_module(self.image_shader);
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

#[cfg(feature = "native-demos")]
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
        let resources = Arc::new(Resources::default());
        let layout = SpecLayout::from_artifact(
            &artifact,
            solara::spec_layout::DocumentConfig {
                viewport: Some(Viewport {
                    window_size: (width, height),
                    ..Default::default()
                }),
                font_ctx: Some(solara::spec_layout::bundled_font_context()),
                net_provider: Some(resources.clone()),
                ..Default::default()
            },
        )?;
        let x = 12 + (index as u32 % 2) * (width + 12);
        let y = 12 + (index as u32 / 2) * (height + 12);
        let window = Window::open(layout, resources, x as i32, y as i32, width, height)
            .map_err(|e| e.to_string())?;
        crate::parser_probe::report_info(format_args!(
            "solara: native-open url={url} window={} size={width}x{height}",
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

fn page_layout(
    engine: &mut DomEngine,
    url: &str,
    html: &str,
    width: u32,
    height: u32,
) -> Result<(SpecLayout, Arc<Resources>), String> {
    let artifact = engine.parse(html, url).map_err(|e| e.to_string())?;
    let resources = Arc::new(Resources::default());
    let layout = SpecLayout::from_artifact(
        &artifact,
        solara::spec_layout::DocumentConfig {
            viewport: Some(Viewport {
                window_size: (width, height),
                ..Default::default()
            }),
            font_ctx: Some(solara::spec_layout::bundled_font_context()),
            net_provider: Some(resources.clone()),
            ..Default::default()
        },
    )?;
    Ok((layout, resources))
}

impl Window {
    fn navigate(&mut self, layout: SpecLayout, resources: Arc<Resources>) -> Result<(), String> {
        let mesh = self.painter.paint(layout.document())?;
        self.images.release(self.device);
        self.layout = layout;
        self.resources = resources;
        self.mesh = mesh;
        self.scroll_y = 0.0;
        self.dirty = true;
        self.uploaded = false;
        self.failed = false;
        Ok(())
    }
}

pub(crate) fn run_browser(initial: Option<crate::run_script::OpenRequest>) -> Result<(), String> {
    use crate::native_tui::{Action, Navigator};
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };
    let mut navigator = Navigator::new().map_err(|e| format!("navigator: {e}"))?;
    let mut engine =
        DomEngine::with_stylesheet_loader(crate::parser_probe::load_embedded_stylesheet)
            .map_err(|e| e.to_string())?;
    let (output_w, output_h) =
        trueos::ui4_scene::output_dimensions().map_err(|e| format!("output: {e:?}"))?;
    let (width, height) = (
        output_w.saturating_sub(80).max(320),
        output_h.saturating_sub(100).max(240),
    );
    let (layout, resources) = page_layout(
        &mut engine,
        "trueos://solara/home",
        include_str!("../docs/home.html"),
        width,
        height,
    )?;
    let mut window =
        Window::open(layout, resources, 40, 50, width, height).map_err(|e| e.to_string())?;
    navigator.status("Home");
    type FetchPage = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>>>>;
    let mut fetching: Option<(url::Url, FetchPage)> = None;
    let mut styling: Option<(url::Url, SpecLayout, Arc<Resources>)> = None;
    let mut requested = initial
        .map(|r| {
            if let Some(source) = r.source {
                match crate::read_source(&source) {
                    Ok(html) => {
                        match page_layout(&mut engine, r.url.as_str(), &html, width, height) {
                            Ok((layout, resources)) => {
                                styling = Some((r.url.clone(), layout, resources))
                            }
                            Err(error) => navigator.status(error),
                        }
                    }
                    Err(error) => navigator.status(error),
                }
                None
            } else {
                Some(r.url)
            }
        })
        .flatten();
    loop {
        trueos::vsys::poll_once();
        match navigator.tick().map_err(|e| format!("navigator: {e}"))? {
            Some(Action::Quit) => return Ok(()),
            Some(Action::Navigate(url)) => requested = Some(url),
            None => {}
        }
        if let Some(url) = requested.take() {
            navigator.location(&url);
            navigator.status(format!("Loading {url}"));
            styling = None;
            // Replacing the future discards the previous kernel operation.
            fetching = Some((
                url.clone(),
                Box::pin(crate::native_images::fetch_bytes(url.into())),
            ));
        }
        if let Some((_, future)) = &mut fetching
            && let Poll::Ready(result) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
        {
            let (url, _) = fetching.take().expect("polled request");
            match result
                .and_then(|bytes| {
                    String::from_utf8(bytes).map_err(|_| "Page is not UTF-8 HTML".into())
                })
                .and_then(|html| {
                    page_layout(
                        &mut engine,
                        url.as_str(),
                        &html,
                        window.frame.width(),
                        window.frame.height(),
                    )
                }) {
                Ok((layout, resources)) => {
                    navigator.status("Laying out page…");
                    styling = Some((url, layout, resources));
                }
                Err(error) => navigator.status(format!("Could not open {url}: {error}")),
            }
        }
        if let Some((_, layout, resources)) = &mut styling {
            resources.poll()?;
            // Resizes during the fetch must be reflected in the arriving page.
            layout.set_viewport(Viewport {
                window_size: (window.frame.width(), window.frame.height()),
                ..Default::default()
            })?;
            if layout.resolve(0.0)? {
                let (url, layout, resources) = styling.take().expect("resolved page");
                match window.navigate(layout, resources) {
                    Ok(()) => {
                        navigator.location(&url);
                        navigator.status(format!("Loaded {url}"));
                    }
                    Err(error) => navigator.status(format!("Layout failed: {error}")),
                }
            }
        }
        if !window.failed
            && let Err(error) = window.tick()
            && !error.busy()
        {
            navigator.status(format!("Page rendering failed: {error}"));
            crate::parser_probe::report_error(format_args!(
                "solara: native-window failed window={} error={error}",
                window.frame.window_id()
            ));
            window.failed = true;
        }
        trueos::vsys::sleep_ms(16);
    }
}
