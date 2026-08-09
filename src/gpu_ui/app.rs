use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread;
use std::time::Duration;

use ureq::Agent;
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::gpu_ui::async_utils::block_on;
use crate::gpu_ui::html::{Document, RenderBatch};
use crate::gpu_ui::input::{MouseEventKind, MouseInput};
use crate::gpu_ui::loader::{LoadedPage, load_page};
use crate::gpu_ui::renderer::{RenderError, Renderer, RendererContext};
use crate::gpu_ui::video::{self, VideoEvent, VideoPacket};
use crate::gpu_ui::youtube::{self, YoutubeWatchBootstrap};
use crate::gpu_ui::youtube_media::{self, YoutubeMediaChoice};

const WINDOW_WIDTH: u32 = 960;
const WINDOW_HEIGHT: u32 = 720;
const VIDEO_DEMO_HTML_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/video_demo.html");
const YOUTUBE_WATCH_URL: &str = "https://www.youtube.com/watch?v=nXvnof8fTBc";
const YOUTUBE_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/126.0 Safari/537.36";

struct YoutubeStartup {
    bootstrap: YoutubeWatchBootstrap,
    choices: Vec<YoutubeMediaChoice>,
    selected: Option<usize>,
    media_path: Option<PathBuf>,
}

pub fn run(watch_url: Option<String>) -> Result<(), String> {
    let watch_url = youtube_watch_url(watch_url.as_deref())?;
    let (youtube_bootstrap, youtube_choices, selected_choice, youtube_media_path) =
        match fetch_youtube_bootstrap(&watch_url) {
            Ok(startup) => {
                println!(
                    "solara: passive YouTube bootstrap captured (video {}; {} adaptive formats)",
                    startup.bootstrap.video_id,
                    startup.bootstrap.adaptive_formats.len(),
                );
                (
                    Some(startup.bootstrap),
                    startup.choices,
                    startup.selected,
                    startup.media_path,
                )
            }
            Err(error) => {
                eprintln!(
                    "solara: passive YouTube bootstrap unavailable; continuing without playback: {error}"
                );
                (None, Vec::new(), None, None)
            }
        };
    let mut initial_pages = load_initial_pages(watch_url.as_str())?;
    let labels = if youtube_choices.is_empty() {
        vec!["No MP4 formats available".to_owned()]
    } else {
        youtube_choices
            .iter()
            .map(|choice| choice.label.clone())
            .collect()
    };
    for page in &mut initial_pages {
        page.document.configure_select(
            "format-select",
            labels.clone(),
            selected_choice.unwrap_or(0),
        );
        println!("Loading {} ({})", page.url, page.label);
    }

    let event_loop = EventLoop::<VideoEvent>::with_user_event()
        .build()
        .map_err(|error| format!("failed to create event loop: {error}"))?;
    let (video_tx, video_rx) = sync_channel(1);
    let proxy = event_loop.create_proxy();
    let playback = if let Some(path) = youtube_media_path.as_ref() {
        println!(
            "solara: cached YouTube playback source ready at {}",
            path.display()
        );
        Some(video::spawn_cached(
            path.clone(),
            0,
            video_tx.clone(),
            proxy.clone(),
        ))
    } else {
        println!("solara: cached YouTube playback source is not ready; video remains empty");
        None
    };
    let mut app = GpuUiApp {
        initial_pages,
        video_rx,
        video_tx,
        video_proxy: proxy,
        video_started: false,
        windows: HashMap::new(),
        watch_url,
        youtube_bootstrap,
        youtube_choices,
        selected_choice,
        active_request_id: 0,
        youtube_media_path,
        playback,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|error| format!("event loop failed: {error}"))
}

fn youtube_watch_url(input: Option<&str>) -> Result<Url, String> {
    let raw_url = input.unwrap_or(YOUTUBE_WATCH_URL);
    let url = Url::parse(raw_url)
        .map_err(|error| format!("invalid YouTube watch URL {raw_url:?}: {error}"))?;
    if url.scheme() != "https" {
        return Err("YouTube watch URL must use HTTPS".to_owned());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "YouTube watch URL has no host".to_owned())?;
    if host != "youtu.be" && host != "youtube.com" && !host.ends_with(".youtube.com") {
        return Err(format!("unsupported YouTube watch host {host:?}"));
    }
    Ok(url)
}

