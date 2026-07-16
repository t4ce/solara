//! Complete WGPU boundary for Solara.
//!
//! The upstream crates are re-exported for escape-hatch access, while the
//! composed API separates shared GPU ownership, per-window presentation, and
//! paint command encoding.

use std::sync::{Arc, OnceLock};

pub use wgpu;
pub use wgpu_text;

#[cfg(not(feature = "text-only"))]
use wgpu::util::DeviceExt;
use wgpu_text::{
    BrushBuilder, TextBrush,
    glyph_brush::{
        Section, Text,
        ab_glyph::{Font, FontArc, ScaleFont},
    },
};
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

#[cfg(feature = "text-only")]
pub mod text_only;

#[cfg(not(feature = "text-only"))]
const SHAPE_SHADER: &str = include_str!("shape.wgsl");

/// Renderer-neutral shape record accepted by [`GpuPainter`].
#[cfg(not(feature = "text-only"))]
pub trait Shape {
    fn position_size(&self) -> [f32; 4];
    fn color(&self) -> [f32; 4];
    fn kind(&self) -> u32;
}

/// Renderer-neutral text record accepted by [`GpuPainter`].
pub trait TextRun {
    fn position(&self) -> (f32, f32);
    fn bounds(&self) -> (f32, f32);
    fn text(&self) -> &str;
    fn color(&self) -> [f32; 4];
    fn scale(&self) -> f32;
}

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(not(feature = "text-only"))]
struct ScreenUniform {
    size: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg(not(feature = "text-only"))]
struct GpuShape {
    pos_size: [f32; 4],
    color: [f32; 4],
    shape_type: u32,
    _pad: u32,
}

#[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VisualDebugEvent {
    SurfaceConfigure,
    FrameAcquire,
    ShapeUpload,
    GlyphUpload,
    SubmitPresent,
}

#[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
impl VisualDebugEvent {
    const ALL: [Self; 5] = [
        Self::SurfaceConfigure,
        Self::FrameAcquire,
        Self::ShapeUpload,
        Self::GlyphUpload,
        Self::SubmitPresent,
    ];

    const fn bit(self) -> u8 {
        1 << self as u8
    }

    const fn color(self) -> [f32; 4] {
        match self {
            Self::SurfaceConfigure => [0.65, 0.38, 0.95, 0.95],
            Self::FrameAcquire => [0.10, 0.78, 0.92, 0.95],
            Self::ShapeUpload => [1.00, 0.63, 0.12, 0.95],
            Self::GlyphUpload => [0.94, 0.28, 0.68, 0.95],
            Self::SubmitPresent => [0.24, 0.82, 0.44, 0.95],
        }
    }
}

/// One-frame visual activity state for costly WGPU boundaries.
#[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
#[derive(Default)]
pub struct VisualDebug {
    active: u8,
}

#[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
impl VisualDebug {
    pub fn indicate(&mut self, event: VisualDebugEvent) {
        self.active |= event.bit();
    }

    pub fn is_active(&self, event: VisualDebugEvent) -> bool {
        self.active & event.bit() != 0
    }

    fn clear(&mut self) {
        self.active = 0;
    }

    fn overlay(&self, width: u32) -> Vec<GpuShape> {
        const PANEL_WIDTH: f32 = 116.0;
        const PANEL_Y: f32 = 10.0;
        const MARKER_WIDTH: f32 = 14.0;
        const MARKER_GAP: f32 = 6.0;
        let panel_x = (width as f32 - PANEL_WIDTH - 10.0).max(10.0);
        let mut shapes = vec![GpuShape {
            pos_size: [panel_x, PANEL_Y, PANEL_WIDTH, 26.0],
            color: [0.04, 0.06, 0.09, 0.78],
            shape_type: 0,
            _pad: 0,
        }];
        for (index, event) in VisualDebugEvent::ALL.into_iter().enumerate() {
            let active = self.is_active(event);
            let height = if active { 14.0 } else { 4.0 };
            let y = PANEL_Y + 18.0 - height * 0.5;
            shapes.push(GpuShape {
                pos_size: [
                    panel_x + 9.0 + index as f32 * (MARKER_WIDTH + MARKER_GAP),
                    y,
                    MARKER_WIDTH,
                    height,
                ],
                color: if active {
                    event.color()
                } else {
                    [0.34, 0.38, 0.44, 0.72]
                },
                shape_type: 0,
                _pad: 0,
            });
        }
        shapes
    }
}

