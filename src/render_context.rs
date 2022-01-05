use cairo_sys as cairo;
use std::ptr;
use x11::xft;
use x11::xlib;

use crate::color::Color;
use crate::geometrics::{PhysicalSize, Rect, Size};
use crate::text::{Text, TextRenderer};

#[derive(Debug)]
pub struct RenderContext<'a> {
    display: *mut xlib::Display,
    window: xlib::Window,
    viewport: PhysicalSize,
    pixmap: xlib::Pixmap,
    gc: xlib::GC,
    cairo: *mut cairo::cairo_t,
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
            let cairo_surface = cairo::cairo_xlib_surface_create(
                display,
                pixmap,
                visual,
                viewport.width as i32,
                viewport.height as i32,
            );
            let cairo = cairo::cairo_create(cairo_surface);

            Self {
                display,
                window,
                viewport,
                pixmap,
                gc,
                cairo,
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
        let [r, g, b] = color.into_f64_components();

        unsafe {
            cairo::cairo_save(self.cairo);

            cairo::cairo_rectangle(self.cairo, 0.0, 0.0, self.viewport.width as f64, self.viewport.height as f64);

            cairo::cairo_set_source_rgb(self.cairo, r, g, b);
            cairo::cairo_fill(self.cairo);

            cairo::cairo_restore(self.cairo);
        }
    }

    pub fn fill_rectange(&mut self, color: Color, bounds: Rect) {
        let [r, g, b] = color.into_f64_components();

        unsafe {
            cairo::cairo_save(self.cairo);

            cairo::cairo_rectangle(self.cairo, bounds.x, bounds.y, bounds.width, bounds.height);

            cairo::cairo_set_source_rgb(self.cairo, r, g, b);
            cairo::cairo_fill(self.cairo);

            cairo::cairo_restore(self.cairo);
        }
    }

    pub fn fill_rounded_rectange(&mut self, color: Color, bounds: Rect, mut radius: Size) {
        // Reference: https://www.cairographics.org/cookbook/roundedrectangles/ (Method B)
        const ARC_TO_BEZIER: f64 = 0.55228475;

        if radius.width > bounds.width - radius.width {
            radius.width = bounds.width / 2.0;
        }
        if radius.height > bounds.height - radius.height {
            radius.height = bounds.height / 2.0;
        }

        let curve_x = radius.width * ARC_TO_BEZIER;
        let curve_y = radius.height * ARC_TO_BEZIER;
        let [r, g, b] = color.into_f64_components();

        unsafe {
            cairo::cairo_save(self.cairo);

            cairo::cairo_new_path(self.cairo);
            cairo::cairo_move_to(
                self.cairo,
                bounds.x + radius.width,
                bounds.y,
            );
            cairo::cairo_rel_line_to(self.cairo, bounds.width - 2.0 * radius.width, 0.0);
            cairo::cairo_rel_curve_to(
                self.cairo,
                curve_x,
                0.0,
                radius.width,
                curve_y,
                radius.width,
                radius.height,
            );
            cairo::cairo_rel_line_to(self.cairo, 0.0, bounds.height - 2.0 * radius.height);
            cairo::cairo_rel_curve_to(
                self.cairo,
                0.0,
                curve_y,
                curve_x - radius.width,
                radius.height,
                -radius.width,
                radius.height,
            );
            cairo::cairo_rel_line_to(self.cairo, -bounds.width + 2.0 * radius.width, 0.0);
            cairo::cairo_rel_curve_to(
                self.cairo,
                -curve_x,
                0.0,
                -radius.width,
                -curve_y,
                -radius.width,
                -radius.height,
            );
            cairo::cairo_rel_line_to(self.cairo, 0.0, -bounds.height + 2.0 * radius.height);
            cairo::cairo_rel_curve_to(
                self.cairo,
                0.0,
                -curve_y,
                radius.width - curve_x,
                -radius.height,
                radius.width,
                -radius.height,
            );
            cairo::cairo_close_path(self.cairo);

            cairo::cairo_set_source_rgb(self.cairo, r, g, b);
            cairo::cairo_fill(self.cairo);

            cairo::cairo_restore(self.cairo);
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
