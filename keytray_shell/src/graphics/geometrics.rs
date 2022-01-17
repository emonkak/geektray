#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect<P = f64, S = f64> {
    pub x: P,
    pub y: P,
    pub width: S,
    pub height: S,
}

impl<P, S> Rect<P, S> {
    pub fn new(position: Point<P>, size: Size<S>) -> Self {
        Self {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    }
}

impl Rect {
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
    pub fn contains(&self, point: PhysicalPoint) -> bool {
        self.x <= point.x
            && point.x <= self.x + self.width as i32
            && self.y <= point.y
            && point.y <= self.y + self.height as i32
    }

    pub fn contains_rect(&self, rect: PhysicalRect) -> bool {
        self.x <= rect.x + rect.width as i32
            || rect.x <= self.x + self.width as i32
            || self.y <= rect.y + rect.height as i32
            || rect.y <= self.y + self.height as i32
    }
}

pub type PhysicalRect = Rect<i32, u32>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T = f64> {
    pub x: T,
    pub y: T,
}

pub type PhysicalPoint = Point<i32>;

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size<T = f64> {
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
            width: self.width as f64,
            height: self.height as f64,
        }
    }
}
