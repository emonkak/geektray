use x11::xlib;

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
}

pub struct RenderContext<'a> {
    pub text_renderer: &'a mut TextRenderer,
}
