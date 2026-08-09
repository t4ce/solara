use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use ureq::Agent;

use crate::gpu_ui::media_store;
use crate::gpu_ui::youtube::YoutubeWatchBootstrap;

pub(crate) const DEFAULT_HEIGHT: u32 = 1440;
const YT_DLP_VERSION: &str = "2026.07.04";
const YT_DLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/download/2026.07.04/yt-dlp";
const YT_DLP_SHA256: &str = "495be29ff4d9d4e9be7eabdfef225221e5d5282e77f2f505abc6dca80349f3fd";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CachedYoutubeMedia {
    pub(crate) path: PathBuf,
    pub(crate) length: u64,
    pub(crate) itag: u32,
    duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct YoutubeMediaChoice {
    pub(crate) itag: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) label: String,
}

pub(crate) fn available_mp4_choices(bootstrap: &YoutubeWatchBootstrap) -> Vec<YoutubeMediaChoice> {
    let mut formats: Vec<_> = bootstrap
        .adaptive_formats
        .iter()
        .filter(|format| {
            format.mime_type.starts_with("video/mp4")
                && format.width.is_some()
                && format.height.is_some()
                && format.content_length.is_some()
                && format.duration_ms.is_some()
        })
        .collect();
    formats.sort_by(|left, right| {
        right
            .height
            .cmp(&left.height)
            .then_with(|| codec_rank(&left.mime_type).cmp(&codec_rank(&right.mime_type)))
            .then_with(|| left.content_length.cmp(&right.content_length))
            .then_with(|| left.itag.cmp(&right.itag))
    });

    let mut choices = Vec::new();
    for format in formats {
        let width = format.width.unwrap();
        let height = format.height.unwrap();
        if choices
            .last()
            .is_some_and(|choice: &YoutubeMediaChoice| choice.height == height)
        {
            continue;
        }
        choices.push(YoutubeMediaChoice {
            itag: format.itag,
            width,
            height,
            label: format!("{height}p MP4"),
        });
    }
    choices
}

pub(crate) fn default_choice_index(choices: &[YoutubeMediaChoice]) -> Option<usize> {
    choices
        .iter()
        .position(|choice| choice.height == DEFAULT_HEIGHT)
        .or_else(|| {
            choices
                .iter()
                .position(|choice| choice.height < DEFAULT_HEIGHT)
        })
        .or((!choices.is_empty()).then_some(0))
}

fn codec_rank(mime_type: &str) -> u8 {
    if mime_type.contains("av01.") {
        0
    } else if mime_type.contains("avc1.") {
        1
    } else {
        2
    }
}

impl CachedYoutubeMedia {
    pub(crate) fn has_estimated_playback_buffer(&self, minimum: Duration) -> bool {
        let Ok(metadata) = self.path.metadata() else {
            return false;
        };
        if self.length == 0 || self.duration_ms == 0 {
            return false;
        }
        let requested_ms = u64::try_from(minimum.as_millis()).unwrap_or(u64::MAX);
        let required_ms = requested_ms.min(self.duration_ms);
        u128::from(metadata.len()) * u128::from(self.duration_ms)
            >= u128::from(self.length) * u128::from(required_ms)
    }
}

pub(crate) fn cache_video_asset(
    bootstrap: &YoutubeWatchBootstrap,
    watch_url: &str,
    itag: u32,
) -> Result<CachedYoutubeMedia, String> {
    let format = bootstrap
        .adaptive_formats
        .iter()
        .find(|format| format.itag == itag)
        .ok_or_else(|| format!("YouTube format {itag} is unavailable"))?;
    if !format.mime_type.starts_with("video/mp4") {
        return Err(format!(
            "YouTube format {itag} is not an MP4 video: {}",
            format.mime_type
        ));
    }
    let expected_length = format
        .content_length
        .ok_or_else(|| format!("YouTube format {itag} has no content length"))?;
    let duration_ms = format
        .duration_ms
        .ok_or_else(|| format!("YouTube format {itag} has no duration"))?;
    let directory = media_store::root()
        .join("youtube")
        .join(&bootstrap.video_id);
    let destination = directory.join(format!("video-{itag}.mp4"));

    if destination.exists() {
        validate_video_asset(&destination, expected_length)?;
        return Ok(CachedYoutubeMedia {
            path: destination,
            length: expected_length,
            itag,
            duration_ms,
        });
    }

    let yt_dlp = ensure_yt_dlp()?;
    std::fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "failed to create YouTube media cache {}: {error}",
            directory.display()
        )
    })?;
    let output = Command::new("python3")
        .arg(&yt_dlp)
        .args([
            "--no-config-locations",
            "--no-playlist",
            "--js-runtimes",
            "node",
            "--extractor-args",
            "youtube:player_client=web_embedded",
            "--format",
        ])
        .arg(itag.to_string())
        .arg("--output")
        .arg(&destination)
        .args(["--no-progress", "--quiet", watch_url])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("failed to start pinned yt-dlp media bridge: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "pinned yt-dlp media bridge failed: {}",
            concise_process_error(&output.stderr)
        ));
    }
    validate_video_asset(&destination, expected_length)?;
    Ok(CachedYoutubeMedia {
        path: destination,
        length: expected_length,
        itag,
        duration_ms,
    })
}

