use std::borrow::Cow;

use super::font::FontDescription;

#[derive(Clone, Debug)]
pub struct Text {
    pub content: Cow<'static, str>,
    pub font_description: FontDescription,
    pub font_size: f64,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum HorizontalAlign {
    Left,
    Center,
    Right,
}
