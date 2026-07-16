use std::collections::HashMap;
use std::sync::Arc;

use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::gpu_ui::async_utils::block_on;
use crate::gpu_ui::html::{Document, RenderBatch};
use crate::gpu_ui::loader::{LoadedPage, load_page};
use crate::gpu_ui::renderer::{RenderError, Renderer, RendererContext};

const WINDOW_WIDTH: u32 = 960;
const WINDOW_HEIGHT: u32 = 720;
const PREVIEW_HTML_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/preview.html");

pub fn run(source: Option<String>) -> Result<(), String> {
    let initial_pages = load_initial_pages(source.as_deref())?;
    for page in &initial_pages {
        println!("Loading {} ({})", page.url, page.label);
    }

    let event_loop =
        EventLoop::new().map_err(|error| format!("failed to create event loop: {error}"))?;
    let mut app = GpuUiApp::new(initial_pages);
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("event loop failed: {error}"))
}

struct InitialPage {
    label: &'static str,
    url: Url,
    title: String,
    favicon_url: Option<Url>,
    document: Document,
}

fn load_initial_pages(primary_source: Option<&str>) -> Result<Vec<InitialPage>, String> {
    Ok(vec![
        load_initial_page(primary_source, "bundled demo")?,
        load_initial_page(Some(PREVIEW_HTML_PATH), "preview")?,
    ])
}

fn load_initial_page(source: Option<&str>, label: &'static str) -> Result<InitialPage, String> {
    let LoadedPage {
        url,
        favicon_url,
        title,
        artifact,
        dom_engine,
    } = load_page(source)?;
    let document = Document::from_dom(artifact, dom_engine, WINDOW_WIDTH as f32)?;
    Ok(InitialPage {
        label,
        url,
        title,
        favicon_url,
        document,
    })
}

struct GpuUiApp {
    initial_pages: Vec<InitialPage>,
    windows: HashMap<WindowId, PageWindow>,
}

impl GpuUiApp {
    fn new(initial_pages: Vec<InitialPage>) -> Self {
        Self {
            initial_pages,
            windows: HashMap::new(),
        }
    }
}

struct PageWindow {
    window: Arc<Window>,
    renderer: Renderer,
    document: Document,
    batch: RenderBatch,
    viewport_height: f32,
    scale_factor: f32,
    cursor: (f32, f32),
    _favicon_url: Option<Url>,
}

impl PageWindow {
    fn create(
        event_loop: &ActiveEventLoop,
        mut page: InitialPage,
        index: usize,
        renderer_context: Option<Arc<RendererContext>>,
    ) -> Self {
        let window_title = format!("{} — {} - Solara", page.title, page.label);
        let offset = 48.0 + index as f64 * 72.0;
        let window_attributes = Window::default_attributes()
            .with_title(window_title)
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .with_position(LogicalPosition::new(offset, offset));
        let window = Arc::new(
            event_loop
                .create_window(window_attributes)
                .expect("failed to create window"),
        );
        let renderer = block_on(Renderer::new(
            window.clone(),
            event_loop.owned_display_handle(),
            renderer_context,
        ));
        let scale_factor = window.scale_factor() as f32;
        let (width, viewport_height) = logical_size(&window);
        page.document.relayout(width);

        Self {
            window,
            renderer,
            document: page.document,
            batch: RenderBatch::default(),
            viewport_height,
            scale_factor,
            cursor: (0.0, 0.0),
            _favicon_url: page.favicon_url,
        }
    }

    fn sync_layout(&mut self) {
        self.scale_factor = self.window.scale_factor() as f32;
        let (width, height) = logical_size(&self.window);
        self.viewport_height = height;
        self.document.relayout(width);
    }

    fn rebuild(&mut self) {
        self.document.clamp_scroll_to(self.viewport_height);
        crate::gpu_ui::html::collect_batch(&self.document, self.scale_factor, &mut self.batch);
    }

    fn render(&mut self) {
        self.rebuild();
        match self
            .renderer
            .render(&self.batch.shapes, &self.batch.text.sections)
        {
            Ok(()) => {}
            Err(RenderError::Lost | RenderError::Outdated) => {
                let size = self.window.inner_size();
                self.renderer.resize(size.width, size.height);
            }
            Err(RenderError::Validation) => eprintln!("surface validation error"),
        }
    }

    fn handle_event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::Resized(size) => {
                self.renderer.resize(size.width, size.height);
                self.sync_layout();
                self.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let size = self.window.inner_size();
                self.renderer.resize(size.width, size.height);
                self.sync_layout();
                self.window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let logical = position.to_logical::<f32>(self.window.scale_factor());
                self.cursor = (logical.x, logical.y);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 24.0,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / self.scale_factor,
                };
                self.document.scroll_by(scroll);
                self.window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.document
                    .toggle_details_at(self.cursor.0, self.cursor.1);
                self.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn logical_size(window: &Window) -> (f32, f32) {
    let size = window.inner_size().to_logical::<f32>(window.scale_factor());
    (size.width, size.height)
}

impl ApplicationHandler for GpuUiApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.windows.is_empty() {
            return;
        }

        let mut renderer_context = None;
        for (index, page) in std::mem::take(&mut self.initial_pages)
            .into_iter()
            .enumerate()
        {
            let page_window = PageWindow::create(event_loop, page, index, renderer_context.take());
            renderer_context = Some(page_window.renderer.context());
            let window_id = page_window.window.id();
            page_window.window.request_redraw();
            self.windows.insert(window_id, page_window);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested) {
            self.windows.remove(&window_id);
            if self.windows.is_empty() {
                event_loop.exit();
            }
            return;
        }

        if let Some(page_window) = self.windows.get_mut(&window_id) {
            page_window.handle_event(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PREVIEW_HTML_PATH, load_initial_pages};

    #[test]
    fn default_demo_session_preloads_two_independent_documents() {
        let pages = load_initial_pages(None).expect("both demo pages load");
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].label, "bundled demo");
        assert_eq!(pages[1].label, "preview");
        assert_ne!(pages[0].url, pages[1].url);
        assert!(pages[1].url.path().ends_with("/preview.html"));
        assert!(std::path::Path::new(PREVIEW_HTML_PATH).is_file());
    }
}
