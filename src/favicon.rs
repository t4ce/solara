//! Small shared favicon records and bounded ICO decoding, independent of UI4.
use rust_qjs_dom::DomNode;
use url::Url;

pub const SIDE: usize = 64;
pub const PIXEL_BYTES: usize = SIDE * SIDE * 4;
pub const MAX_RECORD_BYTES: usize = PIXEL_BYTES + 4096;
pub const MAX_ENCODED_BYTES: usize = 256 * 1024;
pub const CACHE_TTL_SECONDS: u64 = 7 * 24 * 60 * 60;
const MAGIC: &[u8; 8] = b"SOLICON1";

pub fn origin(url: &Url) -> Option<String> {
    matches!(url.scheme(), "http" | "https").then(|| url.origin().ascii_serialization())
}

/// Fixed slots bound disk use to approximately one MiB. The full origin in
/// each record makes a slot collision a miss, never another site's icon.
pub fn cache_path(origin: &str) -> String {
    let hash = origin.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    format!("apps/common/solara/favicons/{:02}.icon", hash % 64)
}

pub fn encode_record(origin: &str, unix: u64, pixels: &[u8]) -> Option<Vec<u8>> {
    if origin.len() > 2048 || pixels.len() != PIXEL_BYTES {
        return None;
    }
    let mut out = Vec::with_capacity(20 + origin.len() + pixels.len());
    out.extend(MAGIC);
    out.extend(unix.to_le_bytes());
    out.extend((origin.len() as u32).to_le_bytes());
    out.extend(origin.as_bytes());
    out.extend(pixels);
    Some(out)
}

pub fn decode_record(origin: &str, now: u64, record: &[u8]) -> Option<Vec<u8>> {
    if record.len() > MAX_RECORD_BYTES || record.get(..8)? != MAGIC {
        return None;
    }
    let saved = u64::from_le_bytes(record.get(8..16)?.try_into().ok()?);
    let len = u32::from_le_bytes(record.get(16..20)?.try_into().ok()?) as usize;
    if saved > now || now.saturating_sub(saved) > CACHE_TTL_SECONDS || len != origin.len() {
        return None;
    }
    if record.get(20..20 + len)? != origin.as_bytes() {
        return None;
    }
    let pixels = record.get(20 + len..)?;
    (pixels.len() == PIXEL_BYTES && pixels.chunks_exact(4).all(|p| p[3] == 255))
        .then(|| pixels.to_vec())
}

pub fn candidates(page: &Url, document: &DomNode) -> Vec<String> {
    fn walk(node: &DomNode, base: &Url, out: &mut Vec<String>) {
        if node.tag_name.as_deref() == Some("link") {
            let attr = |name: &str| node.attrs.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| a.value.as_str());
            if attr("rel").is_some_and(|rel| rel.split_ascii_whitespace().any(|v| v.eq_ignore_ascii_case("icon") || v.eq_ignore_ascii_case("apple-touch-icon"))) {
                if let Some(url) = attr("href").and_then(|href| base.join(href).ok()) {
                    if matches!(url.scheme(), "http" | "https") && out.len() < 8 && !out.contains(&url.to_string()) {
                        out.push(url.to_string());
                    }
                }
            }
        }
        for child in &node.children { walk(child, base, out); }
    }
    let mut out = Vec::new();
    walk(document, page, &mut out);
    if let Ok(url) = page.join("/favicon.ico") {
        if !out.contains(&url.to_string()) { out.push(url.to_string()); }
    }
    out
}

pub enum IconSource<'a> {
    Encoded(&'a [u8]),
    Rgba { width: u32, height: u32, pixels: Vec<u8> },
}

