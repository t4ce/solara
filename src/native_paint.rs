//! First native view: the resolved Blitz boxes and Parley glyphs as geometry.
//! This is intentionally a diagnostic text/contour view, not full CSS painting.
use std::collections::BTreeMap;

use blitz_dom::{BaseDocument, NodeId};
use parley::PositionedLayoutItem;
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{NormalizedCoord, Size},
    outline::{DrawSettings, OutlinePen},
};

#[path = "path_mesh.rs"]
mod path_mesh;
use path_mesh::{FillOptions, Path, PathBuilder, VertexBuffers, point};

#[derive(Default)]
pub struct PageMesh {
    pub vertices: Vec<[f32; 2]>,
    pub triangles: Vec<u32>,
    pub lines: Vec<u32>,
    pub images: Vec<ImageQuad>,
    pub boxes: usize,
    pub glyphs: usize,
    pub height: f32,
}

/// One image's content quad in document coordinates, with cropped UVs.
#[derive(Clone, Debug)]
pub struct ImageQuad {
    pub url: String,
    pub corners: [[f32; 2]; 4],
    pub uv: [[f32; 2]; 4],
}

impl ImageQuad {
    pub fn visible(&self, width: f32, height: f32, scroll_y: f32) -> bool {
        self.corners.iter().any(|p| p[0] >= 0.0)
            && self.corners.iter().any(|p| p[0] <= width)
            && self.corners.iter().any(|p| p[1] >= scroll_y)
            && self.corners.iter().any(|p| p[1] <= scroll_y + height)
    }
}

/// JPEG selection uses the URL path, so query strings and fragments are harmless.
pub fn jpeg_url(url: &url::Url) -> bool {
    url.path()
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"))
}

impl PageMesh {
    /// Retain the whole document on the CPU, but submit only primitives touching
    /// this viewport. Compact line vertices first: the native broker materializes
    /// each draw's indexed vertex prefix, so interleaving text and lines wastes DMA.
    pub fn viewport(&self, width: f32, height: f32, scroll_y: f32) -> Self {
        let mut out = Self::default();
        let mut remap = vec![u32::MAX; self.vertices.len()];
        for (source, count, lines) in [(&self.lines, 2, true), (&self.triangles, 3, false)] {
            for primitive in source.chunks_exact(count) {
                let mut low = [f32::INFINITY; 2];
                let mut high = [f32::NEG_INFINITY; 2];
                for index in primitive {
                    let p = self.vertices[*index as usize];
                    for axis in 0..2 {
                        low[axis] = low[axis].min(p[axis]);
                        high[axis] = high[axis].max(p[axis]);
                    }
                }
                if high[0] < -2.0
                    || low[0] > width + 2.0
                    || high[1] < scroll_y - 2.0
                    || low[1] > scroll_y + height + 2.0
                {
                    continue;
                }
                for index in primitive {
                    let slot = &mut remap[*index as usize];
                    if *slot == u32::MAX {
                        *slot = out.vertices.len() as u32;
                        out.vertices.push(self.vertices[*index as usize]);
                    }
                    if lines {
                        out.lines.push(*slot);
                    } else {
                        out.triangles.push(*slot);
                    }
                }
            }
        }
        out
    }
}

#[derive(Default)]
pub struct Painter {
    glyphs: BTreeMap<(u64, u32, u32, Vec<i16>, u32), VertexBuffers>,
}

impl Painter {
    pub fn cached_glyphs(&self) -> usize {
        self.glyphs.len()
    }

