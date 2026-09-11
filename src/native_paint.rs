//! Native CSS solids and shaped text, in retained document paint order.
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

mod boxes;
use boxes::{ancestor_clips, append_geometry, append_polygon, canvas_background, rounded_box};

#[derive(Default)]
pub struct PageMesh {
    pub vertices: Vec<[f32; 2]>,
    pub triangles: Vec<u32>,
    /// Straight sRGBA per triangle. Adjacent equal colors may share a draw.
    pub triangle_colors: Vec<u32>,
    pub canvas_color: u32,
    pub images: Vec<ImageQuad>,
    pub boxes: usize,
    pub glyphs: usize,
    pub height: f32,
}

/// One image's content quad in document coordinates, with cropped UVs.
#[derive(Clone, Debug)]
pub struct ImageQuad {
    pub node_id: NodeId,
    pub url: String,
    pub corners: [[f32; 2]; 4],
    pub uv: [[f32; 2]; 4],
}

impl ImageQuad {
    /// Test the painted content quad, excluding object-fit letterboxing.
    pub fn contains(&self, point: [f32; 2]) -> bool {
        let mut positive = false;
        let mut negative = false;
        let mut area = 0.0;
        for i in 0..4 {
            let a = self.corners[i];
            let b = self.corners[(i + 1) % 4];
            let cross = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
            positive |= cross > 0.0;
            negative |= cross < 0.0;
            area += a[0] * b[1] - b[0] * a[1];
        }
        area.abs() > f32::EPSILON && !(positive && negative)
    }

    pub fn visible(&self, width: f32, height: f32, scroll_y: f32) -> bool {
        self.corners.iter().any(|p| p[0] >= 0.0)
            && self.corners.iter().any(|p| p[0] <= width)
            && self.corners.iter().any(|p| p[1] >= scroll_y)
            && self.corners.iter().any(|p| p[1] <= scroll_y + height)
    }
}

/// JPEG/PNG selection uses the URL path, so query strings and fragments are harmless.
pub fn raster_image_url(url: &url::Url) -> bool {
    url.path().rsplit_once('.').is_some_and(|(_, ext)| {
        ext.eq_ignore_ascii_case("jpg")
            || ext.eq_ignore_ascii_case("jpeg")
            || ext.eq_ignore_ascii_case("png")
    })
}

impl PageMesh {
    /// Use DOM hit testing for clipping/occlusion, then the painted image
    /// content for object-fit and CSS transforms. The point is in CSS pixels.
    pub fn image_at(&self, doc: &BaseDocument, point: [f32; 2]) -> Option<&ImageQuad> {
        let hit = doc.hit(point[0], point[1])?;
        self.images
            .iter()
            .rev()
            .find(|image| image.node_id == hit.node_id && image.contains(point))
    }

    /// Cull without sorting by color: overlapping boxes must keep painter order.
    pub fn viewport(&self, width: f32, height: f32, scroll_y: f32) -> Self {
        let mut out = Self {
            canvas_color: self.canvas_color,
            ..Self::default()
        };
        let mut remap = vec![u32::MAX; self.vertices.len()];
        for (primitive, color) in self
            .triangles
            .as_chunks::<3>()
            .0
            .iter()
            .zip(&self.triangle_colors)
        {
            let mut low = [f32::INFINITY; 2];
            let mut high = [f32::NEG_INFINITY; 2];
            for index in primitive {
                let p = self.vertices[*index as usize];
                for axis in 0..2 {
                    low[axis] = low[axis].min(p[axis]);
                    high[axis] = high[axis].max(p[axis]);
                }
            }
            if high[0] < 0.0 || low[0] > width || high[1] < scroll_y || low[1] > scroll_y + height {
                continue;
            }
            out.triangle_colors.push(*color);
            for index in primitive {
                let slot = &mut remap[*index as usize];
                if *slot == u32::MAX {
                    *slot = out.vertices.len() as u32;
                    out.vertices.push(self.vertices[*index as usize]);
                }
                out.triangles.push(*slot);
            }
        }
        out
    }

