use std::mem;
use x11::xlib;

use crate::app::RenderContext;
use crate::geometrics::{Point, Rect, Size};

pub trait Widget {
    fn realize(
        &mut self,
        display: *mut xlib::Display,
        parent_window: xlib::Window,
        bounds: Rect,
    ) -> xlib::Window;

    fn render(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        bounds: Rect,
        context: &mut RenderContext,
    );

    fn layout(&mut self, container_size: Size) -> Size;

    fn finalize(&mut self, _display: *mut xlib::Display, _window: xlib::Window) {}
}

#[derive(Debug)]
pub struct WidgetPod<Widget> {
    pub widget: Widget,
    window: Option<xlib::Window>,
    bounds: Rect,
    position_changed: bool,
    size_changed: bool,
}

impl<Widget: self::Widget> WidgetPod<Widget> {
    pub fn new(widget: Widget) -> Self {
        Self {
            widget,
            window: None,
            bounds: Rect::default(),
            position_changed: false,
            size_changed: false,
        }
    }

    pub fn window(&self) -> Option<xlib::Window> {
        self.window
    }

    pub fn render(
        &mut self,
        display: *mut xlib::Display,
        parent_window: xlib::Window,
        context: &mut RenderContext,
    ) {
        let window = match self.window {
            Some(window) => window,
            None => {
                let window = self.widget.realize(display, parent_window, self.bounds);
                self.window = Some(window);
                window
            }
        };

        if self.position_changed || self.size_changed {
            let mut window_changes = unsafe { mem::MaybeUninit::<xlib::XWindowChanges>::uninit().assume_init() };
            let mut value_mask = 0;

            if self.position_changed {
                window_changes.x = self.bounds.x as _;
                window_changes.y = self.bounds.y as _;
                value_mask |= xlib::CWX | xlib::CWY;
                self.position_changed = false;
            }

            if self.size_changed {
                window_changes.width = self.bounds.width as _;
                window_changes.height = self.bounds.height as _;
                value_mask |= xlib::CWWidth | xlib::CWWidth;
                self.size_changed = false;
            }

            unsafe {
                xlib::XConfigureWindow(
                    display,
                    window,
                    value_mask as _,
                    &mut window_changes
                );
            }
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

    pub fn finalize(&mut self, display: *mut xlib::Display) {
        if let Some(window) = self.window.take() {
            self.widget.finalize(display, window);

            unsafe {
                xlib::XDestroyWindow(display, window);
            }
        }
    }
}
