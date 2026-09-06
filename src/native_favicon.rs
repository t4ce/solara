//! Poll disk and network operations without blocking the browser frame loop.
use solara::favicon::{self, IconSource};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use trueos::{async_fs, vmedia};

type Loading = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>>>>;

pub(crate) struct Favicon {
    pub pixels: Vec<u8>,
    loading: Option<Loading>,
}
impl Default for Favicon {
    fn default() -> Self {
        Self {
            pixels: favicon::fallback(),
            loading: None,
        }
    }
}
impl Favicon {
    pub fn navigate(&mut self, request: Option<(String, Vec<String>)>) {
        self.pixels = favicon::fallback();
        self.loading =
            request.map(|(origin, candidates)| Box::pin(load(origin, candidates)) as Loading);
    }
    pub fn poll(&mut self) -> bool {
        let Some(future) = &mut self.loading else {
            return false;
        };
        let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        else {
            return false;
        };
        self.loading = None;
        match result {
            Ok(pixels) => {
                self.pixels = pixels;
                true
            }
            Err(error) => {
                crate::parser_probe::report_info(format_args!(
                    "solara: favicon fallback reason={error}"
                ));
                false
            }
        }
    }
}

async fn load(origin: String, candidates: Vec<String>) -> Result<Vec<u8>, String> {
    let path = favicon::cache_path(&origin);
    let now = match trueos::clock::ntp_current_unix_seconds() {
        0 => trueos::clock::unix_seconds().unwrap_or(0),
        unix => unix,
    };
    if let Ok(meta) = async_fs::metadata(path.as_bytes()).await {
        if meta.len <= favicon::MAX_RECORD_BYTES as u64 {
            if let Ok(record) = async_fs::read_file(path.as_bytes()).await {
                if let Some(pixels) = favicon::decode_record(&origin, now, &record) {
                    crate::parser_probe::report_info(format_args!(
                        "solara: favicon cache-hit origin={origin}"
                    ));
                    return Ok(pixels);
                }
            }
        }
    }
    let mut last_error = String::from("no supported icon");
    for url in candidates {
        if url::Url::parse(&url)
            .ok()
            .is_some_and(|u| u.path().ends_with(".svg"))
        {
            continue;
        }
        let result = async {
            let bytes =
                crate::native_images::fetch_bytes_limited(url.clone(), favicon::MAX_ENCODED_BYTES)
                    .await?;
            match favicon::icon_source(&bytes).ok_or("unsupported or oversized favicon")? {
                IconSource::Rgba {
                    width,
                    height,
                    pixels,
                } => favicon::tile(width, height, width as usize * 4, &pixels)
                    .ok_or("invalid ICO extent".into()),
                IconSource::Encoded(encoded) => {
                    let format = if encoded.starts_with(b"\x89PNG\r\n\x1a\n") {
                        let dimensions = encoded.get(16..24).ok_or("truncated PNG")?;
                        let width = u32::from_be_bytes(dimensions[..4].try_into().unwrap());
                        let height = u32::from_be_bytes(dimensions[4..].try_into().unwrap());
                        if width > 1024 || height > 1024 {
                            return Err("favicon dimensions exceed limit".into());
                        }
                        vmedia::ImageFormat::Png
                    } else if encoded.starts_with(b"BM") {
                        vmedia::ImageFormat::Bmp
                    } else {
                        return Err("unsupported favicon encoding".into());
                    };
                    let decoded = vmedia::decode(format, encoded)
                        .await
                        .map_err(|e| format!("icon decode: {e}"))?;
                    favicon::tile(
                        decoded.info.width,
                        decoded.info.height,
                        decoded.info.stride_bytes as usize,
                        &decoded.rgba,
                    )
                    .ok_or("invalid favicon pixels".into())
                }
            }
        }
        .await;
        match result {
            Ok(pixels) => {
                if let Some(record) = favicon::encode_record(&origin, now, &pixels) {
                    let stored = async {
                        async_fs::create_dir_all(b"apps/common/solara/favicons").await?;
                        async_fs::write_file(path.as_bytes(), &record).await
                    }
                    .await;
                    crate::parser_probe::report_info(format_args!(
                        "solara: favicon fetched origin={origin} cached={}",
                        stored.is_ok()
                    ));
                }
                return Ok(pixels);
            }
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}
