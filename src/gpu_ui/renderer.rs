use std::sync::Arc;

use wgpu::util::DeviceExt;
use wgpu_text::{
    BrushBuilder, TextBrush,
    glyph_brush::{Section, Text},
};
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

use crate::gpu_ui::shapes::{ScreenUniform, ShapeInstance, as_bytes, cast_slice};
use crate::gpu_ui::text::{self, TextBatch};

const SHAPE_SHADER: &str = include_str!("shaders/shape.wgsl");

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    context: Arc<RendererContext>,
    config: wgpu::SurfaceConfiguration,
    screen_buffer: wgpu::Buffer,
    shape_pipeline: wgpu::RenderPipeline,
    shape_bind_group: wgpu::BindGroup,
    text_brush: TextBrush,
}

/// GPU objects shared by every Solara window. A single WebGPU device/queue
/// keeps submissions and presentation synchronization coherent across surfaces.
pub struct RendererContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

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
        shared: Option<Arc<RendererContext>>,
    ) -> Self {
        let size = window.inner_size();
        let (surface, context) = match shared {
            Some(context) => {
                let surface = context
                    .instance
                    .create_surface(window.clone())
                    .expect("failed to create surface");
                (surface, context)
            }
            None => {
                let instance_descriptor =
                    wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(display));
                let instance = wgpu::Instance::new(instance_descriptor);
                let surface = instance
                    .create_surface(window.clone())
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
                        label: Some("gpu_ui_device"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::default(),
                        memory_hints: wgpu::MemoryHints::Performance,
                        ..Default::default()
                    })
                    .await
                    .expect("failed to create device");
                (
                    surface,
                    Arc::new(RendererContext {
                        instance,
                        adapter,
                        device,
                        queue,
                    }),
                )
            }
        };

        let caps = surface.get_capabilities(&context.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&context.device, &config);

        let screen_uniform = ScreenUniform {
            size: [config.width as f32, config.height as f32],
            _pad: [0.0, 0.0],
        };
        let screen_buffer = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("screen_uniform"),
                contents: as_bytes(&screen_uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let screen_bind_group_layout =
            context
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("screen_bind_group_layout"),
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
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shape_screen_bind_group"),
                layout: &screen_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: screen_buffer.as_entire_binding(),
                }],
            });

        let shape_shader = context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("shape_shader"),
                source: wgpu::ShaderSource::Wgsl(SHAPE_SHADER.into()),
            });

        let shape_pipeline_layout =
            context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("shape_pipeline_layout"),
                    bind_group_layouts: &[Some(&screen_bind_group_layout)],
                    immediate_size: 0,
                });

        let shape_pipeline = context
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shape_pipeline"),
            layout: Some(&shape_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shape_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ShapeInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Uint32],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shape_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
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

        let text_brush = BrushBuilder::using_font(text::font().clone()).build(
            &context.device,
            config.width,
            config.height,
            format,
        );

        let renderer = Self {
            surface,
            context,
            config,
            screen_buffer,
            shape_pipeline,
            shape_bind_group,
            text_brush,
        };
        renderer.update_screen_uniform(renderer.config.width, renderer.config.height);
        renderer
    }

    pub fn context(&self) -> Arc<RendererContext> {
        Arc::clone(&self.context)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.context.device, &self.config);
            self.update_screen_uniform(width, height);
            self.text_brush
                .resize_view(width as f32, height as f32, &self.context.queue);
        }
    }

    fn update_screen_uniform(&self, width: u32, height: u32) {
        let uniform = ScreenUniform {
            size: [width as f32, height as f32],
            _pad: [0.0, 0.0],
        };
        self.context
            .queue
            .write_buffer(&self.screen_buffer, 0, as_bytes(&uniform));
    }

    fn queue_text(&mut self, text: &TextBatch) {
        let sections = text
            .sections
            .iter()
            .map(|section| Section {
                screen_position: (section.x, section.y),
                bounds: (section.width, section.height),
                text: vec![
                    Text::new(&section.text)
                        .with_color(section.color)
                        .with_scale(section.scale),
                ],
                ..Section::default()
            })
            .collect::<Vec<_>>();
        self.text_brush
            .queue(&self.context.device, &self.context.queue, &sections)
            .expect("glyph queue failed");
    }

    pub fn render(
        &mut self,
        shapes: &[ShapeInstance],
        text: &TextBatch,
    ) -> Result<(), RenderError> {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.queue_text(text);

        let mut encoder =
            self.context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gpu_ui_encoder"),
                });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gpu_ui_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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

            if !shapes.is_empty() {
                let instance_buffer =
                    self.context
                        .device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("shape_instances"),
                            contents: cast_slice(shapes),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                pass.set_pipeline(&self.shape_pipeline);
                pass.set_bind_group(0, &self.shape_bind_group, &[]);
                pass.set_vertex_buffer(0, instance_buffer.slice(..));
                pass.draw(0..6, 0..shapes.len() as u32);
            }

            self.text_brush.draw(&mut pass);
        }
        self.context.queue.submit(std::iter::once(encoder.finish()));
        self.context.queue.present(output);
        Ok(())
    }
}
