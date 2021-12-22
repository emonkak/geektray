#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect<P = f32, S = f32> {
    pub x: P,
    pub y: P,
    pub width: S,
    pub height: S,
}

impl Rect {
    pub fn snap(self) -> PhysicalRect {
        PhysicalRect {
            x: self.x.round() as i32,
            y: self.y.round() as i32,
            width: self.width.round() as u32,
            height: self.height.round() as u32,
        }
    }
}

impl PhysicalRect {
    pub fn contains(&self, point: PhysicalPoint) -> bool {
        self.x <= point.x
            && point.x <= self.x + self.width as i32
            && self.y <= point.y
            && point.y <= self.y + self.height as i32
    }
}

pub type PhysicalRect = Rect<i32, u32>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T = f32> {
    pub x: T,
    pub y: T,
}

pub type PhysicalPoint = Point<i32>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size<T = f32> {
    pub width: T,
    pub height: T,
}

pub type PhysicalSize = Size<u32>;

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
            width: self.width as f32,
            height: self.height as f32,
        }
    }
}
