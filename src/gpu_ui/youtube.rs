//! Passive extraction of the playback bootstrap embedded in a YouTube watch page.
//!
//! This module deliberately stops at the page boundary: URLs and opaque player
//! configuration are captured as supplied, without fetching, rewriting, or
//! interpreting them.

#![allow(
    dead_code,
    reason = "staged bootstrap fields are retained for later media handoff milestones"
)]

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::gpu_ui::media_store;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct YoutubeWatchBootstrap {
    pub(crate) video_id: String,
    pub(crate) title: String,
    pub(crate) player_js_url: Option<String>,
    pub(crate) adaptive_formats: Vec<YoutubeAdaptiveFormat>,
    pub(crate) server_abr_streaming_url: Option<String>,
    pub(crate) video_playback_ustreamer_config: Option<String>,
    pub(crate) expires_in_seconds: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct YoutubeAdaptiveFormat {
    pub(crate) itag: u32,
    pub(crate) mime_type: String,
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) content_length: Option<u64>,
    pub(crate) duration_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum YoutubeBootstrapError {
    PlayerResponseNotFound,
    InvalidPlayerResponse(String),
    MissingVideoId,
    MissingTitle,
}

impl fmt::Display for YoutubeBootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlayerResponseNotFound => {
                formatter.write_str("ytInitialPlayerResponse was not found in the watch page")
            }
            Self::InvalidPlayerResponse(error) => {
                write!(
                    formatter,
                    "ytInitialPlayerResponse is not valid JSON: {error}"
                )
            }
            Self::MissingVideoId => {
                formatter.write_str("ytInitialPlayerResponse has no videoDetails.videoId")
            }
            Self::MissingTitle => {
                formatter.write_str("ytInitialPlayerResponse has no videoDetails.title")
            }
        }
    }
}

impl std::error::Error for YoutubeBootstrapError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct YoutubeSessionCache {
    pub(crate) directory: PathBuf,
    pub(crate) player_javascript_cached: bool,
}

pub(crate) fn cache_watch_session(
    bootstrap: &YoutubeWatchBootstrap,
    watch_html: &str,
    player_javascript: Option<&str>,
) -> Result<YoutubeSessionCache, String> {
    let video_id = bootstrap.video_id.as_str();
    if video_id.is_empty()
        || !video_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("YouTube video ID is not a safe cache component".to_owned());
    }
    let directory = media_store::root().join("youtube").join(video_id);
    cache_watch_session_in(&directory, bootstrap, watch_html, player_javascript)?;
    Ok(YoutubeSessionCache {
        directory,
        player_javascript_cached: player_javascript.is_some(),
    })
}

fn cache_watch_session_in(
    directory: &Path,
    bootstrap: &YoutubeWatchBootstrap,
    watch_html: &str,
    player_javascript: Option<&str>,
) -> Result<(), String> {
    media_store::store_snapshot(directory, "watch.html", watch_html.as_bytes())?;
    if let Some(player_js_url) = bootstrap.player_js_url.as_deref() {
        media_store::store_snapshot(directory, "player-url.txt", player_js_url.as_bytes())?;
    }
    if let Some(server_abr_url) = bootstrap.server_abr_streaming_url.as_deref() {
        media_store::store_snapshot(directory, "server-abr-url.txt", server_abr_url.as_bytes())?;
    }
    if let Some(ustreamer_config) = bootstrap.video_playback_ustreamer_config.as_deref() {
        media_store::store_snapshot(
            directory,
            "video-playback-ustreamer-config.bin",
            ustreamer_config.as_bytes(),
        )?;
    }
    if let Some(player_javascript) = player_javascript {
        media_store::store_snapshot(directory, "player.js", player_javascript.as_bytes())?;
    }
    Ok(())
}