    /// Headless geometry evidence; images remain separate native textures.
    pub fn svg_preview(&self, width: u32, height: u32, scroll_y: f32) -> String {
        use std::fmt::Write;
        let view = self.viewport(width as f32, height as f32, scroll_y);
        let [r, g, b, a] = self.canvas_color.to_le_bytes();
        let mut svg = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' viewBox='0 {scroll_y} {width} {height}'><rect y='{scroll_y}' width='100%' height='100%' fill='rgb({r},{g},{b})' fill-opacity='{}'/>",
            a as f32 / 255.0
        );
        for (indices, color) in view.color_runs() {
            let [r, g, b, a] = color.to_le_bytes();
            write!(
                svg,
                "<path fill='rgb({r},{g},{b})' fill-opacity='{}' d='",
                a as f32 / 255.0
            )
            .unwrap();
            for t in indices.as_chunks::<3>().0 {
                let [a, b, c] = [
                    view.vertices[t[0] as usize],
                    view.vertices[t[1] as usize],
                    view.vertices[t[2] as usize],
                ];
                write!(
                    svg,
                    "M{:.3},{:.3} L{:.3},{:.3} {:.3},{:.3} Z ",
                    a[0], a[1], b[0], b[1], c[0], c[1]
                )
                .unwrap();
            }
            svg.push_str("'/>");
        }
        svg.push_str("</svg>");
        svg
    }

    pub fn color_runs(&self) -> impl Iterator<Item = (&[u32], u32)> {
        let mut start = 0;
        std::iter::from_fn(move || {
            let color = *self.triangle_colors.get(start)?;
            let mut end = start + 1;
            while self.triangle_colors.get(end) == Some(&color) {
                end += 1;
            }
            let indices = &self.triangles[start * 3..end * 3];
            start = end;
            Some((indices, color))
        })
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
        let (canvas_color, propagated_background) = canvas_background(doc);
        let mut mesh = PageMesh {
            canvas_color,
            ..PageMesh::default()
        };
        for (id, content) in paint_order(doc) {
            let Some(node) = doc.get_node(id) else {
                continue;
            };
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
            let clips = ancestor_clips(doc, node.id);
            if !content && layout.size.width > 0.0 && layout.size.height > 0.0 {
                let w = layout.size.width;
                let h = layout.size.height;
                for p in [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]] {
                    mesh.height = mesh.height.max(transform_point(transform, p)[1]);
                }
                let current = style.clone_color();
                if Some(node.id) != propagated_background {
                    let color = style
                        .clone_background_color()
                        .resolve_to_absolute(&current)
                        .to_nscolor();
                    append_polygon(
                        &mut mesh,
                        &rounded_box(doc, node.id, w, h),
                        transform,
                        color,
                        &clips,
                    )?;
                }
                let mut border_clips = clips.clone();
                border_clips.push(
                    rounded_box(doc, node.id, w, h)
                        .into_iter()
                        .map(|p| transform_point(transform, p))
                        .collect(),
                );
                let border = style.get_border();
                let [t, r, b, l] = [
                    layout.border.top,
                    layout.border.right,
                    layout.border.bottom,
                    layout.border.left,
                ];
                // Four trapezoids meet at the inner corners; CSS border widths
                // come from layout (none/hidden resolve to zero).
                for (points, color, width) in [
                    (
                        [[0.0, 0.0], [w, 0.0], [w - r, t], [l, t]],
                        &border.border_top_color,
                        t,
                    ),
                    (
                        [[w, 0.0], [w, h], [w - r, h - b], [w - r, t]],
                        &border.border_right_color,
                        r,
                    ),
                    (
                        [[w, h], [0.0, h], [l, h - b], [w - r, h - b]],
                        &border.border_bottom_color,
                        b,
                    ),
                    (
                        [[0.0, h], [0.0, 0.0], [l, t], [l, h - b]],
                        &border.border_left_color,
                        l,
                    ),
                ] {
                    if width > 0.0 {
                        append_polygon(
                            &mut mesh,
                            &points,
                            transform,
                            color.resolve_to_absolute(&current).to_nscolor(),
                            &border_clips,
                        )?;
                    }
                }
                mesh.boxes += 1;
            }
            if !content {
                continue;
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
                && raster_image_url(&url)
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
                            node_id: node.id,
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
            let element = node.element_data().expect("element checked above");
            let input = element.text_input_data();
            let text = if node.flags.is_inline_root() {
                element.inline_layout_data.as_ref().map(|text| &text.layout)
            } else {
                input.and_then(|input| input.editor.try_layout())
            };
            let Some(text) = text else {
                continue;
            };
            let mut origin = [
                layout.border.left + layout.padding.left,
                layout.border.top + layout.padding.top,
            ];
            if let Some(input) = input {
                origin[1] += node.text_input_v_centering_offset(1.0) as f32;
                origin[usize::from(input.is_multiline)] -= input.scroll_offset;
            }
            let mut text_clips = clips;
            if input.is_some()
                || ["overflow-x", "overflow-y"].iter().any(|property| {
                    matches!(
                        doc.resolved_style_value(node.id, property).as_str(),
                        "hidden" | "clip" | "auto" | "scroll"
                    )
                })
            {
                let [l, t, r, b] = [
                    layout.border.left,
                    layout.border.top,
                    layout.size.width - layout.border.right,
                    layout.size.height - layout.border.bottom,
                ];
                text_clips.push(
                    [[l, t], [r, t], [r, b], [l, b]]
                        .map(|p| transform_point(transform, p))
                        .to_vec(),
                );
            }
            for line in text.lines() {
                for item in line.items() {
                    let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                        continue;
                    };
                    let brush = glyph_run.style().brush.id;
                    if matches!(
                        doc.resolved_style_value(brush, "visibility").as_str(),
                        "hidden" | "collapse"
                    ) {
                        continue;
                    }
                    let color = doc
                        .get_node(brush)
                        .and_then(|node| node.primary_styles())
                        .map_or_else(
                            || style.clone_color().to_nscolor(),
                            |s| s.clone_color().to_nscolor(),
                        );
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
                        let positions: Vec<_> = geometry
                            .vertices
                            .iter()
                            .map(|p| {
                                transform_point(
                                    transform,
                                    [
                                        origin[0] + glyph.x + p[0] + skew * p[1],
                                        origin[1] + glyph.y - p[1],
                                    ],
                                )
                            })
                            .collect();
                        append_geometry(
                            &mut mesh,
                            &positions,
                            &geometry.indices,
                            color,
                            &text_clips,
                        )?;
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

/// Traverse Blitz's resolved paint children, including hoisted stacking contexts.
/// Slab allocation order is unrelated to DOM order after a mutation.
fn paint_order(doc: &BaseDocument) -> Vec<(NodeId, bool)> {
    let mut out = Vec::new();
    let mut pending: Vec<_> = doc
        .try_root_element()
        .map(|node| (node.id, false))
        .into_iter()
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    while let Some((id, content)) = pending.pop() {
        if !content && !seen.insert(id) {
            continue;
        }
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        out.push((id, content));
        if content {
            continue;
        }
        let mut children = Vec::new();
        if let Some(hoisted) = &node.stacking_context {
            children.extend(hoisted.neg_z_hoisted_children().map(|c| (c.node_id, false)));
        }
        // Negative stacking contexts sit above the parent's background but
        // below its inline content, even when no ordinary child precedes them.
        children.push((id, true));
        children.extend(
            node.paint_children
                .borrow()
                .iter()
                .flatten()
                .map(|id| (*id, false)),
        );
        if let Some(hoisted) = &node.stacking_context {
            children.extend(hoisted.pos_z_hoisted_children().map(|c| (c.node_id, false)));
        }
        pending.extend(children.into_iter().rev());
    }
    out
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
