#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect<T = f32> {
    pub x: T,
    pub y: T,
    pub width: T,
    pub height: T,
}

impl Rect {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    pub fn new(position: Point, size: Size) -> Self {
        Self {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T = f32> {
    pub x: T,
    pub y: T,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size<T = f32> {
    pub width: T,
    pub height: T,
}

type PhysicalSize = Size<u32>;

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub fn snap(self) -> PhysicalSize {
        PhysicalSize {
            width: self.width as u32,
            height: self.height as u32,
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