pub(crate) fn extract_watch_bootstrap(
    watch_html: &str,
) -> Result<YoutubeWatchBootstrap, YoutubeBootstrapError> {
    let player_response = extract_player_response(watch_html)?;
    let video_details = player_response
        .get("videoDetails")
        .and_then(Value::as_object);

    let video_id = video_details
        .and_then(|details| details.get("videoId"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(YoutubeBootstrapError::MissingVideoId)?
        .to_owned();
    let title = video_details
        .and_then(|details| details.get("title"))
        .and_then(Value::as_str)
        .ok_or(YoutubeBootstrapError::MissingTitle)?
        .to_owned();

    let streaming_data = player_response.get("streamingData");
    let adaptive_formats = streaming_data
        .and_then(|data| data.get("adaptiveFormats"))
        .and_then(Value::as_array)
        .map(|formats| formats.iter().filter_map(parse_adaptive_format).collect())
        .unwrap_or_default();
    let server_abr_streaming_url = streaming_data
        .and_then(|data| data.get("serverAbrStreamingUrl"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let expires_in_seconds = streaming_data
        .and_then(|data| data.get("expiresInSeconds"))
        .and_then(json_u64);

    let player_js_url = player_response
        .get("assets")
        .and_then(|assets| assets.get("js"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| find_json_string_property(watch_html, "PLAYER_JS_URL"))
        .or_else(|| find_json_string_property(watch_html, "jsUrl"));
    let video_playback_ustreamer_config =
        find_string_property(&player_response, "videoPlaybackUstreamerConfig")
            .map(str::to_owned)
            .or_else(|| find_json_string_property(watch_html, "videoPlaybackUstreamerConfig"));

    Ok(YoutubeWatchBootstrap {
        video_id,
        title,
        player_js_url,
        adaptive_formats,
        server_abr_streaming_url,
        video_playback_ustreamer_config,
        expires_in_seconds,
    })
}

fn extract_player_response(watch_html: &str) -> Result<Value, YoutubeBootstrapError> {
    const KEY: &str = "ytInitialPlayerResponse";
    let mut parse_error = None;

    for (key_offset, _) in watch_html.match_indices(KEY) {
        let after_key = key_offset + KEY.len();
        let Some(object_start) = assigned_object_start(watch_html, after_key) else {
            continue;
        };
        let Some(object_end) = balanced_object_end(watch_html, object_start) else {
            parse_error = Some("unterminated JSON object".to_owned());
            continue;
        };

        match serde_json::from_str::<Value>(&watch_html[object_start..object_end]) {
            Ok(value) if looks_like_player_response(&value) => return Ok(value),
            Ok(_) => continue,
            Err(error) => parse_error = Some(error.to_string()),
        }
    }

    match parse_error {
        Some(error) => Err(YoutubeBootstrapError::InvalidPlayerResponse(error)),
        None => Err(YoutubeBootstrapError::PlayerResponseNotFound),
    }
}

fn assigned_object_start(source: &str, after_key: usize) -> Option<usize> {
    let search_end = (after_key + 64).min(source.len());
    let suffix = source.get(after_key..search_end)?;
    let delimiter = suffix.find([':', '='])?;
    let after_delimiter = after_key + delimiter + 1;
    let whitespace =
        source.get(after_delimiter..)?.len() - source.get(after_delimiter..)?.trim_start().len();
    let object_start = after_delimiter + whitespace;
    (source.as_bytes().get(object_start) == Some(&b'{')).then_some(object_start)
}

fn balanced_object_end(source: &str, object_start: usize) -> Option<usize> {
    if source.as_bytes().get(object_start) != Some(&b'{') {
        return None;
    }

    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;
    for (relative_offset, byte) in source.as_bytes()[object_start..]
        .iter()
        .copied()
        .enumerate()
    {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }

        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(object_start + relative_offset + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn looks_like_player_response(value: &Value) -> bool {
    value.get("videoDetails").is_some() || value.get("streamingData").is_some()
}

fn parse_adaptive_format(value: &Value) -> Option<YoutubeAdaptiveFormat> {
    let itag = value.get("itag").and_then(json_u64)?.try_into().ok()?;
    let mime_type = value.get("mimeType")?.as_str()?.to_owned();
    Some(YoutubeAdaptiveFormat {
        itag,
        mime_type,
        width: value
            .get("width")
            .and_then(json_u64)
            .and_then(|number| number.try_into().ok()),
        height: value
            .get("height")
            .and_then(json_u64)
            .and_then(|number| number.try_into().ok()),
        content_length: value.get("contentLength").and_then(json_u64),
        duration_ms: value
            .get("approxDurationMs")
            .or_else(|| value.get("durationMs"))
            .and_then(json_u64),
    })
}

fn json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn find_string_property<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    match value {
        Value::Object(object) => {
            if let Some(found) = object.get(key).and_then(Value::as_str) {
                return Some(found);
            }
            object
                .values()
                .find_map(|child| find_string_property(child, key))
        }
        Value::Array(array) => array
            .iter()
            .find_map(|child| find_string_property(child, key)),
        _ => None,
    }
}

fn find_json_string_property(source: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    for (key_start, _) in source.match_indices(&needle) {
        let after_key = key_start + needle.len();
        let Some(suffix) = source.get(after_key..) else {
            continue;
        };
        let Some(value_source) = suffix.trim_start().strip_prefix(':') else {
            continue;
        };
        let value_source = value_source.trim_start();
        let mut values = serde_json::Deserializer::from_str(value_source).into_iter::<Value>();
        if let Some(Ok(Value::String(value))) = values.next() {
            return Some(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        YoutubeBootstrapError, YoutubeWatchBootstrap, balanced_object_end, cache_watch_session_in,
        extract_watch_bootstrap,
    };
    use std::sync::atomic::{AtomicU64, Ordering};

    static CACHE_TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn extracts_passive_watch_snapshot_without_rewriting_opaque_values() {
        let html = r#"
            <p>"PLAYER_JS_URL" is mentioned before its JSON property.</p>
            <script>const decoy = "ytInitialPlayerResponse = {not JSON}";</script>
            <script>
              var ytInitialPlayerResponse = {
                "videoDetails": {
                  "videoId": "nXvnof8fTBc",
                  "title": "A } brace and an escaped \"quote\""
                },
                "streamingData": {
                  "expiresInSeconds": "21540",
                  "serverAbrStreamingUrl": "https://example.invalid/sabr?n=opaque%2Bvalue&expire=123",
                  "adaptiveFormats": [
                    {
                      "itag": 134,
                      "mimeType": "video/mp4; codecs=\"avc1.4d401e\"",
                      "width": 640,
                      "height": 360,
                      "contentLength": "987654",
                      "approxDurationMs": "120034"
                    },
                    {"itag": "140", "mimeType": "audio/mp4", "contentLength": 4567},
                    {"itag": 999}
                  ]
                },
                "playbackConfig": {
                  "videoPlaybackUstreamerConfig": "opaque+/=={}"
                }
              };
              ytcfg.set({"PLAYER_JS_URL":"\/s\/player\/abc123\/player_ias.vflset\/en_US\/base.js"});
            </script>
        "#;

        let snapshot = extract_watch_bootstrap(html).expect("fixture should extract");

        assert_eq!(snapshot.video_id, "nXvnof8fTBc");
        assert_eq!(snapshot.title, "A } brace and an escaped \"quote\"");
        assert_eq!(
            snapshot.player_js_url.as_deref(),
            Some("/s/player/abc123/player_ias.vflset/en_US/base.js")
        );
        assert_eq!(
            snapshot.server_abr_streaming_url.as_deref(),
            Some("https://example.invalid/sabr?n=opaque%2Bvalue&expire=123")
        );
        assert_eq!(
            snapshot.video_playback_ustreamer_config.as_deref(),
            Some("opaque+/=={}")
        );
        assert_eq!(snapshot.expires_in_seconds, Some(21_540));
        assert_eq!(snapshot.adaptive_formats.len(), 2);
        assert_eq!(snapshot.adaptive_formats[0].itag, 134);
        assert_eq!(
            snapshot.adaptive_formats[0].mime_type,
            "video/mp4; codecs=\"avc1.4d401e\""
        );
        assert_eq!(snapshot.adaptive_formats[0].width, Some(640));
        assert_eq!(snapshot.adaptive_formats[0].height, Some(360));
        assert_eq!(snapshot.adaptive_formats[0].content_length, Some(987_654));
        assert_eq!(snapshot.adaptive_formats[0].duration_ms, Some(120_034));
    }

    #[test]
    fn accepts_player_response_as_a_nested_json_property_and_assets_js() {
        let html = r#"
          <script>
            window.bootstrap = {
              "ytInitialPlayerResponse": {
                "videoDetails": {"videoId": "fixture", "title": "Fixture"},
                "assets": {"js": "https://www.youtube.com/s/player/exact/base.js"},
                "streamingData": {"expiresInSeconds": 42, "adaptiveFormats": []}
              }
            };
          </script>
        "#;

        let snapshot = extract_watch_bootstrap(html).expect("nested fixture should extract");
        assert_eq!(snapshot.video_id, "fixture");
        assert_eq!(snapshot.expires_in_seconds, Some(42));
        assert_eq!(
            snapshot.player_js_url.as_deref(),
            Some("https://www.youtube.com/s/player/exact/base.js")
        );
    }

    #[test]
    fn reports_an_unterminated_player_response() {
        let error = extract_watch_bootstrap(
            r#"<script>var ytInitialPlayerResponse = {"videoDetails": {</script>"#,
        )
        .expect_err("unterminated fixture must fail");

        assert_eq!(
            error,
            YoutubeBootstrapError::InvalidPlayerResponse("unterminated JSON object".to_owned())
        );
    }

    #[test]
    fn balanced_json_ignores_braces_inside_strings() {
        let source = r#"prefix {"nested":{"text":"} { \\\" still text"}} suffix"#;
        let start = source.find('{').expect("fixture object");
        let end = balanced_object_end(source, start).expect("fixture should balance");
        assert_eq!(
            &source[start..end],
            r#"{"nested":{"text":"} { \\\" still text"}}"#
        );
    }

    #[test]
    fn caches_exact_opaque_watch_session_materials() {
        let sequence = CACHE_TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "solara-youtube-cache-test-{}-{sequence}",
            std::process::id()
        ));
        let bootstrap = YoutubeWatchBootstrap {
            video_id: "nXvnof8fTBc".to_owned(),
            title: "Fixture".to_owned(),
            player_js_url: Some("/s/player/exact/base.js".to_owned()),
            adaptive_formats: Vec::new(),
            server_abr_streaming_url: Some("https://example.invalid/sabr?n=opaque%2B".to_owned()),
            video_playback_ustreamer_config: Some("opaque+/=={}".to_owned()),
            expires_in_seconds: Some(42),
        };
        let watch_html = "<html>exact watch bytes</html>";
        let player_javascript = "/* exact player bytes */";

        cache_watch_session_in(&directory, &bootstrap, watch_html, Some(player_javascript))
            .expect("session caches");

        assert_eq!(
            std::fs::read(directory.join("watch.html")).expect("watch cache reads"),
            watch_html.as_bytes()
        );
        assert_eq!(
            std::fs::read(directory.join("server-abr-url.txt")).expect("SABR cache reads"),
            bootstrap.server_abr_streaming_url.unwrap().as_bytes()
        );
        assert_eq!(
            std::fs::read(directory.join("video-playback-ustreamer-config.bin"))
                .expect("capsule cache reads"),
            bootstrap
                .video_playback_ustreamer_config
                .unwrap()
                .as_bytes()
        );
        assert_eq!(
            std::fs::read(directory.join("player.js")).expect("player cache reads"),
            player_javascript.as_bytes()
        );
        std::fs::remove_dir_all(&directory).expect("test session cache removes");
    }
}
