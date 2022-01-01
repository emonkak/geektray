use std::ptr;
use x11::xft;
use x11::xlib;

use crate::color::Color;
use crate::geometrics::{PhysicalSize, Rect};
use crate::text::{Text, TextRenderer};

#[derive(Debug)]
pub struct RenderContext<'a> {
    display: *mut xlib::Display,
    window: xlib::Window,
    viewport: PhysicalSize,
    pixmap: xlib::Pixmap,
    gc: xlib::GC,
    xft_draw: *mut xft::XftDraw,
    text_renderer: &'a mut TextRenderer,
}

impl<'a> RenderContext<'a> {
    pub fn new(
        display: *mut xlib::Display,
        window: xlib::Window,
        viewport: PhysicalSize,
        text_renderer: &'a mut TextRenderer,
    ) -> Self {
        unsafe {
            let screen_number = xlib::XDefaultScreen(display);
            let depth = xlib::XDefaultDepth(display, screen_number);
            let pixmap =
                xlib::XCreatePixmap(display, window, viewport.width, viewport.height, depth as _);

            let gc = xlib::XCreateGC(display, pixmap, 0, ptr::null_mut());
            xlib::XSetSubwindowMode(display, gc, xlib::IncludeInferiors);

            let visual = xlib::XDefaultVisual(display, screen_number);
            let colormap = xlib::XDefaultColormap(display, screen_number);
            let xft_draw = xft::XftDrawCreate(display, pixmap, visual, colormap);

            Self {
                display,
                window,
                viewport,
                pixmap,
                gc,
                xft_draw,
                text_renderer,
            }
        }
    }

    pub fn display(&self) -> *mut xlib::Display {
        self.display
    }

    pub fn commit(&mut self) {
        unsafe {
            xlib::XCopyArea(
                self.display,
                self.pixmap,
                self.window,
                self.gc,
                0,
                0,
                self.viewport.width,
                self.viewport.height,
                0,
                0,
            );
            xlib::XFlush(self.display);
        }
    }

    pub fn clear_viewport(&mut self, color: Color) {
        unsafe {
            xlib::XSetForeground(self.display, self.gc, color.pixel());
            xlib::XFillRectangle(
                self.display,
                self.pixmap,
                self.gc,
                0,
                0,
                self.viewport.width,
                self.viewport.height,
            );
        }
    }

    pub fn fill_rectange(&mut self, color: Color, bounds: Rect) {
        unsafe {
            xlib::XSetForeground(self.display, self.gc, color.pixel());
            xlib::XFillRectangle(
                self.display,
                self.pixmap,
                self.gc,
                bounds.x as _,
                bounds.y as _,
                bounds.width as _,
                bounds.height as _,
            );
        }
    }

    pub fn render_single_line_text(&mut self, color: Color, text: Text, bounds: Rect) {
        self.text_renderer
            .render_single_line(self.display, self.xft_draw, color, text, bounds);
    }
}

impl<'a> Drop for RenderContext<'a> {
    fn drop(&mut self) {
        unsafe {
            xlib::XFreeGC(self.display, self.gc);
            xlib::XFreePixmap(self.display, self.pixmap);
            xft::XftDrawDestroy(self.xft_draw);
        }
    }
}
