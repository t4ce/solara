use std::sync::Arc;

use softbuffer::Context;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::gpu_ui::layout::{Button, FlexRow};
use crate::gpu_ui::renderer::{DemoCircle, Renderer};
use crate::gpu_ui::shapes::circle_contains;

const WINDOW_WIDTH: u32 = 960;
const WINDOW_HEIGHT: u32 = 540;

pub fn run() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let context = Context::new(event_loop.owned_display_handle()).expect("failed to create context");
    let mut app = GpuUiApp::new(context);
    event_loop.run_app(&mut app).expect("event loop failed");
}

struct GpuUiApp<D> {
    context: Context<D>,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer<D, Arc<Window>>>,
    buttons: Vec<Button>,
    demo_circle: DemoCircle,
    layout_dirty: bool,
    cursor: (f32, f32),
}

impl<D> GpuUiApp<D> {
    fn new(context: Context<D>) -> Self {
        Self {
            context,
            window: None,
            renderer: None,
            buttons: Vec::new(),
            demo_circle: DemoCircle {
                center_x: 820.0,
                center_y: 120.0,
                diameter: 96.0,
                color: [0.95, 0.55, 0.15, 1.0],
            },
            layout_dirty: true,
            cursor: (0.0, 0.0),
        }
    }

    fn init_buttons(&mut self) {
        self.buttons = vec![
            Button::new("Click Me", [0.18, 0.42, 0.86, 1.0]),
            Button::new("Click Me", [0.16, 0.62, 0.38, 1.0]),
            Button::new("Click Me", [0.72, 0.24, 0.58, 1.0]),
        ];
        self.layout_dirty = true;
    }

    fn relayout(&mut self) {
        let row = FlexRow::new((48.0, 220.0), 16.0);
        row.layout(&mut self.buttons);
        self.layout_dirty = false;
    }

    fn handle_click(&mut self, x: f32, y: f32) {
        for button in &mut self.buttons {
            if button.hit_test(x, y) {
                button.fill = [
                    (button.fill[0] + 0.25).min(1.0),
                    (button.fill[1] * 0.75).max(0.0),
                    (button.fill[2] + 0.1).min(1.0),
                    1.0,
                ];
            }
        }

        if circle_contains(
            self.demo_circle.center_x,
            self.demo_circle.center_y,
            self.demo_circle.diameter,
            x,
            y,
        ) {
            self.demo_circle.color = [
                rand_channel(self.demo_circle.color[0]),
                rand_channel(self.demo_circle.color[1]),
                rand_channel(self.demo_circle.color[2]),
                1.0,
            ];
        }
    }

    fn render(&mut self) {
        if self.layout_dirty {
            self.relayout();
        }

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        if let Err(err) = renderer.render(&self.buttons, &self.demo_circle) {
            eprintln!("render error: {err:?}");
        }
    }
}

fn rand_channel(value: f32) -> f32 {
    let next = value + 0.31;
    if next > 1.0 { next - 0.82 } else { next }
}

impl<D> ApplicationHandler for GpuUiApp<D>
where
    D: softbuffer::HasDisplayHandle + Clone + 'static,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title("Solara Softbuffer UI")
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));

        let window = Arc::new(
            event_loop
                .create_window(window_attributes)
                .expect("failed to create window"),
        );

        let renderer = Renderer::new(&self.context, window.clone(), WINDOW_WIDTH, WINDOW_HEIGHT);
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.init_buttons();
        self.relayout();

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
                self.layout_dirty = true;
                window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
            }
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.cursor;
                self.handle_click(x, y);
                window.request_redraw();
            }
            _ => {}
        }
    }
}
