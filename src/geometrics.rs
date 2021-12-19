#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect<T = f32> {
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size<T = f32> {
    pub width: T,
    pub height: T,
}

pub type PhysicalSize = Size<u32>;

impl Size {
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
