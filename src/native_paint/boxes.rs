//! CSS box geometry and convex clipping for the native solid painter.
use super::*;

pub(super) fn canvas_background(doc: &BaseDocument) -> (u32, Option<NodeId>) {
    let Some(root) = doc.try_root_element() else {
        return (u32::MAX, None);
    };
    let body = root
        .children
        .iter()
        .filter_map(|id| doc.get_node(*id))
        .find(|n| {
            n.element_data()
                .is_some_and(|e| e.name.local.as_ref() == "body")
        });
    for node in std::iter::once(root).chain(body) {
        if let Some(style) = node.primary_styles() {
            let color = style
                .clone_background_color()
                .resolve_to_absolute(&style.clone_color())
                .to_nscolor();
            if color.to_le_bytes()[3] != 0 {
                return (color, Some(node.id));
            }
        }
    }
    (u32::MAX, None)
}

pub(super) type Clip = Vec<[f32; 2]>;
pub(super) fn ancestor_clips(doc: &BaseDocument, id: NodeId) -> Vec<Clip> {
    let mut clips = Vec::new();
    let original = id;
    let mut current = Some(id);
    while let Some(id) = current {
        let Some(node) = doc.get_node(id) else {
            break;
        };
        let x = doc.resolved_style_value(id, "overflow-x");
        let y = doc.resolved_style_value(id, "overflow-y");
        let clip_x = matches!(x.as_str(), "hidden" | "clip" | "scroll" | "auto");
        let clip_y = matches!(y.as_str(), "hidden" | "clip" | "scroll" | "auto");
        if id != original && (clip_x || clip_y) {
            let layout = node.final_layout();
            let [left, right] = if clip_x {
                [layout.border.left, layout.size.width - layout.border.right]
            } else {
                [-1e9, 1e9]
            };
            let [top, bottom] = if clip_y {
                [layout.border.top, layout.size.height - layout.border.bottom]
            } else {
                [-1e9, 1e9]
            };
            let transform = world_transform(doc, id);
            clips.push(
                [[left, top], [right, top], [right, bottom], [left, bottom]]
                    .map(|p| transform_point(transform, p))
                    .to_vec(),
            );
        }
        let legacy = doc.resolved_style_value(id, "clip");
        if matches!(
            doc.resolved_style_value(id, "position").as_str(),
            "absolute" | "fixed"
        ) && let Some(rect) = legacy
            .strip_prefix("rect(")
            .and_then(|v| v.strip_suffix(')'))
        {
            let parts: Vec<_> = rect
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|s| !s.is_empty())
                .collect();
            if parts.len() == 4 {
                let layout = node.final_layout();
                let side = |index: usize, auto: f32| {
                    parts[index]
                        .strip_suffix("px")
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(auto)
                };
                let [t, r, b, l] = [
                    side(0, 0.0),
                    side(1, layout.size.width),
                    side(2, layout.size.height),
                    side(3, 0.0),
                ];
                clips.push(
                    [[l, t], [r, t], [r, b], [l, b]]
                        .map(|p| transform_point(world_transform(doc, id), p))
                        .to_vec(),
                );
            }
        }
        current = node.layout_parent.get();
    }
    clips
}