fn fetch_youtube_bootstrap(watch_url: &Url) -> Result<YoutubeStartup, String> {
    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .build()
        .into();
    let mut response = agent
        .get(watch_url.as_str())
        .header("User-Agent", YOUTUBE_USER_AGENT)
        .header("Accept-Language", "en-US,en;q=0.9")
        .call()
        .map_err(|error| format!("watch-page request failed: {error}"))?;
    let watch_html = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("watch-page response failed: {error}"))?;
    let bootstrap = youtube::extract_watch_bootstrap(&watch_html)
        .map_err(|error| format!("watch-page bootstrap failed: {error}"))?;
    let player_javascript = match fetch_youtube_player_javascript(&agent, &bootstrap, watch_url) {
        Ok(player_javascript) => player_javascript,
        Err(error) => {
            eprintln!("solara: YouTube player JavaScript was not cached: {error}");
            None
        }
    };
    match youtube::cache_watch_session(&bootstrap, &watch_html, player_javascript.as_deref()) {
        Ok(cache) => println!(
            "solara: YouTube session cache ready at {} (player JavaScript: {})",
            cache.directory.display(),
            if cache.player_javascript_cached {
                "cached"
            } else {
                "unavailable"
            },
        ),
        Err(error) => eprintln!("solara: YouTube session cache unavailable: {error}"),
    }
    let choices = youtube_media::available_mp4_choices(&bootstrap);
    let selected = youtube_media::default_choice_index(&choices);
    if let Some(choice) = selected.and_then(|index| choices.get(index))
        && choice.height != youtube_media::DEFAULT_HEIGHT
    {
        println!(
            "solara: 1440p MP4 unavailable; falling back once to {} (itag {})",
            choice.label, choice.itag
        );
    }
    let media_path = match selected.and_then(|index| choices.get(index)) {
        Some(choice) => match cache_selected_media(&bootstrap, watch_url, choice) {
            Ok(path) => Some(path),
            Err(error) => {
                eprintln!(
                    "solara: YouTube video asset unavailable; continuing without playback: {error}"
                );
                None
            }
        },
        None => {
            eprintln!("solara: YouTube response advertises no cacheable MP4 video formats");
            None
        }
    };
    Ok(YoutubeStartup {
        bootstrap,
        choices,
        selected,
        media_path,
    })
}

fn cache_selected_media(
    bootstrap: &YoutubeWatchBootstrap,
    watch_url: &Url,
    choice: &YoutubeMediaChoice,
) -> Result<PathBuf, String> {
    match youtube_media::cache_video_asset(bootstrap, watch_url.as_str(), choice.itag) {
        Ok(asset) => {
            println!(
                "solara: YouTube video asset cached at {} (itag {}; {} bytes)",
                asset.path.display(),
                asset.itag,
                asset.length,
            );
            if asset.has_estimated_playback_buffer(Duration::from_secs(10)) {
                println!("solara: cached media has at least 10 seconds of estimated playback");
                Ok(asset.path)
            } else {
                Err(
                    "cached YouTube asset does not yet have an estimated 10-second playback buffer"
                        .to_owned(),
                )
            }
        }
        Err(error) => Err(error),
    }
}

fn fetch_youtube_player_javascript(
    agent: &Agent,
    bootstrap: &YoutubeWatchBootstrap,
    watch_url: &Url,
) -> Result<Option<String>, String> {
    let Some(player_js_url) = bootstrap.player_js_url.as_deref() else {
        return Ok(None);
    };
    let player_js_url = watch_url
        .join(player_js_url)
        .map_err(|error| format!("invalid player JavaScript URL: {error}"))?;
    let mut response = agent
        .get(player_js_url.as_str())
        .header("User-Agent", YOUTUBE_USER_AGENT)
        .header("Referer", watch_url.as_str())
        .call()
        .map_err(|error| format!("player JavaScript request failed: {error}"))?;
    response
        .body_mut()
        .read_to_string()
        .map(Some)
        .map_err(|error| format!("player JavaScript response failed: {error}"))
}

