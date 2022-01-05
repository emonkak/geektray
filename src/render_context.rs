use cairo_sys as cairo;
use gobject_sys as gobject;
use pango_cairo_sys as pango_cairo;
use pango_sys as pango;
use std::os::raw::*;
use std::ptr;
use x11::xlib;

use crate::color::Color;
use crate::geometrics::{PhysicalSize, Rect, Size};
use crate::text::{HorizontalAlign, Text, VerticalAlign};

#[derive(Debug)]
pub struct RenderContext {
    display: *mut xlib::Display,
    window: xlib::Window,
    viewport: PhysicalSize,
    pixmap: xlib::Pixmap,
    gc: xlib::GC,
    cairo: *mut cairo::cairo_t,
    pango: *mut pango::PangoContext,
}

impl RenderContext {
    pub fn new(display: *mut xlib::Display, window: xlib::Window, viewport: PhysicalSize) -> Self {
        unsafe {
            let screen_number = xlib::XDefaultScreen(display);
            let depth = xlib::XDefaultDepth(display, screen_number);

            let pixmap =
                xlib::XCreatePixmap(display, window, viewport.width, viewport.height, depth as _);
            let gc = xlib::XCreateGC(display, pixmap, 0, ptr::null_mut());

            xlib::XSetSubwindowMode(display, gc, xlib::IncludeInferiors);

            let visual = xlib::XDefaultVisual(display, screen_number);
            let cairo_surface = cairo::cairo_xlib_surface_create(
                display,
                pixmap,
                visual,
                viewport.width as i32,
                viewport.height as i32,
            );
            let cairo = cairo::cairo_create(cairo_surface);
            let pango = pango_cairo::pango_cairo_create_context(cairo);

            Self {
                display,
                window,
                viewport,
                pixmap,
                gc,
                cairo,
                pango,
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
        let [r, g, b, a] = color.into_f64_components();

        unsafe {
            cairo::cairo_save(self.cairo);

            cairo::cairo_rectangle(
                self.cairo,
                0.0,
                0.0,
                self.viewport.width as f64,
                self.viewport.height as f64,
            );

            cairo::cairo_set_source_rgba(self.cairo, r, g, b, a);
            cairo::cairo_fill(self.cairo);

            cairo::cairo_restore(self.cairo);
        }
    }

    pub fn fill_rectange(&mut self, color: Color, bounds: Rect) {
        let [r, g, b, a] = color.into_f64_components();

        unsafe {
            cairo::cairo_save(self.cairo);
            cairo::cairo_rectangle(self.cairo, bounds.x, bounds.y, bounds.width, bounds.height);
            cairo::cairo_set_source_rgba(self.cairo, r, g, b, a);
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
        let [r, g, b, a] = color.into_f64_components();

        unsafe {
            cairo::cairo_save(self.cairo);
            cairo::cairo_new_path(self.cairo);
            cairo::cairo_move_to(self.cairo, bounds.x + radius.width, bounds.y);
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
            cairo::cairo_set_source_rgba(self.cairo, r, g, b, a);
            cairo::cairo_fill(self.cairo);
            cairo::cairo_restore(self.cairo);
        }
    }

    pub fn render_single_line_text(&mut self, color: Color, text: Text, bounds: Rect) {
        let mut font_description = text.font_description.clone();
        font_description.set_font_size(text.font_size * pango::PANGO_SCALE as f64);

        let layout = unsafe {
            let layout = pango::pango_layout_new(self.pango);

            pango::pango_layout_set_width(layout, bounds.width as i32 * pango::PANGO_SCALE);
            pango::pango_layout_set_height(layout, bounds.height as i32 * pango::PANGO_SCALE);
            pango::pango_layout_set_ellipsize(layout, pango::PANGO_ELLIPSIZE_END);
            pango::pango_layout_set_alignment(
                layout,
                match text.horizontal_align {
                    HorizontalAlign::Left => pango::PANGO_ALIGN_LEFT,
                    HorizontalAlign::Center => pango::PANGO_ALIGN_CENTER,
                    HorizontalAlign::Right => pango::PANGO_ALIGN_RIGHT,
                },
            );
            pango::pango_layout_set_font_description(layout, font_description.as_ptr());
            pango::pango_layout_set_text(
                layout,
                text.content.as_ptr() as *const c_char,
                text.content.len() as i32,
            );

            layout
        };

        let [r, g, b, a] = color.into_f64_components();
        let y_offset = unsafe {
            let mut width = 0;
            let mut height = 0;

            pango::pango_layout_get_pixel_size(layout, &mut width, &mut height);

            match text.vertical_align {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Middle => (bounds.height - height as f64) / 2.0,
                VerticalAlign::Bottom => bounds.height - height as f64,
            }
        };

        unsafe {
            cairo::cairo_save(self.cairo);
            cairo::cairo_move_to(self.cairo, bounds.x, bounds.y + y_offset);
            cairo::cairo_set_source_rgba(self.cairo, r, g, b, a);
            pango_cairo::pango_cairo_show_layout(self.cairo, layout);
            cairo::cairo_restore(self.cairo);

            gobject::g_object_unref(layout.cast());
        }
    }
}

impl Drop for RenderContext {
    fn drop(&mut self) {
        unsafe {
            cairo::cairo_destroy(self.cairo);
            gobject::g_object_unref(self.pango.cast());
            xlib::XFreeGC(self.display, self.gc);
            xlib::XFreePixmap(self.display, self.pixmap);
        }
    }
}
