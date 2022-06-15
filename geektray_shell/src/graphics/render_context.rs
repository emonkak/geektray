use cairo_sys as cairo;
use gobject_sys as gobject;
use pango_cairo_sys as pango_cairo;
use pango_sys as pango;
use std::any::Any;
use std::collections::hash_map;
use std::collections::HashMap;
use std::error;
use std::fmt;
use std::ops::Add;
use std::os::raw::*;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use x11rb::connection::Connection;
use x11rb::errors::{ConnectionError, ReplyError, ReplyOrIdError};
use x11rb::protocol::composite::ConnectionExt as _;
use x11rb::protocol::render;
use x11rb::protocol::render::ConnectionExt as _;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::x11_utils::Serialize as _;
use x11rb::xcb_ffi::XCBConnection;

use super::color::Color;
use super::text::{HorizontalAlign, Text, VerticalAlign};
use crate::geometrics::{PhysicalSize, Rect, Size};

pub struct RenderContext {
    connection: Rc<XCBConnection>,
    window: xproto::Window,
    screen_num: usize,
    size: PhysicalSize,
    pixmap: xproto::Pixmap,
    gc: xproto::Gcontext,
    pictformat: render::Pictformat,
    cairo: *mut cairo::cairo_t,
    cairo_surface: *mut cairo::cairo_surface_t,
    pango: *mut pango::PangoContext,
    cached_render_ops: HashMap<CacheKey, (Rc<dyn CacheDep>, RenderOp)>,
}