fn ensure_yt_dlp() -> Result<PathBuf, String> {
    let directory = media_store::cache_root().join("tools");
    let path = directory.join(format!("yt-dlp-{YT_DLP_VERSION}"));
    if path.exists() {
        verify_yt_dlp(&path)?;
        return Ok(path);
    }

    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut response = agent
        .get(YT_DLP_URL)
        .header("User-Agent", "Solara media bridge bootstrap")
        .call()
        .map_err(|error| format!("failed to fetch pinned yt-dlp bridge: {error}"))?;
    let bytes = response
        .body_mut()
        .read_to_vec()
        .map_err(|error| format!("failed to read pinned yt-dlp bridge: {error}"))?;
    let path = media_store::store_snapshot(
        &directory,
        path.file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "invalid yt-dlp cache path".to_owned())?,
        &bytes,
    )?;
    if let Err(error) = verify_yt_dlp(&path) {
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

fn verify_yt_dlp(path: &Path) -> Result<(), String> {
    let output = Command::new("sha256sum")
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to verify pinned yt-dlp bridge: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "sha256sum could not verify pinned yt-dlp bridge: {}",
            concise_process_error(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let digest = stdout.split_whitespace().next().unwrap_or_default();
    if digest != YT_DLP_SHA256 {
        return Err(format!(
            "pinned yt-dlp bridge digest mismatch: expected {YT_DLP_SHA256}, received {digest}"
        ));
    }
    Ok(())
}

fn validate_video_asset(path: &Path, expected_length: u64) -> Result<(), String> {
    let mut file = File::open(path).map_err(|error| {
        format!(
            "failed to open cached YouTube video {}: {error}",
            path.display()
        )
    })?;
    let actual_length = file
        .metadata()
        .map_err(|error| {
            format!(
                "failed to inspect cached YouTube video {}: {error}",
                path.display()
            )
        })?
        .len();
    if actual_length != expected_length {
        return Err(format!(
            "cached YouTube video {} is incomplete: expected {expected_length} bytes, found {actual_length}",
            path.display()
        ));
    }
    let mut header = [0_u8; 12];
    file.read_exact(&mut header).map_err(|error| {
        format!(
            "failed to read cached YouTube video header {}: {error}",
            path.display()
        )
    })?;
    if &header[4..8] != b"ftyp" {
        return Err(format!(
            "cached YouTube video {} is not an ISO BMFF asset",
            path.display()
        ));
    }
    Ok(())
}

fn concise_process_error(stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("process exited without an error message")
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        CachedYoutubeMedia, available_mp4_choices, default_choice_index, validate_video_asset,
    };
    use crate::gpu_ui::youtube::{YoutubeAdaptiveFormat, YoutubeWatchBootstrap};

    fn format(itag: u32, height: u32, codec: &str) -> YoutubeAdaptiveFormat {
        YoutubeAdaptiveFormat {
            itag,
            mime_type: format!("video/mp4; codecs=\"{codec}\""),
            width: Some(height * 16 / 9),
            height: Some(height),
            content_length: Some(u64::from(itag) * 1_000),
            duration_ms: Some(60_000),
        }
    }

    #[test]
    fn validates_expected_length_and_iso_media_header() {
        let directory =
            std::env::temp_dir().join(format!("solara-youtube-media-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("test directory creates");
        let path = directory.join("video-400.mp4");
        std::fs::write(&path, b"\0\0\0\x1cftypdashpayload").expect("fixture writes");

        validate_video_asset(&path, 19).expect("complete ISO media validates");
        let error = validate_video_asset(&path, 20).expect_err("wrong length rejects");
        assert!(error.contains("is incomplete"));
        std::fs::remove_dir_all(&directory).expect("test directory removes");
    }

    #[test]
    fn estimates_a_ten_second_buffer_from_cached_bytes() {
        let directory =
            std::env::temp_dir().join(format!("solara-youtube-buffer-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("test directory creates");
        let path = directory.join("video-400.mp4.part");
        std::fs::write(&path, vec![0_u8; 10]).expect("partial fixture writes");
        let media = CachedYoutubeMedia {
            path,
            length: 100,
            itag: 400,
            duration_ms: 100_000,
        };

        assert!(media.has_estimated_playback_buffer(Duration::from_secs(10)));
        assert!(!media.has_estimated_playback_buffer(Duration::from_secs(11)));
        std::fs::remove_dir_all(&directory).expect("test directory removes");
    }

    #[test]
    fn chooses_one_mp4_per_resolution_and_falls_below_1440p_once() {
        let bootstrap = YoutubeWatchBootstrap {
            video_id: "test".to_owned(),
            title: "test".to_owned(),
            player_js_url: None,
            adaptive_formats: vec![
                format(401, 2160, "av01.0.12M.08"),
                format(137, 1080, "avc1.640028"),
                format(399, 1080, "av01.0.08M.08"),
                format(136, 720, "avc1.4d401f"),
            ],
            server_abr_streaming_url: None,
            video_playback_ustreamer_config: None,
            expires_in_seconds: None,
        };

        let choices = available_mp4_choices(&bootstrap);
        assert_eq!(
            choices
                .iter()
                .map(|choice| choice.height)
                .collect::<Vec<_>>(),
            vec![2160, 1080, 720]
        );
        assert_eq!(choices[1].itag, 399);
        assert_eq!(default_choice_index(&choices), Some(1));
    }
}