/// ICO can contain a PNG or a 32-bit uncompressed Windows DIB. Unsupported
/// palette formats are skipped so a declared PNG can still be tried next.
pub fn icon_source(bytes: &[u8]) -> Option<IconSource<'_>> {
    if bytes.len() > MAX_ENCODED_BYTES { return None; }
    if bytes.get(..4)? != [0, 0, 1, 0] { return Some(IconSource::Encoded(bytes)); }
    let u16_at = |i| Some(u16::from_le_bytes(bytes.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |i| Some(u32::from_le_bytes(bytes.get(i..i + 4)?.try_into().ok()?));
    let count = u16_at(4)? as usize;
    if count == 0 || count > 64 || bytes.len() < 6 + count * 16 { return None; }
    let mut entries: Vec<_> = (0..count).map(|i| {
        let offset = 6 + i * 16;
        let side = if bytes[offset] == 0 { 256 } else { bytes[offset] as u32 };
        (side.abs_diff(SIDE as u32), offset)
    }).collect();
    entries.sort_by_key(|e| e.0);
    for (_, entry) in entries {
        let len = u32_at(entry + 8)? as usize;
        let offset = u32_at(entry + 12)? as usize;
        let Some(image) = offset.checked_add(len).and_then(|end| bytes.get(offset..end)) else { continue; };
        if image.starts_with(b"\x89PNG\r\n\x1a\n") { return Some(IconSource::Encoded(image)); }
        if image.len() < 40 { continue; }
        let word = |i| u32::from_le_bytes(image[i..i+4].try_into().unwrap());
        let header = word(0) as usize;
        let width = word(4);
        let doubled_height = word(8);
        if header < 40 || width == 0 || width > 256 || doubled_height == 0 || doubled_height > 512 || doubled_height % 2 != 0 || image[12..16] != [1, 0, 32, 0] || word(16) != 0 { continue; }
        let height = doubled_height / 2;
        let size = width as usize * height as usize * 4;
        let Some(pixels) = image.get(header..header.saturating_add(size)) else { continue; };
        let mask_stride = ((width as usize + 31) / 32) * 4;
        let mask = image.get(header + size..header + size + mask_stride * height as usize);
        let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
        let mut rgba = Vec::with_capacity(size);
        for y in 0..height as usize {
            let source_y = height as usize - 1 - y;
            for x in 0..width as usize {
                let p = &pixels[(source_y * width as usize + x) * 4..][..4];
                let transparent = mask.is_some_and(|m| m[source_y * mask_stride + x / 8] & (0x80 >> (x % 8)) != 0);
                let alpha = if transparent { 0 } else if has_alpha { p[3] } else { 255 };
                rgba.extend([p[2], p[1], p[0], alpha]);
            }
        }
        return Some(IconSource::Rgba { width, height, pixels: rgba });
    }
    None
}

/// Fit the image into an opaque 64px tile, preserving aspect ratio.
pub fn tile(width: u32, height: u32, stride: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    let (width, height) = (width as usize, height as usize);
    if width == 0 || height == 0 || width > 1024 || height > 1024 || stride < width * 4 || rgba.len() < stride * height { return None; }
    let mut out = [13, 18, 27, 255].repeat(SIDE * SIDE);
    let scale = SIDE as f32 / width.max(height) as f32;
    let w = (width as f32 * scale).round().max(1.0) as usize;
    let h = (height as f32 * scale).round().max(1.0) as usize;
    for y in 0..h {
        for x in 0..w {
            let src = (y * height / h) * stride + (x * width / w) * 4;
            let dst = ((y + (SIDE - h) / 2) * SIDE + x + (SIDE - w) / 2) * 4;
            let alpha = rgba[src + 3] as u32;
            for channel in 0..3 {
                out[dst + channel] = ((rgba[src + channel] as u32 * alpha + out[dst + channel] as u32 * (255 - alpha) + 127) / 255) as u8;
            }
        }
    }
    Some(out)
}

pub fn fallback() -> Vec<u8> {
    let mut pixels = [13, 18, 27, 255].repeat(SIDE * SIDE);
    for y in 12..52 { for x in 12..52 {
        if y < 19 || y > 44 || (y < 32 && x < 19) || (y >= 32 && x > 44) || (29..35).contains(&y) {
            pixels[(y * SIDE + x) * 4..][..4].copy_from_slice(&[245, 182, 62, 255]);
        }
    }}
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_rejects_collision_expiry_and_truncation() {
        let pixels = fallback();
        let record = encode_record("https://example.org", 100, &pixels).unwrap();
        assert_eq!(decode_record("https://example.org", 101, &record), Some(pixels));
        assert!(decode_record("https://another.org", 101, &record).is_none());
        assert!(decode_record("https://example.org", 100 + CACHE_TTL_SECONDS + 1, &record).is_none());
        assert!(decode_record("https://example.org", 101, &record[..record.len()-1]).is_none());
        assert!(!cache_path("https://example.org/../../bad").contains(".."));
    }
    #[test]
    fn ico_bounds_and_aspect_ratio() {
        assert!(icon_source(&[0,0,1,0,255,255]).is_none());
        let tile = tile(2, 1, 8, &[255,0,0,255,0,255,0,255]).unwrap();
        assert_eq!(&tile[..4], &[13,18,27,255]);
        assert_eq!(&tile[(16*64)*4..][..4], &[255,0,0,255]);
        assert_eq!(&tile[(16*64+63)*4..][..4], &[0,255,0,255]);
    }
}
