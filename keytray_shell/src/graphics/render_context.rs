use cairo_sys as cairo;
use gobject_sys as gobject;
use pango_cairo_sys as pango_cairo;
use pango_sys as pango;
use std::os::raw::*;
use std::rc::Rc;
use x11rb::connection::Connection;
use x11rb::errors::{ReplyError, ReplyOrIdError};
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::xproto;
use x11rb::x11_utils::Serialize as _;
use x11rb::xcb_ffi::XCBConnection;

use crate::window::Effect;
use super::color::Color;
use super::geometrics::{PhysicalSize, Rect, Size, Point};
use super::text::{HorizontalAlign, Text, VerticalAlign};

pub struct RenderContext {
    connection: Rc<XCBConnection>,
    window: xproto::Window,
    size: PhysicalSize,
    pixmap: xproto::Pixmap,
    gc: xproto::Gcontext,
    cairo: *mut cairo::cairo_t,
    cairo_surface: *mut cairo::cairo_surface_t,
    pango: *mut pango::PangoContext,
    pending_effects: Vec<Effect>,
}

impl RenderContext {
    pub fn new(
        connection: Rc<XCBConnection>,
        screen_num: usize,
        window: xproto::Window,
        size: PhysicalSize,
    ) -> Result<Self, ReplyOrIdError> {
        let screen = &connection.setup().roots[screen_num];

        let pixmap = connection.generate_id()?;
        connection
            .create_pixmap(
                screen.root_depth,
                pixmap,
                window,
                size.width as u16,
                size.height as u16,
            )?
            .check()?;
        let gc = connection.generate_id()?;

        {
            let values =
                xproto::CreateGCAux::new().subwindow_mode(xproto::SubwindowMode::INCLUDE_INFERIORS);
            connection.create_gc(gc, pixmap, &values)?.check()?;
        }

        let visual = screen
            .allowed_depths
            .iter()
            .filter_map(|depth| {
                depth
                    .visuals
                    .iter()
                    .find(|depth| depth.visual_id == screen.root_visual)
            })
            .next()
            .expect("The root visual not available")
            .serialize();
        let cairo_surface = unsafe {
            cairo::cairo_xcb_surface_create(
                connection.get_raw_xcb_connection().cast(),
                pixmap,
                visual.as_ptr() as *mut cairo::xcb_visualtype_t,
                size.width as i32,
                size.height as i32,
            )
        };
        let cairo = unsafe { cairo::cairo_create(cairo_surface) };
        let pango = unsafe { pango_cairo::pango_cairo_create_context(cairo) };

        Ok(Self {
            connection,
            window,
            size,
            pixmap,
            gc,
            cairo_surface,
            cairo,
            pango,
            pending_effects: Vec::new(),
        })
    }

    pub fn commit(&mut self) -> Result<Effect, ReplyError> {
        unsafe {
            cairo::cairo_surface_flush(self.cairo_surface);
        }
        self.connection
            .copy_area(
                self.pixmap,
                self.window,
                self.gc,
                0,
                0,
                0,
                0,
                self.size.width as u16,
                self.size.height as u16,
            )?
            .check()?;
        Ok(self.pending_effects.drain(..).fold(Effect::Success, |acc, effect| acc + effect))
    }

    pub fn render_image(&self, pixmap: xproto::Pixmap, bounds: Rect) {
        self.connection
            .copy_area(
                pixmap,
                self.pixmap,
                self.gc,
                0,
                0,
                bounds.x as i16,
                bounds.y as i16,
                bounds.width as u16,
                bounds.height as u16,
            )
            .ok();
    }

    pub fn push_effect(&mut self, effect: Effect) {
        self.pending_effects.push(effect)
    }

    pub fn clear(&mut self, color: Color) {
        self.rectangle(color, Rect::new(Point::ZERO, self.size.unsnap()));
    }

    pub fn rectangle(&self, color: Color, bounds: Rect) {
        let [r, g, b, a] = color.to_f64_rgba();

        unsafe {
            cairo::cairo_save(self.cairo);
            cairo::cairo_rectangle(self.cairo, bounds.x, bounds.y, bounds.width, bounds.height);
            cairo::cairo_set_source_rgba(self.cairo, r, g, b, a);
            cairo::cairo_fill(self.cairo);
            cairo::cairo_restore(self.cairo);
        }
    }

    pub fn rounded_rectangle(&self, color: Color, bounds: Rect, mut radius: Size) {
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
        let [r, g, b, a] = color.to_f64_rgba();

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

    pub fn stroke(&self, color: Color, bounds: Rect, border_size: f64) {
        let [r, g, b, a] = color.to_f64_rgba();

        unsafe {
            cairo::cairo_save(self.cairo);
            cairo::cairo_rectangle(
                self.cairo,
                bounds.x + (border_size / 2.0),
                bounds.y + (border_size / 2.0),
                bounds.width - border_size,
                bounds.height - border_size,
            );
            cairo::cairo_set_source_rgba(self.cairo, r, g, b, a);
            cairo::cairo_set_line_width(self.cairo, border_size);
            cairo::cairo_stroke(self.cairo);
            cairo::cairo_restore(self.cairo);
        }
    }

    pub fn text(&self, color: Color, text: Text, bounds: Rect) {
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
            pango::pango_layout_set_font_description(layout, font_description.as_mut_ptr());
            pango::pango_layout_set_text(
                layout,
                text.content.as_ptr() as *const c_char,
                text.content.len() as i32,
            );

            layout
        };

        let [r, g, b, a] = color.to_f64_rgba();
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
            gobject::g_object_unref(self.pango.cast());
            cairo::cairo_destroy(self.cairo);
            cairo::cairo_surface_destroy(self.cairo_surface);
            self.connection.free_gc(self.gc).ok();
            self.connection.free_pixmap(self.pixmap).ok();
        }
    }
}
