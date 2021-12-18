use x11::xlib;

use crate::app::RenderContext;
use crate::geometrics::{Point, Rect, Size};

pub trait Widget {
    fn mount(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        bounds: Rect,
    );

    fn unmount(&mut self, _display: *mut xlib::Display, _window: xlib::Window) {}

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
    bounds: Rect,
    mounted: bool,
    position_changed: bool,
    size_changed: bool,
}

impl<Widget: self::Widget> WidgetPod<Widget> {
    pub fn new(widget: Widget) -> Self {
        Self {
            widget,
            bounds: Rect::default(),
            mounted: false,
            position_changed: false,
            size_changed: false,
        }
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub fn mount(&mut self, display: *mut xlib::Display, window: xlib::Window) {
        self.widget.mount(display, window, self.bounds);
    }

    pub fn unmount(&mut self, display: *mut xlib::Display, window: xlib::Window) {
        self.widget.unmount(display, window);
    }

    pub fn render(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        context: &mut RenderContext,
    ) {
        if !self.mounted {
            self.widget.mount(display, window, self.bounds);
            self.mounted = true;
        }

        self.widget.render(display, window, self.bounds, context);
    }

    pub fn layout(&mut self, container_size: Size) -> Size {
        let size = self.widget.layout(container_size);

        if self.bounds.width != size.width || self.bounds.height != size.height {
            self.bounds.width = size.width;
            self.bounds.height = size.height;
            self.size_changed = true;
        }

        size
    }

    pub fn reposition(&mut self, position: Point) {
        if self.bounds.x != position.x || self.bounds.y != position.y {
            self.bounds.x = position.x;
            self.bounds.y = position.y;
            self.position_changed = true;
        }
    }
}
