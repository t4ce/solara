#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px <= self.x + self.width && py <= self.y + self.height
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn intersection(&self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right > x && bottom > y).then(|| Self::new(x, y, right - x, bottom - y))
    }

    pub fn inset(&self, padding: f32) -> Self {
        Self {
            x: self.x + padding,
            y: self.y + padding,
            width: (self.width - padding * 2.0).max(0.0),
            height: (self.height - padding * 2.0).max(0.0),
        }
    }
}

pub const PAGE_PAD: f32 = 16.0;
pub const BLOCK_GAP: f32 = 8.0;
pub const TEXT_LINE: f32 = 18.0;
pub const CONTROL_H: f32 = 28.0;
pub const IFRAME_INSET: f32 = 8.0;
pub const IFRAME_HEADER_H: f32 = 24.0;

pub fn iframe_viewport(frame: Rect) -> Rect {
    Rect::new(
        frame.x + IFRAME_INSET,
        frame.y + IFRAME_HEADER_H,
        (frame.width - IFRAME_INSET * 2.0).max(0.0),
        (frame.height - IFRAME_HEADER_H - IFRAME_INSET).max(0.0),
    )
}

// All layout constants above are in logical points (CSS px at 1x).
// Multiply by window scale_factor when emitting GPU instances on HiDPI displays.
