use anyhow::{self, Context as _};
use cairo_sys as cairo;
use gobject_sys as gobject;
use pango_cairo_sys as pango_cairo;
use pango_sys as pango;
use std::os::raw::*;
use std::rc::Rc;
use x11rb::connection::Connection;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::x11_utils::Serialize as _;
use x11rb::xcb_ffi::XCBConnection;

use crate::color::Color;
use crate::font::FontDescription;
use crate::geometrics::{PhysicalSize, Rect, Size};

#[derive(Debug)]
pub struct RenderContext {
    connection: Rc<XCBConnection>,
    window: xproto::Window,
    size: PhysicalSize,
    pixmap: xproto::Pixmap,
    gc: xproto::Gcontext,
    cairo: *mut cairo::cairo_t,
    cairo_surface: *mut cairo::cairo_surface_t,
    pango: *mut pango::PangoContext,
}

impl RenderContext {
    pub fn new(
        connection: Rc<XCBConnection>,
        screen_num: usize,
        window: xproto::Window,
        size: PhysicalSize,
    ) -> anyhow::Result<Self> {
        let screen = &connection.setup().roots[screen_num];
        let visual_id = connection
            .get_window_attributes(window)?
            .reply()
            .context("get window visual")?
            .visual;
        let (depth, visual) = screen
            .allowed_depths
            .iter()
            .find_map(|depth| {
                depth
                    .visuals
                    .iter()
                    .find(|visual| visual.visual_id == visual_id)
                    .map(|visual| (depth.depth, visual))
            })
            .ok_or(anyhow::anyhow!(
                "an optimal visual is not found in the screen"
            ))?;

        let pixmap = connection.generate_id().context("generate pixmap id")?;
        connection
            .create_pixmap(depth, pixmap, window, size.width as u16, size.height as u16)?
            .check()
            .context("create pixmap for render context")?;

        let gc = connection.generate_id().context("genrate gc id")?;
        {
            let values =
                xproto::CreateGCAux::new().subwindow_mode(xproto::SubwindowMode::INCLUDE_INFERIORS);
            connection
                .create_gc(gc, pixmap, &values)?
                .check()
                .context("create gc for render context")?;
        }

        let cairo_surface = unsafe {
            let visual = visual.serialize();
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
            pixmap,
            gc,
            cairo_surface,
            cairo,
            pango,
            size,
        })
    }

    pub fn size(&self) -> PhysicalSize {
        self.size
    }

    pub fn flush(&self) -> anyhow::Result<()> {
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
            .check()
            .context("copy rendered contents")?;

        Ok(())
    }

    pub fn draw_rect(&self, bounds: Rect, color: Color) {
        let [r, g, b, a] = color.to_f64_components();

        unsafe {
            cairo::cairo_save(self.cairo);
            cairo::cairo_rectangle(self.cairo, bounds.x, bounds.y, bounds.width, bounds.height);
            cairo::cairo_set_source_rgba(self.cairo, r, g, b, a);
            cairo::cairo_fill(self.cairo);
            cairo::cairo_restore(self.cairo);
        }
    }

    pub fn draw_rounded_rect(&self, bounds: Rect, color: Color, mut radius: Size) {
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
        let [r, g, b, a] = color.to_f64_components();

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

    pub fn draw_text(
        &self,
        content: &str,
        font: &FontDescription,
        font_size: f64,
        halign: HAlign,
        valign: VAlign,
        bounds: Rect,
        color: Color,
    ) {
        let mut font = font.clone();
        font.set_font_size(font_size * pango::PANGO_SCALE as f64);

        let layout = unsafe {
            let layout = pango::pango_layout_new(self.pango);

            pango::pango_layout_set_width(layout, bounds.width as i32 * pango::PANGO_SCALE);
            pango::pango_layout_set_height(layout, bounds.height as i32 * pango::PANGO_SCALE);
            pango::pango_layout_set_ellipsize(layout, pango::PANGO_ELLIPSIZE_END);
            pango::pango_layout_set_alignment(layout, halign.to_pango_align());
            pango::pango_layout_set_font_description(layout, font.as_mut_ptr());
            pango::pango_layout_set_text(
                layout,
                content.as_ptr() as *const c_char,
                content.len() as i32,
            );

            layout
        };

        let [r, g, b, a] = color.to_f64_components();
        let v_offset = unsafe {
            let mut layout_width = 0;
            let mut layout_height = 0;

            pango::pango_layout_get_pixel_size(layout, &mut layout_width, &mut layout_height);

            match valign {
                VAlign::Top => 0.0,
                VAlign::Middle => (bounds.height - layout_height as f64) / 2.0,
                VAlign::Bottom => bounds.height - layout_height as f64,
            }
        };

        unsafe {
            cairo::cairo_save(self.cairo);
            cairo::cairo_move_to(self.cairo, bounds.x, bounds.y + v_offset);
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
        }
        self.connection.free_gc(self.gc).ok();
        self.connection.free_pixmap(self.pixmap).ok();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(unused)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(unused)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

impl HAlign {
    fn to_pango_align(&self) -> pango::PangoAlignment {
        match self {
            Self::Left => pango::PANGO_ALIGN_LEFT,
            Self::Center => pango::PANGO_ALIGN_CENTER,
            Self::Right => pango::PANGO_ALIGN_RIGHT,
        }
    }
}