    pub fn paint(&mut self, doc: &BaseDocument) -> Result<PageMesh, String> {
        let mut mesh = PageMesh::default();
        for (_, node) in doc.tree().iter() {
            if !node.flags.is_in_document() || node.element_data().is_none() {
                continue;
            }
            let Some(style) = node.primary_styles() else {
                continue;
            };
            if style.clone_display().is_none() || style.get_effects().opacity == 0.0 {
                continue;
            }
            if matches!(
                doc.resolved_style_value(node.id, "visibility").as_str(),
                "hidden" | "collapse"
            ) {
                continue;
            }
            let mut ancestor = node.layout_parent.get();
            let mut hidden = false;
            while let Some(id) = ancestor {
                let Some(parent) = doc.get_node(id) else {
                    break;
                };
                if parent
                    .primary_styles()
                    .is_some_and(|s| s.clone_display().is_none() || s.get_effects().opacity == 0.0)
                {
                    hidden = true;
                    break;
                }
                ancestor = parent.layout_parent.get();
            }
            if hidden {
                continue;
            }
            let layout = node.final_layout();
            let transform = world_transform(doc, node.id);
            if layout.size.width > 0.0 && layout.size.height > 0.0 {
                let base = checked_base(&mesh, 4)?;
                for p in [
                    [0.0, 0.0],
                    [layout.size.width, 0.0],
                    [layout.size.width, layout.size.height],
                    [0.0, layout.size.height],
                ] {
                    let p = transform_point(transform, p);
                    mesh.height = mesh.height.max(p[1]);
                    mesh.vertices.push(p);
                }
                mesh.lines.extend([
                    base,
                    base + 1,
                    base + 1,
                    base + 2,
                    base + 2,
                    base + 3,
                    base + 3,
                    base,
                ]);
                mesh.boxes += 1;
            }
            if let Some(element) = node.element_data()
                && element.name.local.as_ref() == "img"
                && let Some(image) = element.raster_image_data()
                && let Some(src) = element
                    .attrs()
                    .iter()
                    .find(|a| a.name.local.as_ref() == "src")
                    .map(|a| a.value.as_str())
                && let Ok(url) = doc.base_url().join(src)
                && jpeg_url(&url)
            {
                let x = layout.border.left + layout.padding.left;
                let y = layout.border.top + layout.padding.top;
                let w = layout.size.width - x - layout.border.right - layout.padding.right;
                let h = layout.size.height - y - layout.border.bottom - layout.padding.bottom;
                if w > 0.0 && h > 0.0 && image.width > 0 && image.height > 0 {
                    let iw = image.width as f32;
                    let ih = image.height as f32;
                    let fit = doc.resolved_style_value(node.id, "object-fit");
                    let scale = match fit.as_str() {
                        "contain" => (w / iw).min(h / ih),
                        "cover" => (w / iw).max(h / ih),
                        "none" => 1.0,
                        "scale-down" => (w / iw).min(h / ih).min(1.0),
                        _ => 0.0,
                    };
                    let (dw, dh) = if scale == 0.0 {
                        (w, h)
                    } else {
                        (iw * scale, ih * scale)
                    };
                    // Computed object-position is a pair of percentages/lengths.
                    let position = doc.resolved_style_value(node.id, "object-position");
                    let mut parts = position.split_whitespace();
                    let offset = |part: Option<&str>, remaining: f32| -> f32 {
                        let part = part.unwrap_or("50%");
                        if let Some(p) = part.strip_suffix('%').and_then(|v| v.parse::<f32>().ok())
                        {
                            remaining * p / 100.0
                        } else {
                            part.strip_suffix("px")
                                .and_then(|v| v.parse().ok())
                                .unwrap_or(remaining * 0.5)
                        }
                    };
                    let dx = offset(parts.next(), w - dw);
                    let dy = offset(parts.next(), h - dh);
                    let left = dx.max(0.0);
                    let top = dy.max(0.0);
                    let right = (dx + dw).min(w);
                    let bottom = (dy + dh).min(h);
                    if right > left && bottom > top {
                        mesh.images.push(ImageQuad {
                            url: url.into(),
                            corners: [
                                [x + left, y + top],
                                [x + right, y + top],
                                [x + right, y + bottom],
                                [x + left, y + bottom],
                            ]
                            .map(|p| transform_point(transform, p)),
                            uv: [
                                [(left - dx) / dw, (top - dy) / dh],
                                [(right - dx) / dw, (top - dy) / dh],
                                [(right - dx) / dw, (bottom - dy) / dh],
                                [(left - dx) / dw, (bottom - dy) / dh],
                            ],
                        });
                    }
                }
            }
            if !node.flags.is_inline_root() {
                continue;
            }
            let Some(text) = node
                .element_data()
                .and_then(|e| e.inline_layout_data.as_ref())
            else {
                continue;
            };
            let origin = [
                layout.border.left + layout.padding.left,
                layout.border.top + layout.padding.top,
            ];
            for line in text.layout.lines() {
                for item in line.items() {
                    let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                        continue;
                    };
                    let run = glyph_run.run();
                    let font = run.font();
                    let skew = run.synthesis().skew().map_or(0.0, |v| v.to_radians().tan());
                    for glyph in glyph_run.positioned_glyphs() {
                        let key = (
                            font.data.id(),
                            font.index,
                            run.font_size().to_bits(),
                            run.normalized_coords().to_vec(),
                            glyph.id,
                        );
                        if !self.glyphs.contains_key(&key) {
                            let face = FontRef::from_index(font.data.as_ref(), font.index)
                                .map_err(|e| format!("font: {e}"))?;
                            let mut pen = Pen(Path::builder_with_options(&FillOptions::DEFAULT));
                            if let Some(outline) = face.outline_glyphs().get(GlyphId::new(glyph.id))
                            {
                                let coords: Vec<_> = run
                                    .normalized_coords()
                                    .iter()
                                    .map(|v| NormalizedCoord::from_bits(*v))
                                    .collect();
                                outline
                                    .draw(
                                        DrawSettings::unhinted(
                                            Size::new(run.font_size()),
                                            coords.as_slice(),
                                        ),
                                        &mut pen,
                                    )
                                    .map_err(|e| format!("glyph {}: {e}", glyph.id))?;
                            }
                            let geometry = pen
                                .0
                                .build()
                                .tessellate(&FillOptions::DEFAULT)
                                .map_err(|e| e.reason().to_owned())?;
                            self.glyphs.insert(key.clone(), geometry);
                        }
                        let geometry = &self.glyphs[&key];
                        let base = checked_base(&mesh, geometry.vertices.len())?;
                        for p in &geometry.vertices {
                            // Outline coordinates are Y-up; Parley's positioned glyph is its baseline.
                            mesh.vertices.push(transform_point(
                                transform,
                                [
                                    origin[0] + glyph.x + p[0] + skew * p[1],
                                    origin[1] + glyph.y - p[1],
                                ],
                            ));
                        }
                        mesh.triangles
                            .extend(geometry.indices.iter().map(|i| base + i));
                        mesh.glyphs += 1;
                    }
                }
            }
        }
        if mesh.vertices.iter().flatten().any(|v| !v.is_finite()) {
            return Err("non-finite page geometry".into());
        }
        Ok(mesh)
    }
}

