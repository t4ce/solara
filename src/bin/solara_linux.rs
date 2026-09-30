//! Small Linux presenter for Solara's retained Blitz/Parley paint mesh.
use std::{
    collections::HashMap,
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::Duration,
};

use base64::Engine;
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use rust_qjs_dom::DomEngine;
use softbuffer::{Context, Surface};
use solara::{
    native_paint::{PageMesh, Painter},
    spec_layout::{DocumentConfig, SpecLayout, Viewport, bundled_font_context},
};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

const MAX_HTML: u64 = 16 * 1024 * 1024;
const MAX_RESOURCE: u64 = 4 * 1024 * 1024;

fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn fetch(agent: &ureq::Agent, url: &str, limit: u64) -> Result<(u16, Vec<u8>), String> {
    let mut response = agent.get(url).call().map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let body = response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|error| error.to_string())?;
    Ok((status, body))
}

struct Resources {
    agent: ureq::Agent,
    cache: Mutex<HashMap<String, Vec<u8>>>,
}

impl NetProvider for Resources {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url.to_string();
        if let Some((metadata, encoded)) = url
            .strip_prefix("data:")
            .and_then(|value| value.split_once(','))
        {
            let bytes = if metadata.ends_with(";base64") {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            handler.bytes(url, Bytes::from(bytes));
            return;
        }
        if !matches!(request.url.scheme(), "http" | "https") {
            handler.bytes(url, Bytes::new());
            return;
        }
        let bytes = match self.cache.lock() {
            Ok(mut cache) => cache
                .entry(url.clone())
                .or_insert_with(|| match fetch(&self.agent, &url, MAX_RESOURCE) {
                    Ok((status, bytes)) if (200..300).contains(&status) => bytes,
                    Ok((status, _)) => {
                        eprintln!("resource {url}: HTTP {status}");
                        Vec::new()
                    }
                    Err(error) => {
                        eprintln!("resource {url}: {error}");
                        Vec::new()
                    }
                })
                .clone(),
            Err(_) => Vec::new(),
        };
        // Blitz completes pending critical stylesheets when it receives bytes.
        handler.bytes(url, Bytes::from(bytes));
    }
}

struct App {
    url: String,
    status: u16,
    layout: SpecLayout,
    painter: Painter,
    mesh: PageMesh,
    scroll_y: f32,
    window: Option<Arc<Window>>,
    context: Option<Context<Arc<Window>>>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
}

impl App {
    fn reflow(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.layout.set_viewport(Viewport {
            window_size: (width, height),
            ..Default::default()
        })?;
        for _ in 0..16 {
            if self.layout.resolve(0.0)? {
                self.mesh = self.painter.paint(self.layout.document())?;
                self.scroll_y = self
                    .scroll_y
                    .min((self.mesh.height - height as f32).max(0.0));
                eprintln!(
                    "Solara Linux: {} boxes, {} glyphs, {} triangles, canvas {}x{}",
                    self.mesh.boxes,
                    self.mesh.glyphs,
                    self.mesh.triangles.len() / 3,
                    width,
                    height
                );
                return Ok(());
            }
        }
        Err("critical stylesheets did not settle".into())
    }