/// Objects shared by every Solara window.
pub struct GpuContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl GpuContext {
    pub async fn open(
        window: Arc<Window>,
        display: OwnedDisplayHandle,
    ) -> (Arc<Self>, wgpu::Surface<'static>) {
        // TEXT_ONLY_WGPU_API: wgpu::InstanceDescriptor::new_with_display_handle_from_env
        let mut descriptor =
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display));
        // WGPU 30 reuses an unreset acquire fence on Linux Vulkan. Keep
        // WGPU's API validation, but leave Vulkan's external layer off until
        // upstream fixes that backend path. WGPU_VALIDATION=1 remains an opt-in.
        #[cfg(target_os = "linux")]
        if std::env::var_os("WGPU_VALIDATION").is_none() {
            descriptor.flags.remove(wgpu::InstanceFlags::VALIDATION);
        }
        // TEXT_ONLY_WGPU_API: wgpu::Instance::new
        let instance = wgpu::Instance::new(descriptor);
        // TEXT_ONLY_WGPU_API: wgpu::Instance::create_surface
        let surface = instance
            .create_surface(window)
            .expect("failed to create surface");
        // TEXT_ONLY_WGPU_API: wgpu::Instance::request_adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .expect("failed to find adapter");
        // TEXT_ONLY_WGPU_API: wgpu::Adapter::request_device
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("solara_gpu_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await
            .expect("failed to create device");
        (
            Arc::new(Self {
                instance,
                adapter,
                device,
                queue,
            }),
            surface,
        )
    }

    pub fn create_surface(&self, window: Arc<Window>) -> wgpu::Surface<'static> {
        // TEXT_ONLY_WGPU_API: wgpu::Instance::create_surface
        self.instance
            .create_surface(window)
            .expect("failed to create surface")
    }

    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}

/// A configured presentation target owned by one native window.
pub struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
}

impl WindowSurface {
    pub fn new(
        surface: wgpu::Surface<'static>,
        context: &GpuContext,
        width: u32,
        height: u32,
    ) -> Self {
        // TEXT_ONLY_WGPU_API: wgpu::Surface::get_capabilities
        let caps = surface.get_capabilities(context.adapter());
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        // TEXT_ONLY_WGPU_API: wgpu::Surface::configure
        surface.configure(context.device(), &config);
        Self { surface, config }
    }

    pub fn configuration(&self) -> &wgpu::SurfaceConfiguration {
        &self.config
    }

    pub fn raw(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn resize(&mut self, context: &GpuContext, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        // TEXT_ONLY_WGPU_API: wgpu::Surface::configure
        self.surface.configure(context.device(), &self.config);
    }

    pub fn acquire(&self) -> Result<Option<SurfaceFrame>, RenderError> {
        // TEXT_ONLY_WGPU_API: wgpu::Surface::get_current_texture
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };
        // TEXT_ONLY_WGPU_API: wgpu::Texture::create_view
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Some(SurfaceFrame { output, view }))
    }
}

/// One acquired swapchain image and its renderable view.
pub struct SurfaceFrame {
    output: wgpu::SurfaceTexture,
    view: wgpu::TextureView,
}

impl SurfaceFrame {
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.output.texture
    }

    pub fn present(self, queue: &wgpu::Queue) {
        // TEXT_ONLY_WGPU_API: wgpu::Queue::present
        queue.present(self.output);
    }
}

/// Encodes Solara shapes and glyphs into any compatible texture view.
pub struct GpuPainter {
    #[cfg(not(feature = "text-only"))]
    screen_buffer: wgpu::Buffer,
    #[cfg(not(feature = "text-only"))]
    shape_pipeline: wgpu::RenderPipeline,
    #[cfg(not(feature = "text-only"))]
    shape_bind_group: wgpu::BindGroup,
    text_brush: TextBrush,
    #[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
    view_width: u32,
}

