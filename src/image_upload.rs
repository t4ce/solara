//! Bound browser image textures within the guest's shared GPU quota.
use std::borrow::Cow;

pub const PAGE_TEXTURE_BYTES: usize = 8 * 1024 * 1024;
const IMAGE_TEXTURE_BYTES: usize = 2 * 1024 * 1024;

pub struct Upload<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: Cow<'a, [u8]>,
}

/// Keep full-resolution CPU pixels/intrinsic layout; reduce only the GPU copy.
/// Box filtering averages premultiplied colors to avoid transparent-edge halos.
pub fn prepare(rgba: &[u8], width: u32, height: u32, available: usize) -> Option<Upload<'_>> {
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if width == 0 || height == 0 || rgba.len() != expected {
        return None;
    }
    let budget = available.min(IMAGE_TEXTURE_BYTES);
    if budget < 4 {
        return None;
    }
    let mut upload = Upload {
        width,
        height,
        rgba: Cow::Borrowed(rgba),
    };
    while upload.rgba.len() > budget {
        let w = upload.width.div_ceil(2);
        let h = upload.height.div_ceil(2);
        let mut output = vec![0; w as usize * h as usize * 4];
        for y in 0..h {
            for x in 0..w {
                let mut sums = [0u32; 4];
                let mut count = 0u32;
                for sy in y * 2..(y * 2 + 2).min(upload.height) {
                    for sx in x * 2..(x * 2 + 2).min(upload.width) {
                        let offset = (sy as usize * upload.width as usize + sx as usize) * 4;
                        let p = &upload.rgba[offset..offset + 4];
                        for c in 0..3 {
                            sums[c] += p[c] as u32 * p[3] as u32;
                        }
                        sums[3] += p[3] as u32;
                        count += 1;
                    }
                }
                let offset = (y as usize * w as usize + x as usize) * 4;
                for c in 0..3 {
                    output[offset + c] = if sums[3] == 0 {
                        0
                    } else {
                        ((sums[c] + sums[3] / 2) / sums[3]) as u8
                    };
                }
                output[offset + 3] = ((sums[3] + count / 2) / count) as u8;
            }
        }
        upload = Upload {
            width: w,
            height: h,
            rgba: Cow::Owned(output),
        };
    }
    Some(upload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_colors_do_not_bleed_into_reduced_texture() {
        let pixels = [255, 0, 0, 255, 0, 255, 0, 0];
        let upload = prepare(&pixels, 2, 1, 4).unwrap();
        assert_eq!((upload.width, upload.height), (1, 1));
        assert_eq!(&*upload.rgba, &[255, 0, 0, 128]);
    }

    #[test]
    fn odd_edges_and_small_images_preserve_pixels() {
        let pixels = [10, 20, 30, 255].repeat(15);
        let full = prepare(&pixels, 5, 3, 60).unwrap();
        assert!(matches!(full.rgba, Cow::Borrowed(_)));
        let small = prepare(&pixels, 5, 3, 24).unwrap();
        assert_eq!((small.width, small.height), (3, 2));
        assert_eq!(&*small.rgba, &[10, 20, 30, 255].repeat(6));
        assert!(prepare(&pixels, 5, 3, 0).is_none());
        assert!(prepare(&pixels[..59], 5, 3, 60).is_none());
        assert!(prepare(&[], 0, 3, 60).is_none());
    }

    #[test]
    fn four_k_texture_fits_browser_upload_budget() {
        let pixels = [10, 20, 30, 255].repeat(3840 * 2160);
        let upload = prepare(&pixels, 3840, 2160, PAGE_TEXTURE_BYTES).unwrap();
        assert_eq!((upload.width, upload.height), (960, 540));
        assert!(upload.rgba.len() <= IMAGE_TEXTURE_BYTES);
        assert!(upload.rgba.chunks_exact(4).all(|p| p == [10, 20, 30, 255]));
    }
}
