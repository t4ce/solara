//! UI4 owns the window; this adapter submits the document's native text/lines.
use crate::native_images::{Images, Resources};
use rust_qjs_dom::DomEngine;
use solara::{
    native_paint::{PageMesh, Painter},
    navigation::{BuiltInDemo, NavigationTarget},
    spec_layout::{SpecLayout, Viewport},
};
use std::sync::Arc;
use trueos::ui4_scene::{Damage, Error as UiError, Frame, MenuEntry, ResizeEvent};
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
const BROWSER_CADENCE_MS: u64 = 250;

#[derive(Default)]
struct MenuState {
    zoom_delta: i32,
    collapse: bool,
}
fn menu_entries() -> [MenuEntry<'static, MenuState>; 3] {
    [
        MenuEntry::new("+", |s| s.zoom_delta += 10),
        MenuEntry::new("-", |s| s.zoom_delta -= 10),
        MenuEntry::new("collapse", |s| s.collapse = true),
    ]
}
#[derive(Clone, Copy)]
struct Expanded {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}
struct Swoop {
    started: u64,
    from: (i32, i32),
    to: (i32, i32),
}

struct Window {
    menu: MenuState,
    zoom_percent: i32,
    needs_reflow: bool,
    expanded: Option<Expanded>,
    swoop: Option<Swoop>,
    icon_press: Option<(trueos::ui4_scene::CursorSource, u32, u32)>,
    favicon: crate::native_favicon::Favicon,
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
    pointer: Option<(trueos::ui4_scene::CursorSource, [f32; 2])>,
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
        let mut frame = Frame::open_streaming(x, y, width, height)?;
        frame.register_context_menu(&menu_entries())?;
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
        let mut favicon = crate::native_favicon::Favicon::default();
        favicon.navigate(resources.favicon.clone());
        Ok(Self {
            menu: MenuState::default(),
            zoom_percent: 100,
            needs_reflow: false,
            expanded: None,
            swoop: None,
            icon_press: None,
            favicon,
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
            pointer: None,
            resize: None,
            dirty: true,
            uploaded: false,
            draws: Vec::new(),
            failed: false,
        })
    }
    fn zoom(&self) -> f32 {
        self.zoom_percent as f32 / 100.0
    }
    fn document_size(&self) -> (u32, u32) {
        self.expanded
            .map(|e| (e.width, e.height))
            .unwrap_or((self.frame.width(), self.frame.height()))
    }
    fn viewport(&self) -> Viewport {
        Viewport {
            window_size: self.document_size(),
            zoom: self.zoom(),
            ..Default::default()
        }
    }
    fn collapse(&mut self) -> Result<(), Error> {
        let (x, y) = self.frame.position()?;
        let expanded = Expanded {
            x,
            y,
            width: self.frame.width(),
            height: self.frame.height(),
        };
        self.frame.resize(64, 64)?;
        self.expanded = Some(expanded);
        self.frame.set_primary_activation(true)?;
        self.swoop = Some(Swoop {
            started: trueos::clock::monotonic_millis(),
            from: (x, y),
            to: (
                x,
                y.saturating_add(expanded.height.saturating_sub(64) as i32),
            ),
        });
        self.icon_press = None;
        self.dirty = true;
        crate::parser_probe::report_info(format_args!(
            "solara: collapsed window={} saved={}x{}@{},{} tile=64x64",
            self.frame.window_id(),
            expanded.width,
            expanded.height,
            x,
            y
        ));
        Ok(())
    }
    fn expand(&mut self) -> Result<(), Error> {
        let Some(saved) = self.expanded else {
            return Ok(());
        };
        self.frame.resize(saved.width, saved.height)?;
        self.frame.set_position(saved.x, saved.y)?;
        self.frame.set_primary_activation(false)?;
        self.expanded = None;
        self.swoop = None;
        self.icon_press = None;
        self.dirty = true;
        self.uploaded = false;
        self.needs_reflow = true;
        crate::parser_probe::report_info(format_args!(
            "solara: expanded window={} restored={}x{}@{},{}",
            self.frame.window_id(),
            saved.width,
            saved.height,
            saved.x,
            saved.y
        ));
        Ok(())
    }
    fn tick_collapsed(&mut self) -> Result<(), Error> {
        if let Some(swoop) = &self.swoop {
            let t = (trueos::clock::monotonic_millis().saturating_sub(swoop.started) as f32
                / 180.0)
                .min(1.0);
            let eased = 1.0 - (1.0 - t).powi(3);
            self.frame.set_position(
                (swoop.from.0 as f32 + (swoop.to.0 - swoop.from.0) as f32 * eased).round() as i32,
                (swoop.from.1 as f32 + (swoop.to.1 - swoop.from.1) as f32 * eased).round() as i32,
            )?;
            if t >= 1.0 {
                self.swoop = None;
            }
        }
        let mut restore = false;
        while let Some(event) = self.frame.take_pointer_event()? {
            if event.buttons_pressed & 1 != 0 {
                self.icon_press = Some((event.source, event.x, event.y));
            }
            if let Some((source, x, y)) = self.icon_press {
                if source != event.source || event.x.abs_diff(x) > 4 || event.y.abs_diff(y) > 4 {
                    self.icon_press = None;
                }
            }
            if event.buttons_released & 1 != 0 {
                restore |= self.icon_press.take().is_some()
                    && (0..64).contains(&event.local_x)
                    && (0..64).contains(&event.local_y);
            }
        }
        while self.frame.take_resize_event()?.is_some() {}
        while self.frame.take_pan_event()?.is_some() {}
        if restore {
            return self.expand();
        }
        if self.dirty {
            self.frame.begin(BACKGROUND)?;
            self.frame.write_opaque_rgba8(&self.favicon.pixels)?;
            self.frame.publish(Damage::full(64, 64))?;
            self.dirty = false;
        }
        Ok(())
    }
    fn tick(&mut self) -> Result<(), Error> {
        let started = trueos::clock::Instant::now();
        self.frame
            .pump_context_menu(&menu_entries(), &mut self.menu)?;
        let zoom_delta = core::mem::take(&mut self.menu.zoom_delta);
        if zoom_delta != 0 {
            self.zoom_percent = (self.zoom_percent + zoom_delta).clamp(10, 500);
            crate::parser_probe::report_info(format_args!(
                "solara: zoom window={} percent={}",
                self.frame.window_id(),
                self.zoom_percent
            ));
            self.pointer = None;
            self.layout.pointer_button(None, self.scroll_y, true);
            self.needs_reflow = true;
        }
        if self.menu.collapse && self.expanded.is_none() {
            self.collapse()?;
        }
        self.menu.collapse = false;
        let icon_changed = self.favicon.poll();
        if self.expanded.is_some() {
            self.dirty |= icon_changed;
            self.tick_collapsed()?;
            return Ok(());
        }
        if self.needs_reflow {
            self.layout.set_viewport(self.viewport())?;
            if !self.layout.resolve(0.0)? {
                return Ok(());
            }
            self.mesh = self.painter.paint(self.layout.document())?;
            self.needs_reflow = false;
            self.dirty = true;
            self.uploaded = false;
        }
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
                zoom: self.zoom(),
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
        let previous_pointer = self.pointer;
        let mut disclosure_changed = false;
        while let Some(event) = self.frame.take_pointer_event()? {
            self.scroll_y -= event.wheel as f32 * 48.0;
            let point = [
                event.local_x as f32 / self.zoom(),
                event.local_y as f32 / self.zoom(),
            ];
            if self
                .pointer
                .is_some_and(|(source, _)| source != event.source)
            {
                self.layout.pointer_button(None, self.scroll_y, true);
            }
            self.pointer = Some((event.source, point));
            if event.buttons_pressed & 1 != 0 {
                self.layout.pointer_button(Some(point), self.scroll_y, true);
            }
            if event.buttons_released & 1 != 0 {
                disclosure_changed |= self
                    .layout
                    .pointer_button(Some(point), self.scroll_y, false);
            }
        }
        while let Some(event) = self.frame.take_pan_event()? {
            self.scroll_y -= event.dy as f32 / self.zoom();
        }
        self.scroll_y = self.scroll_y.clamp(
            0.0,
            (self.mesh.height - self.frame.height() as f32 / self.zoom()).max(0.0),
        );
        if self.scroll_y != previous {
            self.dirty = true;
            self.uploaded = false;
        }
        // Selection changes can remove this cursor's route without delivering
        // another local movement. Do not leave a button stuck in :hover.
        if let Some((source, _)) = self.pointer
            && !self
                .frame
                .input_routes()?
                .iter()
                .any(|route| route.cursor == source && route.selected_for_window)
        {
            self.pointer = None;
            self.layout.pointer_button(None, self.scroll_y, true);
        }
        if disclosure_changed && self.layout.resolve(0.0)? {
            self.mesh = self.painter.paint(self.layout.document())?;
            self.scroll_y = self
                .scroll_y
                .min((self.mesh.height - self.frame.height() as f32 / self.zoom()).max(0.0));
            self.dirty = true;
            self.uploaded = false;
        }
        if (self.dirty || self.pointer != previous_pointer)
            && self
                .layout
                .pointer_move(self.pointer.map(|(_, point)| point), self.scroll_y)
            && self.layout.resolve(0.0)?
        {
            self.mesh = self.painter.paint(self.layout.document())?;
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
        let w = self.frame.width() as f32 / self.zoom();
        let h = self.frame.height() as f32 / self.zoom();
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
        let w = self.frame.width() as f32 / self.zoom();
        let h = self.frame.height() as f32 / self.zoom();
        let visible = self.mesh.viewport(w, h, self.scroll_y);
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut draws = Vec::new();
        let mut remap = vec![(u32::MAX, 0u32); visible.vertices.len()];
        // The broker materializes each draw in contiguous DMA storage. Bound
        // each allocation, and use base_vertex so it copies only that draw.
        let mut line_groups = std::collections::BTreeMap::<u32, Vec<u32>>::new();
        for (line, color) in visible.lines.chunks_exact(2).zip(&visible.line_colors) {
            line_groups
                .entry(*color)
                .or_default()
                .extend_from_slice(line);
        }
        let sources = line_groups
            .iter()
            .map(|(color, indices)| {
                (
                    indices.as_slice(),
                    *color,
                    vgpu::PRIMITIVE_TOPOLOGY_LINE_LIST,
                )
            })
            .chain(std::iter::once((
                visible.triangles.as_slice(),
                INK,
                vgpu::PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
            )));
        for (source, color, topology) in sources {
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
    let favicon = url::Url::parse(url).ok().and_then(|page| {
        let origin = solara::favicon::origin(&page)?;
        let base = artifact
            .asset_index
            .base_href
            .as_deref()
            .and_then(|href| page.join(href).ok())
            .unwrap_or(page);
        Some((
            origin,
            solara::favicon::candidates(&base, &artifact.document),
        ))
    });
    let mut resources = Resources::default();
    resources.favicon = favicon;
    let resources = Arc::new(resources);
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
        self.favicon.navigate(resources.favicon.clone());
        self.needs_reflow = true;
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

fn built_in_demo(demo: BuiltInDemo) -> (&'static str, &'static str) {
    match demo {
        BuiltInDemo::TextAndBorders => (
            "trueos://solara/docs/TextAndBorders.html",
            include_str!("../docs/TextAndBorders.html"),
        ),
        BuiltInDemo::DivsAndPanels => (
            "trueos://solara/docs/DivsAndPanels.html",
            include_str!("../docs/DivsAndPanels.html"),
        ),
        BuiltInDemo::FlowAndForms => (
            "trueos://solara/docs/FlowAndForms.html",
            include_str!("../docs/FlowAndForms.html"),
        ),
    }
}

pub(crate) fn run_browser() -> Result<(), String> {
    use crate::native_tui::{Action, Navigator};
    use std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll, Waker},
    };
    let mut navigator = Navigator::new().map_err(|e| format!("navigator: {e}"))?;
    let initial = crate::run_script::read()?;
    let mut engine =
        DomEngine::with_stylesheet_loader(crate::parser_probe::load_embedded_stylesheet)
            .map_err(|e| e.to_string())?;
    let (output_w, output_h) =
        trueos::ui4_scene::output_dimensions().map_err(|e| format!("output: {e:?}"))?;
    let (width, height) = (
        output_w.saturating_sub(80).clamp(320, 800),
        output_h.saturating_sub(100).clamp(240, 512),
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
    crate::parser_probe::report_info(format_args!(
        "solara: browser-open window={} url=trueos://solara/home",
        window.frame.window_id()
    ));
    navigator.status("Home");
    type FetchPage = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>>>>;
    let mut fetching: Option<(url::Url, FetchPage)> = None;
    let mut styling: Option<(url::Url, SpecLayout, Arc<Resources>)> = None;
    let mut requested = initial.and_then(|r| {
        if let Some(source) = r.source {
            match crate::read_source(&source) {
                Ok(html) => match page_layout(&mut engine, r.url.as_str(), &html, width, height) {
                    Ok((layout, resources)) => styling = Some((r.url.clone(), layout, resources)),
                    Err(error) => navigator.status(error),
                },
                Err(error) => navigator.status(error),
            }
            None
        } else {
            Some(NavigationTarget::Web(r.url))
        }
    });
    loop {
        trueos::vsys::poll_once();
        match navigator.tick().map_err(|e| format!("navigator: {e}"))? {
            Some(Action::Quit) => return Ok(()),
            Some(Action::Navigate(url)) => requested = Some(url),
            None => {}
        }
        if let Some(target) = requested.take() {
            navigator.location(&target);
            navigator.status(format!("Loading {target}"));
            styling = None;
            match target {
                // Replacing the future discards the previous kernel operation.
                NavigationTarget::Web(url) => {
                    fetching = Some((
                        url.clone(),
                        Box::pin(crate::native_images::fetch_bytes(url.into())),
                    ));
                }
                NavigationTarget::Demo(demo) => {
                    fetching = None;
                    let (url, html) = built_in_demo(demo);
                    match page_layout(
                        &mut engine,
                        url,
                        html,
                        window.document_size().0,
                        window.document_size().1,
                    ) {
                        Ok((layout, resources)) => {
                            styling = Some((
                                url::Url::parse(url).expect("built-in demo URLs are valid"),
                                layout,
                                resources,
                            ));
                        }
                        Err(error) => navigator.status(format!("Could not open {demo}: {error}")),
                    }
                }
            }
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
                        window.document_size().0,
                        window.document_size().1,
                    )
                }) {
                Ok((layout, resources)) => {
                    navigator.status("Laying out page…");
                    styling = Some((url, layout, resources));
                }
                Err(error) => navigator.status(format!("Could not open {url}: {error}")),
            }
        }
        let ready = styling.as_mut().map(|(_, layout, resources)| {
            resources.poll()?;
            // Resizes during the fetch must be reflected in the arriving page.
            layout.set_viewport(Viewport {
                ..window.viewport()
            })?;
            layout.resolve(0.0)
        });
        match ready {
            Some(Ok(true)) => {
                let (url, layout, resources) = styling.take().expect("resolved page");
                match window.navigate(layout, resources) {
                    Ok(()) => {
                        navigator.status(format!("Loaded {url}"));
                        crate::parser_probe::report_info(format_args!(
                            "solara: navigation-loaded window={} url={url}",
                            window.frame.window_id()
                        ));
                    }
                    Err(error) => navigator.status(format!("Layout failed: {error}")),
                }
            }
            Some(Err(error)) => {
                styling = None;
                navigator.status(format!("Layout failed: {error}"));
            }
            _ => {}
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
        trueos::vsys::sleep_ms(if window.swoop.is_some() {
            16
        } else {
            BROWSER_CADENCE_MS
        });
    }
}