impl GpuPainter {
    pub fn new(context: &GpuContext, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        #[cfg(not(feature = "text-only"))]
        let screen_uniform = ScreenUniform {
            size: [width as f32, height as f32],
            _pad: [0.0, 0.0],
        };
        #[cfg(not(feature = "text-only"))]
        let screen_buffer =
            context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("solara_screen_uniform"),
                    contents: as_bytes(&screen_uniform),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
        #[cfg(not(feature = "text-only"))]
        let layout = context
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("solara_screen_bind_group_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        #[cfg(not(feature = "text-only"))]
        let shape_bind_group = context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("solara_shape_screen_bind_group"),
                layout: &layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: screen_buffer.as_entire_binding(),
                }],
            });
        #[cfg(not(feature = "text-only"))]
        let shader = context
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("solara_shape_shader"),
                source: wgpu::ShaderSource::Wgsl(SHAPE_SHADER.into()),
            });
        #[cfg(not(feature = "text-only"))]
        let pipeline_layout =
            context
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("solara_shape_pipeline_layout"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                });
        #[cfg(not(feature = "text-only"))]
        let shape_pipeline =
            context
                .device()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("solara_shape_pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<GpuShape>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Uint32],
                        })],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                });
        // TEXT_ONLY_WGPU_API: wgpu_text::BrushBuilder::build
        let text_brush =
            BrushBuilder::using_font(font().clone()).build(context.device(), width, height, format);
        Self {
            #[cfg(not(feature = "text-only"))]
            screen_buffer,
            #[cfg(not(feature = "text-only"))]
            shape_pipeline,
            #[cfg(not(feature = "text-only"))]
            shape_bind_group,
            text_brush,
            #[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
            view_width: width,
        }
    }

    pub fn resize(&mut self, context: &GpuContext, width: u32, height: u32) {
        #[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
        {
            self.view_width = width;
        }
        #[cfg(not(feature = "text-only"))]
        let uniform = ScreenUniform {
            size: [width as f32, height as f32],
            _pad: [0.0, 0.0],
        };
        #[cfg(not(feature = "text-only"))]
        context
            .queue()
            .write_buffer(&self.screen_buffer, 0, as_bytes(&uniform));
        // TEXT_ONLY_WGPU_API: wgpu_text::TextBrush::resize_view
        self.text_brush
            .resize_view(width as f32, height as f32, context.queue());
    }

    fn queue_text<T: TextRun>(&mut self, context: &GpuContext, text: &[T]) {
        let sections = text
            .iter()
            .map(|run| Section {
                screen_position: run.position(),
                bounds: run.bounds(),
                text: vec![
                    Text::new(run.text())
                        .with_color(run.color())
                        .with_scale(run.scale()),
                ],
                ..Section::default()
            })
            .collect::<Vec<_>>();
        // TEXT_ONLY_WGPU_API: wgpu_text::TextBrush::queue
        self.text_brush
            .queue(context.device(), context.queue(), &sections)
            .expect("glyph queue failed");
    }

    #[cfg(not(feature = "text-only"))]
    pub fn encode<S: Shape, T: TextRun>(
        &mut self,
        context: &GpuContext,
        view: &wgpu::TextureView,
        shapes: &[S],
        text: &[T],
    ) -> wgpu::CommandBuffer {
        self.encode_inner(context, view, shapes, text, None)
    }

    #[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
    pub fn encode_with_visual_debug<S: Shape, T: TextRun>(
        &mut self,
        context: &GpuContext,
        view: &wgpu::TextureView,
        shapes: &[S],
        text: &[T],
        visual_debug: &VisualDebug,
    ) -> wgpu::CommandBuffer {
        self.encode_inner(context, view, shapes, text, Some(visual_debug))
    }

    #[cfg(not(feature = "text-only"))]
    fn encode_inner<S: Shape, T: TextRun>(
        &mut self,
        context: &GpuContext,
        view: &wgpu::TextureView,
        shapes: &[S],
        text: &[T],
        #[cfg(feature = "visual-debug")] visual_debug: Option<&VisualDebug>,
        #[cfg(not(feature = "visual-debug"))] _visual_debug: Option<&()>,
    ) -> wgpu::CommandBuffer {
        self.queue_text(context, text);

        let gpu_shapes = shapes
            .iter()
            .map(|shape| GpuShape {
                pos_size: shape.position_size(),
                color: shape.color(),
                shape_type: shape.kind(),
                _pad: 0,
            })
            .collect::<Vec<_>>();
        #[cfg(feature = "visual-debug")]
        let gpu_shapes = match visual_debug {
            Some(visual_debug) => {
                let mut gpu_shapes = gpu_shapes;
                gpu_shapes.extend(visual_debug.overlay(self.view_width));
                gpu_shapes
            }
            None => gpu_shapes,
        };
        let instance_buffer = (!gpu_shapes.is_empty()).then(|| {
            context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("solara_shape_instances"),
                    contents: cast_slice(&gpu_shapes),
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        let mut encoder =
            context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("solara_gpu_encoder"),
                });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("solara_gpu_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.98,
                            g: 0.98,
                            b: 0.98,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(instance_buffer) = &instance_buffer {
                pass.set_pipeline(&self.shape_pipeline);
                pass.set_bind_group(0, &self.shape_bind_group, &[]);
                pass.set_vertex_buffer(0, instance_buffer.slice(..));
                pass.draw(0..6, 0..gpu_shapes.len() as u32);
            }
            self.text_brush.draw(&mut pass);
        }
        encoder.finish()
    }

    /// Encode only text. With `text-only`, this is the shim's complete paint API.
    #[cfg(feature = "text-only")]
    pub fn encode_text<T: TextRun>(
        &mut self,
        context: &GpuContext,
        view: &wgpu::TextureView,
        text: &[T],
    ) -> wgpu::CommandBuffer {
        self.queue_text(context, text);
        // TEXT_ONLY_WGPU_API: wgpu::Device::create_command_encoder
        let mut encoder =
            context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some(text_only::TAG),
                });
        {
            // TEXT_ONLY_WGPU_API: wgpu::CommandEncoder::begin_render_pass
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(text_only::TAG),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.98,
                            g: 0.98,
                            b: 0.98,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // TEXT_ONLY_WGPU_API: wgpu_text::TextBrush::draw
            self.text_brush.draw(&mut pass);
        }
        // TEXT_ONLY_WGPU_API: wgpu::CommandEncoder::finish
        encoder.finish()
    }
}

