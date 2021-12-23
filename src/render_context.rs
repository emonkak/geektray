use std::ptr;
use x11::xft;
use x11::xlib;

use crate::geometrics::PhysicalSize;
use crate::text_renderer::TextRenderer;

#[derive(Debug)]
pub struct RenderContext<'a> {
    pub display: *mut xlib::Display,
    pub window: xlib::Window,
    pub viewport: PhysicalSize,
    pub pixmap: xlib::Pixmap,
    pub gc: xlib::GC,
    pub xft_draw: *mut xft::XftDraw,
    pub text_renderer: &'a mut TextRenderer,
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

    pub fn commit(&self) {
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
