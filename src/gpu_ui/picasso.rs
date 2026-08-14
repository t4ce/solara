//! Solara paint records projected into the retained Picasso V0 scene.
//!
//! This is intentionally the compatibility subset: geometry is ordered before
//! the one retained FontKernel canvas. The SceneDB rows remain high-level; only
//! the final viewport crop expands rounded edges into bounded solid spans.

use trueos_helio_runtime::picasso_scene::{
    Color, CornerRadii, FontLookupRun, LowerError, LoweredCommand, MAX_LOWERED_COMMANDS,
    MAX_SCENE_PRIMITIVES, PicassoScene, Primitive, PrimitiveRef, PrimitiveRow, Rect, SceneError,
    Viewport,
};

use super::html::ScrollbarSide;
use super::shapes::{
    SHAPE_CIRCLE, SHAPE_RECT, SHAPE_ROUNDED_BORDER, SHAPE_ROUNDED_RECT, ShapeInstance,
};

const SCROLLBAR_TRACK_WIDTH: u32 = 10;
const SCROLLBAR_INSET: u32 = 2;
const SCROLLBAR_MIN_THUMB_HEIGHT: u32 = 36;
const SCROLLBAR_TRACK_ORDER: u32 = u32::MAX - 1;
const SCROLLBAR_THUMB_ORDER: u32 = u32::MAX;
const SCROLLBAR_TRACK_COLOR: Color = Color::rgba(20, 28, 40, 48);
const SCROLLBAR_THUMB_COLOR: Color = Color::rgba(46, 107, 219, 220);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuildError {
    Scene(SceneError),
    UnsupportedShape(u32),
}

