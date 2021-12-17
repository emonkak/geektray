#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rectangle<T = f32> {
    pub x: T,
    pub y: T,
    pub width: T,
    pub height: T,
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

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };
}
