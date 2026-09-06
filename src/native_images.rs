//! JPEG transport and decoding stay in the kernel. Poll each operation without
//! blocking the UI; upload once and reuse the image's buffer across page frames.
use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};
use solara::{native_paint::jpeg_url, spec_layout::SpecLayout};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::{Future, poll_fn},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
use trueos::{
    netfs,
    vgpu::{self, Buffer, Device},
    vmedia,
};

#[derive(Default)]
pub(crate) struct Resources {
    queued: Mutex<VecDeque<String>>,
    seen: Mutex<BTreeSet<String>>,
}
impl NetProvider for Resources {
    fn fetch(&self, _: usize, request: Request, handler: Box<dyn NetHandler>) {
        if jpeg_url(&request.url) {
            let url = request.url.to_string();
            if self.seen.lock().expect("resource lock").insert(url.clone()) {
                self.queued.lock().expect("resource lock").push_back(url);
            }
            // Completion is delivered as decoded Resource::Image, bypassing
            // Blitz's encoded-byte handler and its optional client decoders.
        } else {
            let css = crate::parser_probe::load_embedded_stylesheet("", None, request.url.path())
                .map(|v| v.css)
                .unwrap_or_default();
            handler.bytes(request.url.to_string(), Bytes::from(css));
        }
    }
}
struct Fetch(u32);
impl Drop for Fetch {
    fn drop(&mut self) {
        let _ = netfs::fetch_bytes_discard(self.0);
    }
}
async fn load(url: String) -> Result<vmedia::DecodedImage, String> {
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    let bytes = if parsed.scheme() == "trueos"
        && parsed.host_str() == Some("solara")
        && matches!(parsed.path(), "/assets/cat.jpg" | "/assets/cat.jpeg")
    {
        include_bytes!("../assets/images/cat.jpg").to_vec()
    } else if matches!(parsed.scheme(), "http" | "https") {
        let fetch =
            Fetch(netfs::fetch_bytes(url.as_bytes()).map_err(|e| format!("fetch start: {e}"))?);
        let started = trueos::clock::monotonic_millis();
        poll_fn(|cx| match netfs::fetch_bytes_result_len(fetch.0) {
            Err(-8) if trueos::clock::monotonic_millis().saturating_sub(started) < 30_000 => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(format!("fetch: {e}"))),
            Ok(len) if len > 16 * 1024 * 1024 => Poll::Ready(Err("JPEG exceeds 16 MiB".into())),
            Ok(_) => Poll::Ready(
                netfs::fetch_bytes_read(fetch.0).map_err(|e| format!("fetch read: {e}")),
            ),
        })
        .await?
    } else {
        return Err("unsupported image URL".into());
    };
    vmedia::decode(vmedia::ImageFormat::Jpeg, &bytes)
        .await
        .map_err(|e| format!("kernel JPEG decode: {e}"))
}
struct Pending {
    url: String,
    future: Pin<Box<dyn Future<Output = Result<vmedia::DecodedImage, String>>>>,
}
pub(crate) struct Texture {
    pub buffer: Buffer,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
}
#[derive(Default)]
pub(crate) struct Images {
    pending: Vec<Pending>,
    pub textures: BTreeMap<String, Texture>,
}
impl Images {
    pub fn poll(
        &mut self,
        resources: &Resources,
        device: Device,
        layout: &mut SpecLayout,
    ) -> Result<bool, String> {
        while self.pending.len() < 2 {
            let Some(url) = resources
                .queued
                .lock()
                .map_err(|_| "resource lock")?
                .pop_front()
            else {
                break;
            };
            self.pending.push(Pending {
                future: Box::pin(load(url.clone())),
                url,
            });
        }
        let mut changed = false;
        let mut index = 0;
        while index < self.pending.len() {
            let result = self.pending[index]
                .future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
            let Poll::Ready(result) = result else {
                index += 1;
                continue;
            };
            let pending = self.pending.remove(index);
            match result {
                Ok(decoded) => {
                    let info = decoded.info;
                    let buffer = match device
                        .create_buffer(decoded.rgba.len(), vgpu::BUFFER_USAGE_MAP_WRITE)
                    {
                        Ok(buffer) => buffer,
                        Err(error) => {
                            crate::parser_probe::report_error(format_args!(
                                "solara: image-failed url={} allocation={error}",
                                pending.url
                            ));
                            continue;
                        }
                    };
                    match device.write_buffer(buffer, 0, &decoded.rgba) {
                        Ok(n) if n == decoded.rgba.len() => {}
                        result => {
                            let _ = device.destroy_buffer(buffer);
                            return Err(format!("image upload: {result:?}"));
                        }
                    }
                    layout.load_image(
                        pending.url.clone(),
                        info.width,
                        info.height,
                        Arc::new(decoded.rgba),
                    );
                    self.textures.insert(
                        pending.url.clone(),
                        Texture {
                            buffer,
                            width: info.width,
                            height: info.height,
                            pitch: info.stride_bytes,
                        },
                    );
                    crate::parser_probe::report_info(format_args!(
                        "solara: image-ready url={} size={}x{} backend={:?} texture_buffer={} decoder=kernel",
                        pending.url,
                        info.width,
                        info.height,
                        info.backend,
                        buffer.raw()
                    ));
                    changed = true;
                }
                Err(error) => crate::parser_probe::report_error(format_args!(
                    "solara: image-failed url={} error={error}",
                    pending.url
                )),
            }
        }
        Ok(changed)
    }
    pub fn release(&mut self, device: Device) {
        self.pending.clear();
        for (_, texture) in std::mem::take(&mut self.textures) {
            let _ = device.destroy_buffer(texture.buffer);
        }
    }
}