impl From<SceneError> for BuildError {
    fn from(error: SceneError) -> Self {
        Self::Scene(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ViewportError {
    Scene(SceneError),
    Lower(LowerError),
}

impl From<SceneError> for ViewportError {
    fn from(error: SceneError) -> Self {
        Self::Scene(error)
    }
}

impl From<LowerError> for ViewportError {
    fn from(error: LowerError) -> Self {
        Self::Lower(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PublicationStats {
    pub epoch: u64,
    pub logical_rows: usize,
    pub shape_rows: usize,
    pub font_lookup_rows: usize,
}

pub(crate) struct PaintScene {
    scene: PicassoScene,
    scrollbar: ScrollbarScene,
    canvas: (u32, u32),
    scrollbar_side: ScrollbarSide,
}

impl PaintScene {
    pub(crate) const fn new() -> Self {
        Self {
            scene: PicassoScene::new(),
            scrollbar: ScrollbarScene::new(),
            canvas: (0, 0),
            scrollbar_side: ScrollbarSide::Right,
        }
    }

    /// Replace one coherent DOM-paint projection. The canvas backdrop and
    /// FontCanvas marker make the V0 ordering policy explicit rather than
    /// relying on a renderer-side shape/text split.
    pub(crate) fn rebuild(
        &mut self,
        shapes: &[ShapeInstance],
        text: &super::text::TextBatch,
        canvas: (u32, u32),
        backdrop: Color,
        scrollbar_side: ScrollbarSide,
    ) -> Result<PublicationStats, BuildError> {
        let font_lookup_rows = text
            .sections
            .iter()
            .filter(|section| text_section_is_retainable(section))
            .count();
        if shapes.len().saturating_add(font_lookup_rows) > MAX_SCENE_PRIMITIVES.saturating_sub(2) {
            return Err(BuildError::Scene(SceneError::PrimitiveLimit {
                limit: MAX_SCENE_PRIMITIVES,
            }));
        }
        let mut rows = Vec::with_capacity(
            shapes
                .len()
                .saturating_add(font_lookup_rows)
                .saturating_add(2),
        );
        rows.push(PrimitiveRow::new(
            0,
            Primitive::solid_rect(
                Rect::new(0.0, 0.0, canvas.0 as f32, canvas.1 as f32),
                backdrop,
            ),
        ));
        for (index, shape) in shapes.iter().enumerate() {
            let order = u32::try_from(index + 1).map_err(|_| {
                BuildError::Scene(SceneError::PrimitiveLimit {
                    limit: MAX_SCENE_PRIMITIVES,
                })
            })?;
            rows.push(PrimitiveRow::new(order, primitive_for_shape(shape)?));
        }
        let font_order = u32::try_from(rows.len()).map_err(|_| {
            BuildError::Scene(SceneError::PrimitiveLimit {
                limit: MAX_SCENE_PRIMITIVES,
            })
        })?;
        rows.push(PrimitiveRow::new(
            font_order,
            Primitive::font_canvas(Rect::new(0.0, 0.0, canvas.0 as f32, canvas.1 as f32)),
        ));
        for section in text
            .sections
            .iter()
            .filter(|section| text_section_is_retainable(section))
        {
            let order = u32::try_from(rows.len()).map_err(|_| {
                BuildError::Scene(SceneError::PrimitiveLimit {
                    limit: MAX_SCENE_PRIMITIVES,
                })
            })?;
            rows.push(PrimitiveRow::new(
                order,
                Primitive::font_lookup(FontLookupRun {
                    rect: Rect::new(section.x, section.y, section.width, section.height),
                    origin: [section.x, section.y],
                    text: section.text.clone(),
                    face: section.face,
                    slant: section.slant,
                    font_pixels: section.font_size,
                    color: Color::from_rgba_f32(section.color)?,
                }),
            ));
        }
        self.scene.replace_ordered(rows)?;
        self.canvas = canvas;
        self.scrollbar_side = scrollbar_side;
        let published = self.scene.publish();
        Ok(PublicationStats {
            epoch: published.epoch,
            logical_rows: published.live_count,
            shape_rows: shapes.len(),
            font_lookup_rows,
        })
    }

    pub(crate) fn lower(
        &mut self,
        origin: (u32, u32),
        viewport: (u32, u32),
    ) -> Result<Vec<LoweredCommand>, ViewportError> {
        if viewport.0 == 0 || viewport.1 == 0 {
            return Err(ViewportError::Lower(LowerError::InvalidViewport));
        }
        self.scrollbar
            .sync(self.scrollbar_side, origin.1, self.canvas.1, viewport)?;
        let mut overlay = self.scrollbar.scene.lower(
            Viewport::new(0.0, 0.0, viewport.0, viewport.1),
            MAX_LOWERED_COMMANDS,
        )?;
        let document_limit = MAX_LOWERED_COMMANDS.saturating_sub(overlay.len());
        let mut document = self.scene.lower(
            Viewport::new(origin.0 as f32, origin.1 as f32, viewport.0, viewport.1),
            document_limit,
        )?;
        document.append(&mut overlay);
        Ok(document)
    }

    pub(crate) fn font_lookup(&self, lookup: PrimitiveRef) -> Option<&FontLookupRun> {
        self.scene.font_lookup(lookup)
    }

    /// Copy the compact Unicode/style requests from the authoritative SceneDB
    /// publication for asynchronous FontKernel submission. No outline or
    /// coverage data is present in these rows.
    pub(crate) fn font_lookup_rows(&self) -> impl Iterator<Item = &FontLookupRun> {
        self.scene.font_lookup_rows().map(|(_, run)| run)
    }
}

fn text_section_is_retainable(section: &super::text::TextSection) -> bool {
    !section.text.is_empty()
        && section.x.is_finite()
        && section.y.is_finite()
        && section.width.is_finite()
        && section.width > 0.0
        && section.height.is_finite()
        && section.height > 0.0
        && section.font_size.is_finite()
        && section.font_size > 0.0
}

struct ScrollbarScene {
    scene: PicassoScene,
    track: Option<PrimitiveRef>,
    thumb: Option<PrimitiveRef>,
}

impl ScrollbarScene {
    const fn new() -> Self {
        Self {
            scene: PicassoScene::new(),
            track: None,
            thumb: None,
        }
    }

    fn sync(
        &mut self,
        side: ScrollbarSide,
        scroll_y: u32,
        content_height: u32,
        viewport: (u32, u32),
    ) -> Result<(), SceneError> {
        let geometry = scrollbar_geometry(side, scroll_y, content_height, viewport);
        let track_row = PrimitiveRow::new(
            SCROLLBAR_TRACK_ORDER,
            Primitive::solid_rect(geometry.track, SCROLLBAR_TRACK_COLOR),
        );
        let thumb_row = PrimitiveRow::new(
            SCROLLBAR_THUMB_ORDER,
            Primitive::rounded_rect(
                geometry.thumb,
                CornerRadii::all(geometry.thumb_radius),
                SCROLLBAR_THUMB_COLOR,
            ),
        );

        let Some((track, thumb)) = self.track.zip(self.thumb) else {
            let track = self.scene.insert(track_row)?;
            let thumb = match self.scene.insert(thumb_row) {
                Ok(thumb) => thumb,
                Err(error) => {
                    let _ = self.scene.remove(track);
                    return Err(error);
                }
            };
            self.track = Some(track);
            self.thumb = Some(thumb);
            let _ = self.scene.publish();
            return Ok(());
        };

        let mut changed = false;
        if self.scene.get(track) != Some(&track_row) {
            self.scene.update(track, track_row)?;
            changed = true;
        }
        if self.scene.get(thumb) != Some(&thumb_row) {
            self.scene.update(thumb, thumb_row)?;
            changed = true;
        }
        if changed {
            let _ = self.scene.publish();
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarGeometry {
    track: Rect,
    thumb: Rect,
    thumb_radius: f32,
}

fn scrollbar_geometry(
    side: ScrollbarSide,
    scroll_y: u32,
    content_height: u32,
    viewport: (u32, u32),
) -> ScrollbarGeometry {
    let track_width = SCROLLBAR_TRACK_WIDTH.min(viewport.0).max(1);
    let track_x = match side {
        ScrollbarSide::Left => 0,
        ScrollbarSide::Right => viewport.0.saturating_sub(track_width),
    };
    let horizontal_inset = SCROLLBAR_INSET.min(track_width.saturating_sub(1) / 2);
    let vertical_inset = SCROLLBAR_INSET.min(viewport.1.saturating_sub(1) / 2);
    let usable_height = viewport
        .1
        .saturating_sub(vertical_inset.saturating_mul(2))
        .max(1);
    let effective_extent = content_height.max(viewport.1).max(1);
    let proportional =
        (u64::from(usable_height) * u64::from(viewport.1) / u64::from(effective_extent)) as u32;
    let minimum = SCROLLBAR_MIN_THUMB_HEIGHT.min(usable_height);
    let thumb_height = proportional.clamp(minimum, usable_height);
    let max_scroll = content_height.saturating_sub(viewport.1);
    let travel = usable_height.saturating_sub(thumb_height);
    let thumb_offset = if max_scroll == 0 {
        0
    } else {
        (u64::from(travel) * u64::from(scroll_y.min(max_scroll)) / u64::from(max_scroll)) as u32
    };
    let thumb_width = track_width
        .saturating_sub(horizontal_inset.saturating_mul(2))
        .max(1);
    let thumb = Rect::new(
        track_x.saturating_add(horizontal_inset) as f32,
        vertical_inset.saturating_add(thumb_offset) as f32,
        thumb_width as f32,
        thumb_height as f32,
    );
    ScrollbarGeometry {
        track: Rect::new(
            track_x as f32,
            0.0,
            track_width as f32,
            viewport.1.max(1) as f32,
        ),
        thumb,
        thumb_radius: 3.0f32.min(thumb.width * 0.5).min(thumb.height * 0.5),
    }
}

fn primitive_for_shape(shape: &ShapeInstance) -> Result<Primitive, BuildError> {
    let rect = Rect::new(
        shape.pos_size[0],
        shape.pos_size[1],
        shape.pos_size[2],
        shape.pos_size[3],
    );
    let color = Color::from_rgba_f32(shape.color)?;
    match shape.shape_type {
        SHAPE_RECT => Ok(Primitive::solid_rect(rect, color)),
        SHAPE_CIRCLE => Ok(Primitive::rounded_rect(
            rect,
            CornerRadii::all(rect.width.min(rect.height) * 0.5),
            color,
        )),
        SHAPE_ROUNDED_RECT => Ok(Primitive::rounded_rect(
            rect,
            CornerRadii::all(shape.parameters[0]),
            color,
        )),
        SHAPE_ROUNDED_BORDER => Ok(Primitive::rounded_border(
            rect,
            CornerRadii::all(shape.parameters[0]),
            shape.parameters[1],
            color,
        )),
        unsupported => Err(BuildError::UnsupportedShape(unsupported)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_ui::geometry::Rect as SolaraRect;
    use crate::gpu_ui::html::{Document, RenderBatch, collect_batch};
    use crate::gpu_ui::text::{FontFace, FontSlant, TextBatch, queue_left_sized_with_font};
    use rust_qjs_dom::DomEngine;

    #[test]
    fn dom_shapes_publish_as_background_geometry_then_font() {
        let shapes = [
            ShapeInstance::rect(
                SolaraRect::new(4.0, 5.0, 20.0, 10.0),
                [0.25, 0.5, 0.75, 1.0],
            ),
            ShapeInstance::rounded_rect(
                SolaraRect::new(2.0, 2.0, 28.0, 18.0),
                5.0,
                [0.9, 0.9, 0.9, 1.0],
            ),
            ShapeInstance::rounded_border(
                SolaraRect::new(2.0, 2.0, 28.0, 18.0),
                5.0,
                2.0,
                [0.1, 0.2, 0.3, 1.0],
            ),
        ];
        let mut scene = PaintScene::new();
        let published = scene
            .rebuild(
                &shapes,
                &TextBatch::default(),
                (32, 24),
                Color::rgba(250, 250, 250, 255),
                ScrollbarSide::Right,
            )
            .unwrap();
        assert_eq!(published.logical_rows, 5);
        let commands = scene.lower((0, 0), (32, 24)).unwrap();
        assert!(matches!(
            commands.first(),
            Some(LoweredCommand::SolidSpan {
                order: 0,
                width: 32,
                height: 24,
                ..
            })
        ));
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, LoweredCommand::SolidSpan { order: 3, .. }))
        );
        let font_index = commands
            .iter()
            .position(|command| matches!(command, LoweredCommand::FontCanvas { order: 4, .. }))
            .expect("document FontCanvas remains ordered");
        assert!(
            commands[font_index + 1..]
                .iter()
                .all(|command| match command {
                    LoweredCommand::FontLookup { .. } => true,
                    LoweredCommand::SolidSpan { order, .. } => {
                        *order == SCROLLBAR_TRACK_ORDER || *order == SCROLLBAR_THUMB_ORDER
                    }
                    _ => false,
                })
        );
    }

    #[test]
    fn unknown_shape_kind_is_rejected_before_publication() {
        let mut shape =
            ShapeInstance::rect(SolaraRect::new(0.0, 0.0, 1.0, 1.0), [1.0, 1.0, 1.0, 1.0]);
        shape.shape_type = 99;
        let mut scene = PaintScene::new();
        assert_eq!(
            scene.rebuild(
                &[shape],
                &TextBatch::default(),
                (1, 1),
                Color::rgba(0, 0, 0, 255),
                ScrollbarSide::Right,
            ),
            Err(BuildError::UnsupportedShape(99))
        );
    }

    #[test]
    fn dom_shape_input_is_bounded_before_adapter_allocation() {
        let shape = ShapeInstance::rect(SolaraRect::new(0.0, 0.0, 1.0, 1.0), [1.0, 1.0, 1.0, 1.0]);
        let shapes = vec![shape; MAX_SCENE_PRIMITIVES - 1];
        let mut scene = PaintScene::new();
        assert_eq!(
            scene.rebuild(
                &shapes,
                &TextBatch::default(),
                (1, 1),
                Color::rgba(0, 0, 0, 255),
                ScrollbarSide::Right,
            ),
            Err(BuildError::Scene(SceneError::PrimitiveLimit {
                limit: MAX_SCENE_PRIMITIVES,
            }))
        );
    }

    #[test]
    fn bundled_html_crosses_dom_paint_scenedb_and_viewport_lowering() {
        let mut engine = DomEngine::new().expect("QuickJS DOM engine starts");
        let artifact = engine
            .parse(
                include_str!("../../docs/demoui.html"),
                "trueos://solara/docs/demoui.html",
            )
            .expect("bundled HTML parses");
        let document =
            Document::from_dom(artifact, engine, 960.0).expect("DOM adapts to Solara layout");
        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        assert!(!batch.shapes.is_empty());
        assert!(!batch.text.sections.is_empty());

        let canvas = (
            960,
            document.content_height.ceil().clamp(1.0, 4_096.0) as u32,
        );
        let mut scene = PaintScene::new();
        let published = scene
            .rebuild(
                batch.shapes.as_slice(),
                &batch.text,
                canvas,
                Color::rgba(250, 250, 250, 255),
                document.scrollbar_side(),
            )
            .expect("paint projection publishes");
        assert_eq!(published.shape_rows, batch.shapes.len());
        assert_eq!(
            published.font_lookup_rows,
            batch
                .text
                .sections
                .iter()
                .filter(|section| text_section_is_retainable(section))
                .count()
        );
        let commands = scene
            .lower((0, 0), (960, 720))
            .expect("first viewport lowers within UI4 cap");
        assert!(
            commands.len() <= 512,
            "bundled first viewport should remain a compact compatibility scene, got {} commands",
            commands.len(),
        );
        assert!(matches!(
            commands.first(),
            Some(LoweredCommand::SolidSpan { order: 0, .. })
        ));
        let font_index = commands
            .iter()
            .position(|command| matches!(command, LoweredCommand::FontCanvas { .. }))
            .expect("FontCanvas is present");
        assert!(font_index + 1 < commands.len());
        assert!(
            commands[font_index + 1..]
                .iter()
                .all(|command| match command {
                    LoweredCommand::FontLookup { .. } => true,
                    LoweredCommand::SolidSpan { order, .. } => {
                        *order == SCROLLBAR_TRACK_ORDER || *order == SCROLLBAR_THUMB_ORDER
                    }
                    _ => false,
                })
        );
    }

    #[test]
    fn scrollbar_tracks_both_edges_and_the_full_scroll_range() {
        let top = scrollbar_geometry(ScrollbarSide::Left, 0, 400, (100, 100));
        let middle = scrollbar_geometry(ScrollbarSide::Right, 150, 400, (100, 100));
        let bottom = scrollbar_geometry(ScrollbarSide::Right, 300, 400, (100, 100));
        assert_eq!(top.track.x, 0.0);
        assert_eq!(middle.track.x, 90.0);
        assert_eq!(bottom.track.x, 90.0);
        assert!(top.thumb.y < middle.thumb.y);
        assert!(middle.thumb.y < bottom.thumb.y);
        assert_eq!(bottom.thumb.y + bottom.thumb.height, 98.0);

        let no_overflow = scrollbar_geometry(ScrollbarSide::Right, 0, 80, (100, 100));
        assert_eq!(no_overflow.thumb.y, 2.0);
        assert_eq!(no_overflow.thumb.height, 96.0);
    }

    #[test]
    fn one_scroll_origin_moves_font_crop_and_fixed_scenedb_thumb_together() {
        let mut scene = PaintScene::new();
        scene
            .rebuild(
                &[],
                &TextBatch::default(),
                (100, 400),
                Color::rgba(250, 250, 250, 255),
                ScrollbarSide::Left,
            )
            .unwrap();
        let top = scene.lower((0, 0), (100, 100)).unwrap();
        let bottom = scene.lower((0, 300), (100, 100)).unwrap();
        let font_source_y = |commands: &[LoweredCommand]| {
            commands.iter().find_map(|command| match command {
                LoweredCommand::FontCanvas { source_y, .. } => Some(*source_y),
                _ => None,
            })
        };
        let thumb_top = |commands: &[LoweredCommand]| {
            commands
                .iter()
                .filter_map(|command| match command {
                    LoweredCommand::SolidSpan {
                        order: SCROLLBAR_THUMB_ORDER,
                        y,
                        ..
                    } => Some(*y),
                    _ => None,
                })
                .min()
        };
        assert_eq!(font_source_y(&top), Some(0));
        assert_eq!(font_source_y(&bottom), Some(300));
        assert!(thumb_top(&top) < thumb_top(&bottom));
    }

    #[test]
    fn unicode_font_lookup_preserves_style_color_and_order_with_canvas_fallback() {
        let mut text = TextBatch::default();
        queue_left_sized_with_font(
            &mut text,
            11.0,
            19.0,
            "SceneDB 你好",
            [0.25, 0.5, 0.75, 1.0],
            23.0,
            31.0,
            FontFace::NotoSansSc,
            FontSlant::Italic,
        );
        let mut scene = PaintScene::new();
        let publication = scene
            .rebuild(
                &[],
                &text,
                (320, 120),
                Color::rgba(250, 250, 250, 255),
                ScrollbarSide::Right,
            )
            .expect("Unicode lookup row publishes");
        assert_eq!(publication.font_lookup_rows, 1);
        let commands = scene.lower((0, 0), (320, 120)).unwrap();
        let canvas_index = commands
            .iter()
            .position(|command| matches!(command, LoweredCommand::FontCanvas { .. }))
            .expect("compatibility canvas remains present");
        let (lookup_index, lookup) = commands
            .iter()
            .enumerate()
            .find_map(|(index, command)| match command {
                LoweredCommand::FontLookup { lookup, .. } => Some((index, *lookup)),
                _ => None,
            })
            .expect("compact Unicode lookup survives lowering");
        let run = scene.font_lookup(lookup).expect("lookup handle resolves");
        assert_eq!(run.text, "SceneDB 你好");
        assert_eq!(run.face, FontFace::NotoSansSc);
        assert_eq!(run.slant, FontSlant::Italic);
        assert_eq!(run.font_pixels, 23.0);
        assert_eq!(run.origin, [11.0, 19.0]);
        assert_eq!(run.color, Color::rgba(64, 128, 191, 255));
        assert!(canvas_index < lookup_index);
    }

    #[test]
    fn dom_css_typography_survives_into_ordered_scenedb_lookup_rows() {
        let mut engine = DomEngine::new().expect("QuickJS DOM engine starts");
        let artifact = engine
            .parse(
                r#"
                    <style>
                      #first { color: rgb(10, 20, 30); font-family: "Noto Sans SC"; font-size: 19px; font-style: italic; }
                      #second { color: rgb(40, 50, 60); font-family: monospace; font-size: 13px; }
                    </style>
                    <main><p id="first">你好</p><p id="second">second</p></main>
                "#,
                "https://solara.test/font-lookup",
            )
            .expect("DOM parses");
        let document = Document::from_dom(artifact, engine, 320.0).expect("DOM adapts");
        let mut batch = RenderBatch::default();
        collect_batch(&document, 1.0, &mut batch);
        let mut scene = PaintScene::new();
        scene
            .rebuild(
                &batch.shapes,
                &batch.text,
                (320, document.content_height.ceil() as u32),
                Color::rgba(255, 255, 255, 255),
                ScrollbarSide::Right,
            )
            .expect("DOM scene publishes");
        let rows = scene.font_lookup_rows().collect::<Vec<_>>();
        let first = rows.iter().position(|row| row.text == "你好").unwrap();
        let second = rows.iter().position(|row| row.text == "second").unwrap();
        assert!(first < second, "DOM paint order is retained");
        assert_eq!(rows[first].face, FontFace::NotoSansSc);
        assert_eq!(rows[first].slant, FontSlant::Italic);
        assert_eq!(rows[first].font_pixels, 19.0);
        assert_eq!(rows[first].color, Color::rgba(10, 20, 30, 255));
        assert_eq!(rows[second].face, FontFace::Inconsolata);
        assert_eq!(rows[second].slant, FontSlant::Normal);
        assert_eq!(rows[second].font_pixels, 13.0);
        assert_eq!(rows[second].color, Color::rgba(40, 50, 60, 255));
    }
}