fn checked_base(mesh: &PageMesh, count: usize) -> Result<u32, String> {
    // Keep allocations bounded even for untrusted documents; fail visibly, never truncate a page.
    if mesh.vertices.len().saturating_add(count) > 4_000_000 {
        return Err("page geometry exceeds four million vertices".into());
    }
    u32::try_from(mesh.vertices.len()).map_err(|_| "page index overflow".into())
}

type Matrix = [f64; 6];
fn transform_point(m: Matrix, p: [f32; 2]) -> [f32; 2] {
    let [x, y] = p.map(f64::from);
    [
        (m[0] * x + m[2] * y + m[4]) as f32,
        (m[1] * x + m[3] * y + m[5]) as f32,
    ]
}
fn multiply(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}
fn world_transform(doc: &BaseDocument, id: NodeId) -> Matrix {
    let mut chain = Vec::new();
    let mut current = Some(id);
    while let Some(id) = current {
        let Some(node) = doc.get_node(id) else {
            break;
        };
        let location = node.final_layout().location;
        let translation = [1.0, 0.0, 0.0, 1.0, location.x as f64, location.y as f64];
        let local = node
            .transform()
            .as_deref()
            .map_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], |m| m.as_coeffs());
        chain.push(multiply(translation, local));
        current = node.layout_parent.get();
    }
    chain
        .into_iter()
        .rev()
        .fold([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], multiply)
}

struct Pen(PathBuilder);
impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.begin(point(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(point(x, y));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.0.quadratic_bezier_to(point(cx, cy), point(x, y));
    }
    fn curve_to(&mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) {
        self.0
            .cubic_bezier_to(point(ax, ay), point(bx, by), point(x, y));
    }
    fn close(&mut self) {
        self.0.close();
    }
}
