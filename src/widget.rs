use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{Point, Rect, Size};
use crate::render_context::RenderContext;

pub trait Widget {
    fn render(&mut self, bounds: Rect, context: &mut RenderContext);

    fn layout(&mut self, container_size: Size) -> Size;

    fn on_event(
        &mut self,
        _display: *mut xlib::Display,
        _window: xlib::Window,
        _event: &X11Event,
        _bounds: Rect,
    ) -> SideEffect {
        SideEffect::None
    }
}

#[derive(Debug)]
pub struct WidgetPod<Widget> {
    pub widget: Widget,
    pub bounds: Rect,
}

impl<Widget: self::Widget> WidgetPod<Widget> {
    pub fn new(widget: Widget) -> Self {
        Self {
            widget,
            bounds: Rect::default(),
        }
    }

    pub fn render(&mut self, context: &mut RenderContext) {
        self.widget.render(self.bounds, context);
    }

    pub fn layout(&mut self, container_size: Size) -> Size {
        let size = self.widget.layout(container_size);

        if self.bounds.width != size.width || self.bounds.height != size.height {
            self.bounds.width = size.width;
            self.bounds.height = size.height;
        }

        size
    }

    pub fn reposition(&mut self, position: Point) {
        if self.bounds.x != position.x || self.bounds.y != position.y {
            self.bounds.x = position.x;
            self.bounds.y = position.y;
        }
    }

    pub fn on_event(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        event: &X11Event,
    ) -> SideEffect {
        self.widget.on_event(display, window, event, self.bounds)
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