struct InitialPage {
    label: &'static str,
    url: Url,
    title: String,
    favicon_url: Option<Url>,
    document: Document,
    plays_cached_video: bool,
}

fn load_initial_pages(watch_url: &str) -> Result<Vec<InitialPage>, String> {
    let mut page = load_initial_page(Some(VIDEO_DEMO_HTML_PATH), "YouTube cache playback", true)?;
    page.document.set_primary_heading_text(watch_url);
    Ok(vec![page])
}

fn load_initial_page(
    source: Option<&str>,
    label: &'static str,
    plays_cached_video: bool,
) -> Result<InitialPage, String> {
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
        plays_cached_video,
    })
}

struct GpuUiApp {
    initial_pages: Vec<InitialPage>,
    windows: HashMap<WindowId, PageWindow>,
    video_rx: Receiver<VideoPacket>,
    video_tx: SyncSender<VideoPacket>,
    video_proxy: EventLoopProxy<VideoEvent>,
    video_started: bool,
    watch_url: Url,
    youtube_bootstrap: Option<YoutubeWatchBootstrap>,
    youtube_choices: Vec<YoutubeMediaChoice>,
    selected_choice: Option<usize>,
    active_request_id: u64,
    youtube_media_path: Option<PathBuf>,
    playback: Option<video::PlaybackHandle>,
}

impl GpuUiApp {
    fn request_choice(&mut self, index: usize) {
        let Some(bootstrap) = self.youtube_bootstrap.clone() else {
            return;
        };
        let Some(choice) = self.youtube_choices.get(index).cloned() else {
            return;
        };
        if self.selected_choice == Some(index) {
            return;
        }
        self.selected_choice = Some(index);
        self.active_request_id = self.active_request_id.wrapping_add(1);
        let request_id = self.active_request_id;
        if let Some(playback) = self.playback.take() {
            playback.stop();
        }
        self.youtube_media_path = None;
        self.video_started = false;
        for window in self.windows.values_mut() {
            window.video_frame = None;
            window.window.request_redraw();
        }

        println!(
            "solara: format changed to {} (itag {}); requesting once",
            choice.label, choice.itag
        );
        let watch_url = self.watch_url.clone();
        let packet_tx = self.video_tx.clone();
        let proxy = self.video_proxy.clone();
        thread::Builder::new()
            .name("solara-youtube-cache".to_owned())
            .spawn(move || {
                let packet = match cache_selected_media(&bootstrap, &watch_url, &choice) {
                    Ok(path) => VideoPacket::CacheReady { request_id, path },
                    Err(error) => VideoPacket::CacheFailed { request_id, error },
                };
                if packet_tx.send(packet).is_ok() {
                    let _ = proxy.send_event(VideoEvent::PacketReady);
                }
            })
            .expect("failed to start YouTube cache request thread");
    }

    fn start_playback(&mut self, request_id: u64, path: PathBuf) {
        if request_id != self.active_request_id {
            return;
        }
        if let Some(playback) = self.playback.take() {
            playback.stop();
        }
        println!(
            "solara: cached YouTube playback source ready at {}",
            path.display()
        );
        self.playback = Some(video::spawn_cached(
            path.clone(),
            request_id,
            self.video_tx.clone(),
            self.video_proxy.clone(),
        ));
        self.youtube_media_path = Some(path);
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
    mouse_buttons: u32,
    modifiers: ModifiersState,
    plays_cached_video: bool,
    video_frame: Option<Arc<[u8]>>,
    _favicon_url: Option<Url>,
}

enum PageAction {
    FormatSelected(usize),
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
            mouse_buttons: 0,
            modifiers: ModifiersState::empty(),
            plays_cached_video: page.plays_cached_video,
            video_frame: None,
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
        #[cfg(feature = "gpu-text-only")]
        let result = self.renderer.render_text(&self.batch.text.sections);
        #[cfg(not(feature = "gpu-text-only"))]
        let result = match (self.video_frame.as_deref(), self.document.video_bounds()) {
            (Some(pixels), Some(mut destination)) => {
                for value in &mut destination {
                    *value *= self.scale_factor;
                }
                self.renderer.render_with_video(
                    &self.batch.shapes,
                    &self.batch.text.sections,
                    solara_wgpu_shim::RgbaVideoFrame {
                        pixels,
                        width: video::FRAME_WIDTH,
                        height: video::FRAME_HEIGHT,
                        destination,
                    },
                )
            }
            _ => self
                .renderer
                .render(&self.batch.shapes, &self.batch.text.sections),
        };
        match result {
            Ok(()) => {}
            Err(RenderError::Lost | RenderError::Outdated) => {
                let size = self.window.inner_size();
                self.renderer.resize(size.width, size.height);
            }
            Err(RenderError::Validation) => eprintln!("surface validation error"),
        }
    }