impl RenderContext {
    pub fn new(
        connection: Rc<XCBConnection>,
        screen_num: usize,
        window: xproto::Window,
        size: PhysicalSize,
    ) -> Result<Self, RenderError> {
        let screen = &connection.setup().roots[screen_num];
        let visual_id = connection.get_window_attributes(window)?.reply()?.visual;

        let (depth, visual) = screen
            .allowed_depths
            .iter()
            .filter_map(|depth| {
                depth
                    .visuals
                    .iter()
                    .find(|visual| visual.visual_id == visual_id)
                    .map(|visual| (depth.depth, visual))
            })
            .next()
            .ok_or(RenderError::VisualNotFound)?;

        let pixmap = connection.generate_id()?;
        connection
            .create_pixmap(depth, pixmap, window, size.width as u16, size.height as u16)?
            .check()?;

        let gc = connection.generate_id()?;
        {
            let values =
                xproto::CreateGCAux::new().subwindow_mode(xproto::SubwindowMode::INCLUDE_INFERIORS);
            connection.create_gc(gc, pixmap, &values)?.check()?;
        }

        let pictformat = get_pictformat_from_visual(connection.as_ref(), visual.visual_id)?
            .ok_or(RenderError::PictFormatNotFound)?;

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
            screen_num,
            size,
            pixmap,
            gc,
            pictformat,
            cairo_surface,
            cairo,
            pango,
            cached_render_ops: HashMap::new(),
        })
    }

    pub fn commit(&mut self, render_op: RenderOp) -> Result<(), RenderError> {
        self.eval(render_op)?;

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

        Ok(())
    }

    fn eval(&mut self, render_op: RenderOp) -> Result<(), RenderError> {
        let mut queue = Vec::new();
        let mut current = render_op;

        loop {
            match current {
                RenderOp::None => {}
                RenderOp::Batch(render_ops) => {
                    queue.extend(render_ops.into_iter().rev());
                }
                RenderOp::Rect(color, bounds) => {
                    self.rect(color, bounds);
                }
                RenderOp::RoundedRect(color, bounds, radius) => {
                    self.rounded_rect(color, bounds, radius);
                }
                RenderOp::Stroke(color, bounds, border_size) => {
                    self.stroke(color, bounds, border_size);
                }
                RenderOp::Text(color, bounds, text) => {
                    self.text(color, bounds, text);
                }
                RenderOp::Image(image, bounds, depth) => {
                    self.image(image.as_slice(), bounds, depth);
                }
                RenderOp::CompositeWindow(window, bounds) => {
                    self.composite_window(window, bounds)?;
                }
                RenderOp::Action(action) => {
                    current = action(self.connection.as_ref(), self.screen_num, self.window)?;
                    continue;
                }
                RenderOp::Memoize(key, dependency, action) => {
                    current = match self.cached_render_ops.entry(key) {
                        hash_map::Entry::Occupied(mut entry) => {
                            if entry.get().0.same(dependency.as_any()) {
                                entry.get().1.clone()
                            } else {
                                let render_op =
                                    action(self.connection.as_ref(), self.screen_num, self.window)?;
                                entry.insert((dependency, render_op.clone()));
                                render_op
                            }
                        }
                        hash_map::Entry::Vacant(entry) => {
                            let render_op =
                                action(self.connection.as_ref(), self.screen_num, self.window)?;
                            entry.insert((dependency, render_op.clone()));
                            render_op
                        }
                    };
                    continue;
                }
            }

            if let Some(render_op) = queue.pop() {
                current = render_op;
            } else {
                break;
            }
        }

        Ok(())
    }

    fn rect(&self, color: Color, bounds: Rect) {
        let [r, g, b, a] = color.to_f64_rgba();

        unsafe {
            cairo::cairo_save(self.cairo);
            cairo::cairo_rectangle(self.cairo, bounds.x, bounds.y, bounds.width, bounds.height);
            cairo::cairo_set_source_rgba(self.cairo, r, g, b, a);
            cairo::cairo_fill(self.cairo);
            cairo::cairo_restore(self.cairo);
        }
    }

    fn rounded_rect(&self, color: Color, bounds: Rect, mut radius: Size) {
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

    fn stroke(&self, color: Color, bounds: Rect, border_size: f64) {
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

    fn text(&self, color: Color, bounds: Rect, text: Text) {
        let mut font = text.font.clone();
        font.set_font_size(text.size * pango::PANGO_SCALE as f64);

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
            pango::pango_layout_set_font_description(layout, font.as_mut_ptr());
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

    fn image(&self, image: &[u8], bounds: Rect, depth: u8) {
        let format = if depth == 32 {
            cairo::FORMAT_A_RGB32
        } else {
            cairo::FORMAT_RGB24
        };
        unsafe {
            cairo::cairo_save(self.cairo);
            let stride = cairo::cairo_format_stride_for_width(format, bounds.width as i32);
            let image_surface = cairo::cairo_image_surface_create_for_data(
                image.as_ptr() as *mut _,
                format,
                bounds.width as i32,
                bounds.height as i32,
                stride,
            );
            cairo::cairo_set_source_surface(self.cairo, image_surface, bounds.x, bounds.y);
            cairo::cairo_paint(self.cairo);
            cairo::cairo_surface_destroy(image_surface);
            cairo::cairo_restore(self.cairo);
        }
    }

    fn composite_window(&self, window: xproto::Window, bounds: Rect) -> Result<(), RenderError> {
        let pixmap = self.connection.generate_id()?;

        if let Err(error) = self
            .connection
            .composite_name_window_pixmap(window, pixmap)?
            .check()
        {
            // Window is probably hidden.
            log::warn!("composite_window failure: {}", error);
            return Ok(());
        }

        let src_picture = self.connection.generate_id()?;
        let dest_picture = self.connection.generate_id()?;

        let visual = self
            .connection
            .get_window_attributes(window)?
            .reply()?
            .visual;
        let pictformat = get_pictformat_from_visual(self.connection.as_ref(), visual)?
            .ok_or(RenderError::PictFormatNotFound)?;

        {
            let values = render::CreatePictureAux::new();
            self.connection
                .render_create_picture(src_picture, pixmap, pictformat, &values)?
                .check()?;
            self.connection
                .render_create_picture(dest_picture, self.pixmap, self.pictformat, &values)?
                .check()?;
        }

        self.connection
            .render_composite(
                render::PictOp::OVER,
                src_picture,
                x11rb::NONE,
                dest_picture,
                0,
                0,
                0,
                0,
                bounds.x as i16,
                bounds.y as i16,
                bounds.width as u16,
                bounds.height as u16,
            )?
            .check()?;

        self.connection.render_free_picture(dest_picture)?.check()?;
        self.connection.render_free_picture(src_picture)?.check()?;

        Ok(())
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

#[derive(Clone)]
pub enum RenderOp {
    None,
    Batch(Vec<RenderOp>),
    Rect(Color, Rect),
    RoundedRect(Color, Rect, Size),
    Stroke(Color, Rect, f64),
    Text(Color, Rect, Text),
    Image(Rc<Vec<u8>>, Rect, u8),
    CompositeWindow(xproto::Window, Rect),
    Action(Rc<dyn Fn(&XCBConnection, usize, xproto::Window) -> Result<RenderOp, ReplyError>>),
    Memoize(
        CacheKey,
        Rc<dyn CacheDep>,
        Rc<dyn Fn(&XCBConnection, usize, xproto::Window) -> Result<RenderOp, ReplyError>>,
    ),
}

impl RenderOp {
    pub fn action<F>(action: F) -> Self
    where
        F: 'static + Fn(&XCBConnection, usize, xproto::Window) -> Result<RenderOp, ReplyError>,
    {
        Self::Action(Rc::new(action))
    }

    pub fn memoize<D, F>(key: CacheKey, dependency: D, action: F) -> Self
    where
        D: 'static + CacheDep,
        F: 'static + Fn(&XCBConnection, usize, xproto::Window) -> Result<RenderOp, ReplyError>,
    {
        Self::Memoize(key, Rc::new(dependency), Rc::new(action))
    }
}

impl Add for RenderOp {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::Batch(mut xs), Self::Batch(ys)) => {
                xs.extend(ys);
                Self::Batch(xs)
            }
            (Self::Batch(mut xs), y) => {
                xs.push(y);
                Self::Batch(xs)
            }
            (x, Self::Batch(ys)) => {
                let mut xs = Vec::with_capacity(ys.len() + 1);
                xs.push(x);
                xs.extend(ys);
                Self::Batch(xs)
            }
            (x, y) => Self::Batch(vec![x, y]),
        }
    }
}

#[derive(Debug)]
pub enum RenderError {
    VisualNotFound,
    PictFormatNotFound,
    X11Error(ReplyOrIdError),
}

impl From<ReplyOrIdError> for RenderError {
    fn from(value: ReplyOrIdError) -> Self {
        Self::X11Error(value)
    }
}

impl From<ReplyError> for RenderError {
    fn from(value: ReplyError) -> Self {
        Self::X11Error(value.into())
    }
}

impl From<ConnectionError> for RenderError {
    fn from(value: ConnectionError) -> Self {
        Self::X11Error(value.into())
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::VisualNotFound => f.write_str("Visual is not found"),
            Self::PictFormatNotFound => f.write_str("Pictformat is not found"),
            Self::X11Error(error) => error.fmt(f),
        }
    }
}

impl error::Error for RenderError {}

pub trait CacheDep {
    fn same(&self, other: &dyn Any) -> bool;

    fn as_any(&self) -> &dyn Any;
}

impl<T: 'static + PartialEq> CacheDep for T {
    fn same(&self, other: &dyn Any) -> bool {
        other.downcast_ref().map_or(false, |other| self == other)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn get_pictformat_from_visual<C: Connection>(
    connection: &C,
    visual_id: xproto::Visualid,
) -> Result<Option<render::Pictformat>, ReplyError> {
    let pictformat = connection
        .render_query_pict_formats()?
        .reply()?
        .screens
        .iter()
        .flat_map(|pictscreen| &pictscreen.depths)
        .flat_map(|pictdepth| &pictdepth.visuals)
        .find(|pictvisual| pictvisual.visual == visual_id)
        .map(|pictvisual| pictvisual.format);
    Ok(pictformat)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CacheKey(usize);

impl CacheKey {
    pub fn next() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(1);
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(counter)
    }
}