    fn redraw(&mut self) -> Result<(), String> {
        let Some(window) = self.window.as_ref() else {
            return Ok(());
        };
        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        let surface = self.surface.as_mut().ok_or("window surface missing")?;
        surface
            .resize(width, height)
            .map_err(|error| error.to_string())?;
        let mut buffer = surface.buffer_mut().map_err(|error| error.to_string())?;
        render(
            &self.mesh,
            size.width,
            size.height,
            self.scroll_y,
            &mut buffer,
        )?;
        buffer.present().map_err(|error| error.to_string())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(format!(
                "Solara Linux | HTTP {} | {}",
                self.status, self.url
            ))
            .with_inner_size(LogicalSize::new(1280.0, 800.0));
        let result = (|| -> Result<(), String> {
            let window = Arc::new(
                event_loop
                    .create_window(attributes)
                    .map_err(|error| error.to_string())?,
            );
            let context = Context::new(window.clone()).map_err(|error| error.to_string())?;
            let surface =
                Surface::new(&context, window.clone()).map_err(|error| error.to_string())?;
            let size = window.inner_size();
            self.reflow(size.width.max(1), size.height.max(1))?;
            window.request_redraw();
            self.window = Some(window);
            self.context = Some(context);
            self.surface = Some(surface);
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Solara Linux window: {error}");
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .window
            .as_ref()
            .is_none_or(|window| window.id() != window_id)
        {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                if let Err(error) = self.reflow(size.width, size.height) {
                    eprintln!("Solara Linux reflow: {error}");
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw() {
                    eprintln!("Solara Linux render: {error}");
                    event_loop.exit();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, lines) => lines * 80.0,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32,
                };
                let height = self.window.as_ref().unwrap().inner_size().height as f32;
                self.scroll_y =
                    (self.scroll_y - amount).clamp(0.0, (self.mesh.height - height).max(0.0));
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                event_loop.exit();
            }
            _ => {}
        }
    }
}

fn render(
    mesh: &PageMesh,
    width: u32,
    height: u32,
    scroll_y: f32,
    out: &mut [u32],
) -> Result<(), String> {
    let mut pixmap = Pixmap::new(width, height).ok_or("invalid window size")?;
    let [r, g, b, a] = mesh.canvas_color.to_le_bytes();
    pixmap.fill(Color::from_rgba8(r, g, b, a));
    let view = mesh.viewport(width as f32, height as f32, scroll_y);
    for (indices, color) in view.color_runs() {
        let mut path = PathBuilder::new();
        for triangle in indices.chunks_exact(3) {
            let [x0, y0] = view.vertices[triangle[0] as usize];
            let [x1, y1] = view.vertices[triangle[1] as usize];
            let [x2, y2] = view.vertices[triangle[2] as usize];
            path.move_to(x0, y0);
            path.line_to(x1, y1);
            path.line_to(x2, y2);
            path.close();
        }
        if let Some(path) = path.finish() {
            let [r, g, b, a] = color.to_le_bytes();
            let mut paint = Paint::default();
            paint.set_color_rgba8(r, g, b, a);
            paint.anti_alias = true;
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::from_translate(0.0, -scroll_y),
                None,
            );
        }
    }
    for (destination, rgba) in out.iter_mut().zip(pixmap.data().chunks_exact(4)) {
        *destination = (u32::from(rgba[0]) << 16) | (u32::from(rgba[1]) << 8) | u32::from(rgba[2]);
    }
    if let Ok(path) = std::env::var("SOLARA_LINUX_SNAPSHOT") {
        pixmap.save_png(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://chatgpt.com/".into());
    let parsed = url::Url::parse(&url).map_err(|error| error.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("expected an HTTP or HTTPS URL".into());
    }
    let agent = http_agent();
    let (status, html) = fetch(&agent, &url, MAX_HTML)?;
    eprintln!(
        "Solara Linux: GET {url} -> HTTP {status}, {} bytes",
        html.len()
    );
    let html = String::from_utf8_lossy(&html);
    let mut engine = DomEngine::new().map_err(|error| error.to_string())?;
    let artifact = engine
        .parse(&html, &url)
        .map_err(|error| error.to_string())?;
    let resources = Arc::new(Resources {
        agent,
        cache: Mutex::new(HashMap::new()),
    });
    let layout = SpecLayout::from_artifact(
        &artifact,
        DocumentConfig {
            viewport: Some(Viewport {
                window_size: (1280, 800),
                ..Default::default()
            }),
            font_ctx: Some(bundled_font_context()),
            net_provider: Some(resources),
            ..Default::default()
        },
    )?;
    let mut app = App {
        url,
        status,
        layout,
        painter: Painter::default(),
        mesh: PageMesh::default(),
        scroll_y: 0.0,
        window: None,
        context: None,
        surface: None,
    };
    let event_loop = EventLoop::new().map_err(|error| error.to_string())?;
    event_loop
        .run_app(&mut app)
        .map_err(|error| error.to_string())
}