    fn handle_event(&mut self, event: WindowEvent) -> Option<PageAction> {
        let mut action = None;
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
                let movement = (logical.x - self.cursor.0, logical.y - self.cursor.1);
                self.cursor = (logical.x, logical.y);
                self.dispatch_mouse(
                    MouseInput::at(
                        MouseEventKind::Move,
                        self.cursor.0,
                        self.cursor.1,
                        self.mouse_buttons,
                    )
                    .with_screen(self.cursor.0, self.cursor.1)
                    .with_movement(movement.0, movement.1),
                );
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (delta_x, delta_y, scroll) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (-x * 24.0, -y * 24.0, y * 24.0),
                    MouseScrollDelta::PixelDelta(position) => {
                        let x = position.x as f32 / self.scale_factor;
                        let y = position.y as f32 / self.scale_factor;
                        (-x, -y, y)
                    }
                };
                let prevented = self.dispatch_mouse(
                    MouseInput::at(
                        MouseEventKind::Wheel,
                        self.cursor.0,
                        self.cursor.1,
                        self.mouse_buttons,
                    )
                    .with_wheel(delta_x, delta_y),
                );
                if !prevented {
                    self.document.scroll_by(scroll);
                    self.window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::MouseInput { state, button, .. } => {
                let (dom_button, button_mask) = mouse_button(button)?;
                match state {
                    ElementState::Pressed => {
                        self.mouse_buttons |= button_mask;
                        self.dispatch_mouse(
                            MouseInput::at(
                                MouseEventKind::Down,
                                self.cursor.0,
                                self.cursor.1,
                                self.mouse_buttons,
                            )
                            .with_button(dom_button),
                        );
                    }
                    ElementState::Released => {
                        self.mouse_buttons &= !button_mask;
                        self.dispatch_mouse(
                            MouseInput::at(
                                MouseEventKind::Up,
                                self.cursor.0,
                                self.cursor.1,
                                self.mouse_buttons,
                            )
                            .with_button(dom_button),
                        );
                        let prevented = self.dispatch_mouse(
                            MouseInput::at(
                                MouseEventKind::Click,
                                self.cursor.0,
                                self.cursor.1,
                                self.mouse_buttons,
                            )
                            .with_button(dom_button),
                        );
                        if button == MouseButton::Left && !prevented {
                            if let Some(interaction) = self
                                .document
                                .activate_select_at(self.cursor.0, self.cursor.1)
                            {
                                self.window.request_redraw();
                                if let Some(selected) = interaction.selected {
                                    action = Some(PageAction::FormatSelected(selected));
                                }
                            } else if self
                                .document
                                .toggle_details_at(self.cursor.0, self.cursor.1)
                            {
                                self.window.request_redraw();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        action
    }

    fn dispatch_mouse(&mut self, input: MouseInput) -> bool {
        let input = input.with_modifiers(
            self.modifiers.control_key(),
            self.modifiers.shift_key(),
            self.modifiers.alt_key(),
            self.modifiers.super_key(),
        );
        match self.document.dispatch_mouse(input) {
            Ok(outcome) => outcome.default_prevented,
            Err(error) => {
                eprintln!("solara: mouse dispatch failed: {error}");
                false
            }
        }
    }
}

fn mouse_button(button: MouseButton) -> Option<(i16, u32)> {
    match button {
        MouseButton::Left => Some((0, 1)),
        MouseButton::Middle => Some((1, 4)),
        MouseButton::Right => Some((2, 2)),
        MouseButton::Back => Some((3, 8)),
        MouseButton::Forward => Some((4, 16)),
        MouseButton::Other(button) if button < 16 => {
            Some((i16::try_from(button).ok()?, 1u32 << button))
        }
        MouseButton::Other(_) => None,
    }
}

fn logical_size(window: &Window) -> (f32, f32) {
    let size = window.inner_size().to_logical::<f32>(window.scale_factor());
    (size.width, size.height)
}

impl ApplicationHandler<VideoEvent> for GpuUiApp {
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

        let action = self
            .windows
            .get_mut(&window_id)
            .and_then(|page_window| page_window.handle_event(event));
        if let Some(PageAction::FormatSelected(index)) = action {
            self.request_choice(index);
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: VideoEvent) {
        while let Ok(packet) = self.video_rx.try_recv() {
            match packet {
                VideoPacket::Frame {
                    request_id,
                    pixels: frame,
                } if request_id == self.active_request_id => {
                    if !self.video_started {
                        self.video_started = true;
                        eprintln!(
                            "solara: cached YouTube video frames active ({}x{} RGBA)",
                            video::FRAME_WIDTH,
                            video::FRAME_HEIGHT,
                        );
                    }
                    for window in self
                        .windows
                        .values_mut()
                        .filter(|window| window.plays_cached_video)
                    {
                        window.video_frame = Some(Arc::clone(&frame));
                        window.window.request_redraw();
                    }
                }
                VideoPacket::Failed { request_id, error }
                    if request_id == self.active_request_id =>
                {
                    eprintln!("solara: cached YouTube playback stopped: {error}")
                }
                VideoPacket::CacheReady { request_id, path } => {
                    self.start_playback(request_id, path);
                }
                VideoPacket::CacheFailed { request_id, error }
                    if request_id == self.active_request_id =>
                {
                    eprintln!("solara: selected YouTube format could not be cached: {error}");
                }
                VideoPacket::Frame { .. }
                | VideoPacket::Failed { .. }
                | VideoPacket::CacheFailed { .. } => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{VIDEO_DEMO_HTML_PATH, load_initial_pages, youtube_watch_url};
    use crate::gpu_ui::html::{RenderBatch, collect_batch};

    #[test]
    fn default_session_is_the_single_video_ladder_window() {
        let target = "https://www.youtube.com/watch?v=custom123&t=4";
        let mut pages = load_initial_pages(target).expect("video demo loads");
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].label, "YouTube cache playback");
        assert!(pages[0].plays_cached_video);
        assert!(pages[0].url.path().ends_with("/docs/video_demo.html"));
        assert!(std::path::Path::new(VIDEO_DEMO_HTML_PATH).is_file());

        let mut page = pages.pop().unwrap();
        let video = page
            .document
            .video_bounds()
            .expect("video has layout bounds");
        assert!((video[2] / video[3] - 16.0 / 9.0).abs() < 0.001);
        let mut batch = RenderBatch::default();
        collect_batch(&page.document, 1.0, &mut batch);
        assert!(
            batch
                .text
                .sections
                .iter()
                .any(|section| { section.text.contains(target) })
        );

        page.document.relayout(480.0);
        let narrow = page.document.video_bounds().unwrap();
        assert_eq!(narrow[2], 448.0);
        assert!((narrow[2] / narrow[3] - 16.0 / 9.0).abs() < 0.001);
    }

    #[test]
    fn final_argument_accepts_only_https_youtube_targets() {
        assert_eq!(
            youtube_watch_url(None).unwrap().as_str(),
            "https://www.youtube.com/watch?v=nXvnof8fTBc"
        );
        assert!(youtube_watch_url(Some("https://youtu.be/nXvnof8fTBc")).is_ok());
        assert!(youtube_watch_url(Some("http://www.youtube.com/watch?v=test")).is_err());
        assert!(youtube_watch_url(Some("https://example.com/watch?v=test")).is_err());
    }
}