pub(super) fn rounded_box(doc: &BaseDocument, id: NodeId, w: f32, h: f32) -> Vec<[f32; 2]> {
    let mut radii = [[0.0f32; 2]; 4];
    for (index, corner) in ["top-left", "top-right", "bottom-right", "bottom-left"]
        .iter()
        .enumerate()
    {
        let value = doc.resolved_style_value(id, &format!("border-{corner}-radius"));
        let mut parts = value.split_whitespace();
        let first = parts.next().unwrap_or("0px");
        for (axis, text) in [first, parts.next().unwrap_or(first)].iter().enumerate() {
            radii[index][axis] =
                if let Some(p) = text.strip_suffix('%').and_then(|p| p.parse::<f32>().ok()) {
                    p / 100.0 * [w, h][axis]
                } else {
                    text.strip_suffix("px")
                        .and_then(|p| p.parse::<f32>().ok())
                        .unwrap_or(0.0)
                }
                .max(0.0);
        }
    }
    if radii.iter().flatten().all(|v| *v == 0.0) {
        return vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
    }
    let factor = [
        w / (radii[0][0] + radii[1][0]),
        w / (radii[2][0] + radii[3][0]),
        h / (radii[0][1] + radii[3][1]),
        h / (radii[1][1] + radii[2][1]),
    ]
    .into_iter()
    .fold(1.0f32, f32::min);
    let mut points = Vec::new();
    for (index, [rx, ry]) in radii.map(|r| r.map(|v| v * factor)).into_iter().enumerate() {
        let center = [[rx, ry], [w - rx, ry], [w - rx, h - ry], [rx, h - ry]][index];
        for step in 0..=12 {
            let angle = std::f32::consts::PI
                + (index as f32 + step as f32 / 12.0) * std::f32::consts::FRAC_PI_2;
            points.push([center[0] + rx * angle.cos(), center[1] + ry * angle.sin()]);
        }
    }
    points
}

pub(super) fn append_polygon(
    mesh: &mut PageMesh,
    polygon: &[[f32; 2]],
    transform: Matrix,
    color: u32,
    clips: &[Clip],
) -> Result<(), String> {
    if color.to_le_bytes()[3] == 0 || polygon.len() < 3 {
        return Ok(());
    }
    let positions: Vec<_> = polygon
        .iter()
        .map(|p| transform_point(transform, *p))
        .collect();
    let mut indices = Vec::new();
    for i in 1..polygon.len() - 1 {
        indices.extend([0, i as u32, i as u32 + 1]);
    }
    append_geometry(mesh, &positions, &indices, color, clips)
}

pub(super) fn append_geometry(
    mesh: &mut PageMesh,
    positions: &[[f32; 2]],
    indices: &[u32],
    color: u32,
    clips: &[Clip],
) -> Result<(), String> {
    if color.to_le_bytes()[3] == 0 {
        return Ok(());
    }
    if clips.is_empty() {
        let base = checked_base(mesh, positions.len())?;
        mesh.vertices.extend_from_slice(positions);
        mesh.triangles.extend(indices.iter().map(|i| i + base));
        mesh.triangle_colors
            .extend(std::iter::repeat_n(color, indices.len() / 3));
    } else {
        for triangle in indices.chunks_exact(3) {
            let mut polygon: Vec<_> = triangle.iter().map(|i| positions[*i as usize]).collect();
            for clip in clips {
                polygon = clip_polygon(polygon, clip);
            }
            if polygon.len() < 3 {
                continue;
            }
            let base = checked_base(mesh, polygon.len())?;
            mesh.vertices.extend_from_slice(&polygon);
            for i in 1..polygon.len() - 1 {
                mesh.triangles
                    .extend([base, base + i as u32, base + i as u32 + 1]);
                mesh.triangle_colors.push(color);
            }
        }
    }
    Ok(())
}

/// Convex polygon clipping also handles transformed overflow rectangles.
fn clip_polygon(mut polygon: Vec<[f32; 2]>, clip: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let cross = |a: [f32; 2], b: [f32; 2], p: [f32; 2]| {
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    };
    let area: f32 = clip
        .iter()
        .zip(clip.iter().cycle().skip(1))
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    if area.abs() < 1e-6 {
        return Vec::new();
    }
    let sign = area.signum();
    for (a, b) in clip
        .iter()
        .zip(clip.iter().cycle().skip(1))
        .take(clip.len())
    {
        if polygon.is_empty() {
            break;
        }
        let source = std::mem::take(&mut polygon);
        let mut prev = *source.last().expect("nonempty polygon");
        let mut dprev = sign * cross(*a, *b, prev);
        for point in source {
            let d = sign * cross(*a, *b, point);
            if (d >= 0.0) != (dprev >= 0.0) {
                let t = dprev / (dprev - d);
                polygon.push([
                    prev[0] + t * (point[0] - prev[0]),
                    prev[1] + t * (point[1] - prev[1]),
                ]);
            }
            if d >= 0.0 {
                polygon.push(point);
            }
            prev = point;
            dprev = d;
        }
    }
    polygon
}