/// Convenience composition used by Solara's window event loop.
pub struct Renderer {
    surface: WindowSurface,
    context: Arc<GpuContext>,
    painter: GpuPainter,
    #[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
    visual_debug: VisualDebug,
}

pub type RendererContext = GpuContext;

#[derive(Clone, Copy, Debug)]
pub enum RenderError {
    Lost,
    Outdated,
    Validation,
}

impl Renderer {
    pub async fn new(
        window: Arc<Window>,
        display: OwnedDisplayHandle,
        shared: Option<Arc<GpuContext>>,
    ) -> Self {
        let size = window.inner_size();
        let (context, raw_surface) = match shared {
            Some(context) => {
                let surface = context.create_surface(window);
                (context, surface)
            }
            None => GpuContext::open(window, display).await,
        };
        let surface = WindowSurface::new(raw_surface, &context, size.width, size.height);
        let painter = GpuPainter::new(
            &context,
            surface.configuration().width,
            surface.configuration().height,
            surface.format(),
        );
        Self {
            surface,
            context,
            painter,
            #[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
            visual_debug: {
                let mut debug = VisualDebug::default();
                debug.indicate(VisualDebugEvent::SurfaceConfigure);
                debug
            },
        }
    }

    pub fn context(&self) -> Arc<GpuContext> {
        Arc::clone(&self.context)
    }

    pub fn surface(&self) -> &WindowSurface {
        &self.surface
    }

    pub fn surface_mut(&mut self) -> &mut WindowSurface {
        &mut self.surface
    }

    pub fn painter(&self) -> &GpuPainter {
        &self.painter
    }

    pub fn painter_mut(&mut self) -> &mut GpuPainter {
        &mut self.painter
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface.resize(&self.context, width, height);
        self.painter.resize(&self.context, width, height);
        #[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
        self.visual_debug
            .indicate(VisualDebugEvent::SurfaceConfigure);
    }

    #[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
    pub fn indicate_visual_debug(&mut self, event: VisualDebugEvent) {
        self.visual_debug.indicate(event);
    }

    #[cfg(not(feature = "text-only"))]
    pub fn render<S: Shape, T: TextRun>(
        &mut self,
        shapes: &[S],
        text: &[T],
    ) -> Result<(), RenderError> {
        let Some(frame) = self.surface.acquire()? else {
            return Ok(());
        };
        #[cfg(feature = "visual-debug")]
        {
            self.visual_debug.indicate(VisualDebugEvent::FrameAcquire);
            if !shapes.is_empty() {
                self.visual_debug.indicate(VisualDebugEvent::ShapeUpload);
            }
            if !text.is_empty() {
                self.visual_debug.indicate(VisualDebugEvent::GlyphUpload);
            }
            self.visual_debug.indicate(VisualDebugEvent::SubmitPresent);
        }
        let commands = self.painter.encode_inner(
            &self.context,
            frame.view(),
            shapes,
            text,
            #[cfg(feature = "visual-debug")]
            Some(&self.visual_debug),
            #[cfg(not(feature = "visual-debug"))]
            None,
        );
        self.context.queue().submit([commands]);
        frame.present(self.context.queue());
        #[cfg(feature = "visual-debug")]
        self.visual_debug.clear();
        Ok(())
    }

