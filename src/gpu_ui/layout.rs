//! Step 5: a minimal CSS-like box model and horizontal flex row layout.

#[derive(Clone, Copy, Debug, Default)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x
            && py >= self.y
            && px <= self.x + self.width
            && py <= self.y + self.height
    }
}

#[derive(Clone, Debug)]
pub struct BoxStyle {
    pub margin: Edges,
    pub padding: Edges,
    pub border: Edges,
    pub width: f32,
    pub height: f32,
}

impl Default for BoxStyle {
    fn default() -> Self {
        Self {
            margin: Edges::all(0.0),
            padding: Edges::all(12.0),
            border: Edges::all(2.0),
            width: 140.0,
            height: 44.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Button {
    pub label: String,
    pub fill: [f32; 4],
    pub style: BoxStyle,
    pub border_rect: Rect,
    pub content_rect: Rect,
}

impl Button {
    pub fn new(label: impl Into<String>, fill: [f32; 4]) -> Self {
        Self {
            label: label.into(),
            fill,
            style: BoxStyle::default(),
            border_rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            content_rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
        }
    }

    pub fn hit_test(&self, x: f32, y: f32) -> bool {
        self.border_rect.contains(x, y)
    }
}

pub struct FlexRow {
    pub origin: (f32, f32),
    pub gap: f32,
}

impl FlexRow {
    pub const fn new(origin: (f32, f32), gap: f32) -> Self {
        Self { origin, gap }
    }

    pub fn layout(&self, buttons: &mut [Button]) {
        let mut cursor_x = self.origin.0;

        for button in buttons {
            let style = &button.style;
            let outer_width = style.width
                + style.padding.horizontal()
                + style.border.horizontal()
                + style.margin.horizontal();
            let outer_height = style.height
                + style.padding.vertical()
                + style.border.vertical()
                + style.margin.vertical();

            let border_x = cursor_x + style.margin.left;
            let border_y = self.origin.1 + style.margin.top;

            button.border_rect = Rect {
                x: border_x,
                y: border_y,
                width: outer_width - style.margin.horizontal(),
                height: outer_height - style.margin.vertical(),
            };

            button.content_rect = Rect {
                x: border_x + style.border.left + style.padding.left,
                y: border_y + style.border.top + style.padding.top,
                width: style.width,
                height: style.height,
            };

            cursor_x += outer_width + self.gap;
        }
    }
}
