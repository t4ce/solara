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
    pub artifact: Option<rust_qjs_dom::DomArtifact>,
    pub favicon: Option<(String, Vec<String>)>,
    queued: Mutex<VecDeque<String>>,
    seen: Mutex<BTreeSet<String>>,
    requests: Mutex<VecDeque<(String, Box<dyn NetHandler>)>>,
    fetching: Mutex<Vec<PendingResource>>,
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
            if let Ok(css) =
                crate::parser_probe::load_embedded_stylesheet("", None, request.url.path())
            {
                handler.bytes(request.url.to_string(), Bytes::from(css.css));
            } else if matches!(request.url.scheme(), "http" | "https") {
                self.requests
                    .lock()
                    .expect("resource lock")
                    .push_back((request.url.into(), handler));
            } else {
                handler.bytes(request.url.into(), Bytes::new());
            }
        }
    }
}
struct PendingResource {
    url: String,
    handler: Box<dyn NetHandler>,
    future: Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send>>,
}
impl Resources {
    pub fn poll(&self) -> Result<bool, String> {
        let mut fetching = self.fetching.lock().map_err(|_| "resource lock")?;
        while fetching.len() < 2 {
            let Some((url, handler)) = self
                .requests
                .lock()
                .map_err(|_| "resource lock")?
                .pop_front()
            else {
                break;
            };
            fetching.push(PendingResource {
                future: Box::pin(fetch_bytes(url.clone())),
                url,
                handler,
            });
        }
        let mut changed = false;
        let mut index = 0;
        while index < fetching.len() {
            let result = fetching[index]
                .future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
            let Poll::Ready(result) = result else {
                index += 1;
                continue;
            };
            let request = fetching.remove(index);
            let bytes = match result {
                Ok(bytes) => bytes,
                Err(error) => {
                    crate::parser_probe::report_error(format_args!(
                        "solara: resource-failed url={} error={error}",
                        request.url
                    ));
                    Vec::new()
                }
            };
            request.handler.bytes(request.url, Bytes::from(bytes));
            changed = true;
        }
        Ok(changed)
    }
}

struct Fetch(u32);
impl Drop for Fetch {
    fn drop(&mut self) {
        let _ = netfs::fetch_bytes_discard(self.0);
    }
}
/// Poll the kernel's HTTP/HTTPS operation; dropping discards its result slot.
/// The current GET ABI may finish transport in the background after discard.
pub(crate) async fn fetch_bytes(url: String) -> Result<Vec<u8>, String> {
    fetch_bytes_limited(url, 8 * 1024 * 1024).await
}
pub(crate) async fn fetch_bytes_limited(url: String, max_bytes: usize) -> Result<Vec<u8>, String> {
    let fetch = Fetch(netfs::fetch_bytes(url.as_bytes()).map_err(|e| format!("fetch start: {e}"))?);
    let started = trueos::clock::monotonic_millis();
    poll_fn(|cx| match netfs::fetch_bytes_result_len(fetch.0) {
        Err(-8) if trueos::clock::monotonic_millis().saturating_sub(started) < 46_000 => {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
        Err(e) => Poll::Ready(Err(format!("fetch: {e}"))),
        Ok(len) if len > max_bytes => Poll::Ready(Err("resource exceeds byte limit".into())),
        Ok(_) => {
            Poll::Ready(netfs::fetch_bytes_read(fetch.0).map_err(|e| format!("fetch read: {e}")))
        }
    })
    .await
}
async fn load(url: String) -> Result<vmedia::DecodedImage, String> {
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    let bytes = if parsed.scheme() == "trueos"
        && parsed.host_str() == Some("solara")
        && matches!(parsed.path(), "/assets/cat.jpg" | "/assets/cat.jpeg")
    {
        include_bytes!("../assets/images/cat.jpg").to_vec()
    } else if url == "trueos://solara/assets/logo.jpg" {
        include_bytes!("../assets/images/logo.jpg").to_vec()
    } else if matches!(parsed.scheme(), "http" | "https") {
        fetch_bytes(url.clone()).await?
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
                    // Sample the decoded buffer before upload so a partial decode
                    // can be distinguished from a texture/rendering failure.
                    let pixel = |y: u32| {
                        let offset =
                            y as usize * info.stride_bytes as usize + info.width as usize / 2 * 4;
                        decoded.rgba.get(offset..offset + 4).unwrap_or(&[])
                    };
                    let _ = trueos::logl::log_record(
                        trueos::logl::level::DEBUG,
                        "blueprint",
                        format_args!(
                            "solara: image-decoded url={} size={}x{} backend={:?} center_rgba_top={:?} center_rgba_middle={:?} center_rgba_bottom={:?}",
                            pending.url,
                            info.width,
                            info.height,
                            info.backend,
                            pixel(0),
                            pixel(info.height / 2),
                            pixel(info.height.saturating_sub(1))
                        ),
                    );
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
