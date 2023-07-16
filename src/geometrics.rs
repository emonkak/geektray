pub type PhysicalRect = Rect<i32, u32>;

pub type PhysicalPoint = Point<i32>;

pub type PhysicalSize = Size<u32>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect<P = f64, S = f64> {
    pub x: P,
    pub y: P,
    pub width: S,
    pub height: S,
}

impl Rect {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    pub fn snap(&self) -> PhysicalRect {
        PhysicalRect {
            x: self.x.round() as i32,
            y: self.y.round() as i32,
            width: self.width.round() as u32,
            height: self.height.round() as u32,
        }
    }
}

impl PhysicalRect {
    pub fn contains_pos(&self, pos: PhysicalPoint) -> bool {
        self.x <= pos.x
            && pos.x <= self.x + self.width as i32
            && self.y <= pos.y
            && pos.y <= self.y + self.height as i32
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T = f64> {
    pub x: T,
    pub y: T,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size<T = f64> {
    pub width: T,
    pub height: T,
}

impl Size {
    pub fn snap(self) -> PhysicalSize {
        PhysicalSize {
            width: self.width.round() as u32,
            height: self.height.round() as u32,
        }
    }
}

impl PhysicalSize {
    pub fn unsnap(self) -> Size {
        Size {
            width: self.width as f64,
            height: self.height as f64,
        }
    }
}