    /// Acquire, draw only the supplied text runs, submit, and present.
    #[cfg(feature = "text-only")]
    pub fn render_text<T: TextRun>(&mut self, text: &[T]) -> Result<(), RenderError> {
        let Some(frame) = self.surface.acquire()? else {
            return Ok(());
        };
        let commands = self.painter.encode_text(&self.context, frame.view(), text);
        // TEXT_ONLY_WGPU_API: wgpu::Queue::submit
        self.context.queue().submit([commands]);
        frame.present(self.context.queue());
        Ok(())
    }
}

static FONT: OnceLock<FontArc> = OnceLock::new();

/// Metrics for the bundled font at one CSS `font-size`.
///
/// CSS sizes are pixels per em. `wgpu_text` uses `ab_glyph`'s pixel-height
/// scale, so [`glyph_scale`](Self::glyph_scale) is deliberately distinct from
/// [`font_size`](Self::font_size).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub font_size: f32,
    pub glyph_scale: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub advance_width: f32,
}

impl FontMetrics {
    pub fn natural_line_height(self) -> f32 {
        self.ascent + self.descent + self.line_gap
    }
}

fn font() -> &'static FontArc {
    FONT.get_or_init(|| {
        FontArc::try_from_slice(include_bytes!("../fonts/Inconsolata-Regular.ttf"))
            .expect("invalid bundled font")
    })
}

pub fn font_metrics(font_size: f32) -> FontMetrics {
    let font = font();
    let font_size = if font_size.is_finite() {
        font_size.max(0.0)
    } else {
        0.0
    };
    let em = font.units_per_em().unwrap_or(1000.0);
    let glyph_scale = font_size * font.height_unscaled() / em;
    let scaled = font.as_scaled(glyph_scale);
    FontMetrics {
        font_size,
        glyph_scale,
        ascent: scaled.ascent(),
        descent: -scaled.descent(),
        line_gap: scaled.line_gap(),
        advance_width: scaled.h_advance(scaled.glyph_id('n')),
    }
}

pub fn char_width(font_size: f32) -> f32 {
    font_metrics(font_size).advance_width
}

#[cfg(not(feature = "text-only"))]
fn as_bytes<T: Copy>(value: &T) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts((value as *const T).cast::<u8>(), std::mem::size_of::<T>())
    }
}

#[cfg(not(feature = "text-only"))]
fn cast_slice<T: Copy>(values: &[T]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

#[cfg(test)]
mod tests {
    use super::{char_width, font_metrics};

    #[cfg(not(feature = "text-only"))]
    use super::GpuShape;

    #[test]
    fn bundled_font_exposes_stable_positive_metrics() {
        let small = font_metrics(14.0);
        let large = font_metrics(28.0);

        assert!(small.advance_width.is_finite());
        assert!(small.advance_width > 0.0);
        assert!(small.ascent > 0.0);
        assert!(small.descent >= 0.0);
        assert!(small.natural_line_height() > 0.0);
        assert!((large.glyph_scale - small.glyph_scale * 2.0).abs() < 0.001);
        assert!((char_width(28.0) - char_width(14.0) * 2.0).abs() < 0.001);
        assert_ne!(small.font_size, small.glyph_scale);
    }

    #[cfg(not(feature = "text-only"))]
    #[test]
    fn internal_shape_layout_matches_the_wgsl_instance_contract() {
        assert_eq!(std::mem::size_of::<GpuShape>(), 40);
    }

    #[cfg(all(feature = "visual-debug", not(feature = "text-only")))]
    #[test]
    fn visual_debug_rail_distinguishes_active_events() {
        use super::{VisualDebug, VisualDebugEvent};

        let mut debug = VisualDebug::default();
        debug.indicate(VisualDebugEvent::FrameAcquire);
        debug.indicate(VisualDebugEvent::GlyphUpload);
        let overlay = debug.overlay(960);

        assert_eq!(overlay.len(), 6);
        assert_eq!(overlay[1].pos_size[3], 4.0);
        assert_eq!(overlay[2].pos_size[3], 14.0);
        assert_eq!(overlay[4].pos_size[3], 14.0);
        assert_eq!(overlay[5].pos_size[3], 4.0);
    }
}
