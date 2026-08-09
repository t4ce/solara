use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::thread;

use winit::event_loop::EventLoopProxy;

pub const FRAME_WIDTH: u32 = 768;
pub const FRAME_HEIGHT: u32 = 432;
const FRAME_BYTES: usize = FRAME_WIDTH as usize * FRAME_HEIGHT as usize * 4;

#[derive(Debug)]
pub enum VideoPacket {
    Frame { request_id: u64, pixels: Arc<[u8]> },
    Failed { request_id: u64, error: String },
    CacheReady { request_id: u64, path: PathBuf },
    CacheFailed { request_id: u64, error: String },
}

#[derive(Clone, Copy, Debug)]
pub enum VideoEvent {
    PacketReady,
}

pub struct PlaybackHandle {
    cancelled: Arc<AtomicBool>,
}

impl PlaybackHandle {
    pub fn stop(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

pub fn spawn_cached(
    path: PathBuf,
    request_id: u64,
    packet_tx: SyncSender<VideoPacket>,
    proxy: EventLoopProxy<VideoEvent>,
) -> PlaybackHandle {
    let cancelled = Arc::new(AtomicBool::new(false));
    let thread_cancelled = Arc::clone(&cancelled);
    thread::Builder::new()
        .name("solara-video-decode".to_string())
        .spawn(move || {
            if let Err(error) =
                decode_file(&path, request_id, &thread_cancelled, &packet_tx, &proxy)
            {
                match packet_tx.try_send(VideoPacket::Failed { request_id, error }) {
                    Ok(()) => {
                        let _ = proxy.send_event(VideoEvent::PacketReady);
                    }
                    Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
                }
            }
        })
        .expect("failed to start Solara's video decode thread");
    PlaybackHandle { cancelled }
}

fn decode_file(
    path: &Path,
    request_id: u64,
    cancelled: &AtomicBool,
    packet_tx: &SyncSender<VideoPacket>,
    proxy: &EventLoopProxy<VideoEvent>,
) -> Result<(), String> {
    let caps = format!(
        "video/x-raw,format=RGBA,width={FRAME_WIDTH},height={FRAME_HEIGHT},pixel-aspect-ratio=1/1"
    );
    let location = format!("location={}", path.display());
    let mut child = Command::new("gst-launch-1.0")
        .args([
            "-q",
            "filesrc",
            location.as_str(),
            "!",
            "decodebin",
            "!",
            "videoconvert",
            "!",
            "videoscale",
            "!",
            caps.as_str(),
            "!",
            "fdsink",
            "fd=1",
            "sync=true",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot start Linux GStreamer playback: {error}"))?;

    let mut output = child
        .stdout
        .take()
        .ok_or_else(|| "GStreamer video pipe is unavailable".to_string())?;
    let mut error_output = child
        .stderr
        .take()
        .ok_or_else(|| "GStreamer error pipe is unavailable".to_string())?;
    let error_reader = thread::spawn(move || {
        let mut message = String::new();
        let _ = error_output.read_to_string(&mut message);
        message
    });

    loop {
        if cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = error_reader.join();
            return Ok(());
        }
        let mut rgba = vec![0; FRAME_BYTES];
        match output.read_exact(&mut rgba) {
            Ok(()) => {
                if packet_tx
                    .send(VideoPacket::Frame {
                        request_id,
                        pixels: rgba.into(),
                    })
                    .is_err()
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = error_reader.join();
                    return Ok(());
                }
                if proxy.send_event(VideoEvent::PacketReady).is_err() {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = error_reader.join();
                    return Ok(());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = error_reader.join();
                return Err(format!("failed to read a decoded video frame: {error}"));
            }
        }
    }

    let status = child
        .wait()
        .map_err(|error| format!("failed to reap GStreamer: {error}"))?;
    let decoder_error = error_reader
        .join()
        .map_err(|_| "GStreamer error reader panicked".to_string())?;
    if !status.success() {
        return Err(format!(
            "GStreamer exited with {status}: {}",
            concise_error(&decoder_error)
        ));
    }
    Ok(())
}

fn concise_error(message: &str) -> &str {
    let mut lines = message.lines().filter(|line| !line.trim().is_empty());
    lines
        .clone()
        .find(|line| {
            line.contains("Missing element:")
                || line.contains("Missing decoder:")
                || line.contains("missing a plug-in")
        })
        .or_else(|| lines.next_back())
        .map(str::trim)
        .unwrap_or("no decoder error was reported")
}
