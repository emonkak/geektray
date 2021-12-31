use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{Point, Size};
use crate::paint_context::PaintContext;

pub trait Widget {
    fn render(&mut self, position: Point, layout: &LayoutResult, context: &mut PaintContext);

    fn layout(&mut self, container_size: Size) -> LayoutResult;

    fn on_event(
        &mut self,
        _display: *mut xlib::Display,
        _window: xlib::Window,
        _event: &X11Event,
        _position: Point,
        _layout: &LayoutResult,
    ) -> SideEffect {
        SideEffect::None
    }
}

#[repr(usize)]
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum SideEffect {
    None,
    RequestRedraw,
    RequestLayout,
}

impl SideEffect {
    pub fn compose(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, x) | (x, Self::None) => x,
            (Self::RequestRedraw, _) | (_, Self::RequestRedraw) => Self::RequestRedraw,
            (Self::RequestLayout, _) => Self::RequestLayout,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LayoutResult {
    pub size: Size,
    pub children: Vec<(Point, LayoutResult)>,
}
