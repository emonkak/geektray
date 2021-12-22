use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{Point, Rect, Size};
use crate::text_renderer::TextRenderer;

pub trait Widget {
    fn render(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        bounds: Rect,
        context: &mut RenderContext,
    );

    fn layout(&mut self, container_size: Size) -> Size;

    fn on_event(
        &mut self,
        _display: *mut xlib::Display,
        _window: xlib::Window,
        _event: &X11Event,
        _bounds: Rect,
    ) -> Command {
        Command::None
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

    pub fn render(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        context: &mut RenderContext,
    ) {
        self.widget.render(display, window, self.bounds, context);
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
    ) -> Command {
        self.widget.on_event(display, window, event, self.bounds)
    }
}

#[repr(usize)]
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Command {
    None,
    RequestRedraw,
    RequestLayout,
}

impl Command {
    pub fn compose(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, x) | (x, Self::None) => x,
            (Self::RequestRedraw, _) | (_, Self::RequestRedraw) => Self::RequestRedraw,
            (Self::RequestLayout, _) => Self::RequestLayout,
        }
    }
}

#[derive(Debug)]
pub struct RenderContext<'a> {
    pub text_renderer: &'a mut TextRenderer,
}
