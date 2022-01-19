use crate::geometrics::{Point, Size};

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub size: Size,
    pub children: Vec<(Point, Layout)>,
}
