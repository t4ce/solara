use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::gpu_ui::async_utils::block_on;
use crate::gpu_ui::html::{Document, RenderBatch};
use crate::gpu_ui::loader::{LoadedPage, load_page};
use crate::gpu_ui::renderer::Renderer;

const WINDOW_WIDTH: u32 = 960;
const WINDOW_HEIGHT: u32 = 720;

pub fn run(source: Option<String>) -> Result<(), String> {
    let page = load_page(source.as_deref())?;
    println!("Loading {}", page.url);
    let event_loop =
        EventLoop::new().map_err(|error| format!("failed to create event loop: {error}"))?;
    let mut app = GpuUiApp::new(page);
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("event loop failed: {error}"))
}

struct GpuUiApp {
    initial_page: Option<LoadedPage>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    document: Option<Document>,
    batch: RenderBatch,
    viewport_height: f32,
    scale_factor: f32,
    cursor: (f32, f32),
}

impl GpuUiApp {
    fn new(page: LoadedPage) -> Self {
        Self {
            initial_page: Some(page),
            window: None,
            renderer: None,
            document: None,
            batch: RenderBatch::default(),
            viewport_height: 0.0,
            scale_factor: 1.0,
            cursor: (0.0, 0.0),
        }
    }

    fn logical_size(window: &Window) -> (f32, f32) {
        let scale = window.scale_factor();
        let size = window.inner_size().to_logical::<f32>(scale);
        (size.width, size.height)
    }

    fn sync_layout(&mut self) {
        let Some(window) = &self.window else {
            return;
        };

        self.scale_factor = window.scale_factor() as f32;
        let (width, height) = Self::logical_size(window);
        self.viewport_height = height;
        if let Some(document) = self.document.as_mut() {
            document.relayout(width);
        }
    }

    fn rebuild(&mut self) {
        let Some(document) = self.document.as_mut() else {
            return;
        };
        document.clamp_scroll_to(self.viewport_height);
        crate::gpu_ui::html::collect_batch(document, self.scale_factor, &mut self.batch);
    }

    fn render(&mut self) {
        self.rebuild();
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        match renderer.render(&self.batch.shapes, &self.batch.text) {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost) => {
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    renderer.resize(size.width, size.height);
                }
            }
            Err(wgpu::SurfaceError::OutOfMemory) => panic!("wgpu ran out of memory"),
            Err(err) => eprintln!("surface error: {err:?}"),
        }
    }
}

impl ApplicationHandler for GpuUiApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let page = self
            .initial_page
            .take()
            .expect("initial page must be available before creating the window");
        let window_title = format!("{} - Solara", page.title);
        let window_attributes = Window::default_attributes()
            .with_title(window_title)
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));

        let window = Arc::new(
            event_loop
                .create_window(window_attributes)
                .expect("failed to create window"),
        );

        let renderer = block_on(Renderer::new(window.clone()));
        self.scale_factor = window.scale_factor() as f32;
        let (width, height) = Self::logical_size(&window);
        self.viewport_height = height;
        self.document = Some(Document::from_html(&page.html, &page.css, width));
        self.window = Some(window);
        self.renderer = Some(renderer);

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };

        if window_id != window.id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
                self.sync_layout();
                window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(renderer) = self.renderer.as_mut() {
                    let size = window.inner_size();
                    renderer.resize(size.width, size.height);
                }
                self.sync_layout();
                window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = window.scale_factor();
                let logical = position.to_logical::<f32>(scale);
                self.cursor = (logical.x, logical.y);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(document) = self.document.as_mut() {
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y * 24.0,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 / self.scale_factor,
                    };
                    document.scroll_by(scroll);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(document) = self.document.as_mut() {
                    document.toggle_details_at(self.cursor.0, self.cursor.1);
                }
                window.request_redraw();
            }
            _ => {}
        }
    }
}
