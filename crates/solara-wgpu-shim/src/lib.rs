//! Complete WGPU boundary for Solara.
//!
//! The upstream crates are re-exported for escape-hatch access, while the
//! composed API separates shared GPU ownership, per-window presentation, and
//! paint command encoding.

use std::sync::{Arc, OnceLock};

pub use wgpu;
pub use wgpu_text;

use wgpu::util::DeviceExt;
use wgpu_text::{
    BrushBuilder, TextBrush,
    glyph_brush::{
        Section, Text,
        ab_glyph::{Font, FontArc},
    },
};
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

const SHAPE_SHADER: &str = include_str!("shape.wgsl");

/// Renderer-neutral shape record accepted by [`GpuPainter`].
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
struct ScreenUniform {
    size: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GpuShape {
    pos_size: [f32; 4],
    color: [f32; 4],
    shape_type: u32,
    _pad: u32,
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
        let mut descriptor =
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display));
        // WGPU 30 reuses an unreset acquire fence on Linux Vulkan. Keep
        // WGPU's API validation, but leave Vulkan's external layer off until
        // upstream fixes that backend path. WGPU_VALIDATION=1 remains an opt-in.
        #[cfg(target_os = "linux")]
        if std::env::var_os("WGPU_VALIDATION").is_none() {
            descriptor.flags.remove(wgpu::InstanceFlags::VALIDATION);
        }
        let instance = wgpu::Instance::new(descriptor);
        let surface = instance
            .create_surface(window)
            .expect("failed to create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .expect("failed to find adapter");
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
        self.surface.configure(context.device(), &self.config);
    }

    pub fn acquire(&self) -> Result<Option<SurfaceFrame>, RenderError> {
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
        queue.present(self.output);
    }
}

/// Encodes Solara shapes and glyphs into any compatible texture view.
pub struct GpuPainter {
    screen_buffer: wgpu::Buffer,
    shape_pipeline: wgpu::RenderPipeline,
    shape_bind_group: wgpu::BindGroup,
    text_brush: TextBrush,
}

impl GpuPainter {
    pub fn new(context: &GpuContext, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        let screen_uniform = ScreenUniform {
            size: [width as f32, height as f32],
            _pad: [0.0, 0.0],
        };
        let screen_buffer =
            context
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("solara_screen_uniform"),
                    contents: as_bytes(&screen_uniform),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
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
        let shader = context
            .device()
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("solara_shape_shader"),
                source: wgpu::ShaderSource::Wgsl(SHAPE_SHADER.into()),
            });
        let pipeline_layout =
            context
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("solara_shape_pipeline_layout"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                });
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
        let text_brush =
            BrushBuilder::using_font(font().clone()).build(context.device(), width, height, format);
        Self {
            screen_buffer,
            shape_pipeline,
            shape_bind_group,
            text_brush,
        }
    }

    pub fn resize(&mut self, context: &GpuContext, width: u32, height: u32) {
        let uniform = ScreenUniform {
            size: [width as f32, height as f32],
            _pad: [0.0, 0.0],
        };
        context
            .queue()
            .write_buffer(&self.screen_buffer, 0, as_bytes(&uniform));
        self.text_brush
            .resize_view(width as f32, height as f32, context.queue());
    }

    pub fn encode<S: Shape, T: TextRun>(
        &mut self,
        context: &GpuContext,
        view: &wgpu::TextureView,
        shapes: &[S],
        text: &[T],
    ) -> wgpu::CommandBuffer {
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
        self.text_brush
            .queue(context.device(), context.queue(), &sections)
            .expect("glyph queue failed");

        let gpu_shapes = shapes
            .iter()
            .map(|shape| GpuShape {
                pos_size: shape.position_size(),
                color: shape.color(),
                shape_type: shape.kind(),
                _pad: 0,
            })
            .collect::<Vec<_>>();
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
}

/// Convenience composition used by Solara's window event loop.
pub struct Renderer {
    surface: WindowSurface,
    context: Arc<GpuContext>,
    painter: GpuPainter,
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
    }

    pub fn render<S: Shape, T: TextRun>(
        &mut self,
        shapes: &[S],
        text: &[T],
    ) -> Result<(), RenderError> {
        let Some(frame) = self.surface.acquire()? else {
            return Ok(());
        };
        let commands = self
            .painter
            .encode(&self.context, frame.view(), shapes, text);
        self.context.queue().submit([commands]);
        frame.present(self.context.queue());
        Ok(())
    }
}

static FONT: OnceLock<FontArc> = OnceLock::new();

fn font() -> &'static FontArc {
    FONT.get_or_init(|| {
        FontArc::try_from_slice(include_bytes!("../fonts/Inconsolata-Regular.ttf"))
            .expect("invalid bundled font")
    })
}

pub fn char_width(scale: f32) -> f32 {
    let font = font();
    let em = font.units_per_em().unwrap_or(1000.0);
    font.h_advance_unscaled(font.glyph_id('n')) * scale / em
}

fn as_bytes<T: Copy>(value: &T) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts((value as *const T).cast::<u8>(), std::mem::size_of::<T>())
    }
}

fn cast_slice<T: Copy>(values: &[T]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), std::mem::size_of_val(values))
    }
}

#[cfg(test)]
mod tests {
    use super::{GpuShape, char_width};

    #[test]
    fn bundled_font_exposes_stable_positive_metrics() {
        let width = char_width(14.0);
        assert!(width.is_finite());
        assert!(width > 0.0);
    }

    #[test]
    fn internal_shape_layout_matches_the_wgsl_instance_contract() {
        assert_eq!(std::mem::size_of::<GpuShape>(), 40);
    }
}
