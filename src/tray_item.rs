use std::mem;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use x11::xft;
use x11::xlib;

use app::{RenderContext, Styles};
use geometrics::{Point, Rectangle, Size};
use text_renderer::{HorizontalAlign, Text, VerticalAlign};
use utils;

pub struct TrayItem {
    display: *mut xlib::Display,
    embedder_window: xlib::Window,
    icon_window: xlib::Window,
    icon_title: String,
    styles: Rc<Styles>,
    position: Point,
    size: Size,
    is_embedded: bool,
    is_selected: bool,
}

impl TrayItem {
    pub fn new(
        display: *mut xlib::Display,
        tray_window: xlib::Window,
        icon_window: xlib::Window,
        icon_title: String,
        styles: Rc<Styles>,
    ) -> Self {
        unsafe {
            let mut attributes: xlib::XSetWindowAttributes =
                mem::MaybeUninit::uninit().assume_init();

            attributes.backing_store = xlib::WhenMapped;
            attributes.win_gravity = xlib::NorthWestGravity;

            let embedder_window = xlib::XCreateWindow(
                display,
                tray_window,
                0,
                0,
                1,
                1,
                0,
                xlib::CopyFromParent,
                xlib::InputOutput as u32,
                xlib::CopyFromParent as *mut xlib::Visual,
                xlib::CWBackingStore | xlib::CWWinGravity,
                &mut attributes,
            );

            xlib::XSelectInput(
                display,
                embedder_window,
                xlib::StructureNotifyMask | xlib::PropertyChangeMask | xlib::ExposureMask,
            );

            xlib::XSelectInput(display, icon_window, xlib::PropertyChangeMask);

            Self {
                display,
                embedder_window,
                icon_window,
                icon_title,
                styles,
                position: Point::default(),
                size: Size::default(),
                is_embedded: false,
                is_selected: false,
            }
        }
    }

    pub fn render(&self, context: &mut RenderContext) {
        println!(
            "TrayItem.render: {:?} (embedder_window: {}) (icon_window: {})",
            self.icon_title, self.embedder_window, self.icon_window
        );

        unsafe {
            let screen_number = xlib::XDefaultScreen(self.display);
            let visual = xlib::XDefaultVisual(self.display, screen_number);
            let colormap = xlib::XDefaultColormap(self.display, screen_number);
            let depth = xlib::XDefaultDepth(self.display, screen_number);
            let pixmap = xlib::XCreatePixmap(
                self.display,
                self.embedder_window,
                self.size.width as _,
                self.size.height as _,
                depth as u32,
            );
            let gc = xlib::XCreateGC(self.display, pixmap, 0, ptr::null_mut());
            let draw = xft::XftDrawCreate(self.display, pixmap, visual, colormap);

            let background_color;
            let foreground_color;

            if self.is_selected {
                background_color = &self.styles.selected_background;
                foreground_color = &self.styles.selected_foreground;
            } else {
                background_color = &self.styles.normal_background;
                foreground_color = &self.styles.normal_foreground;
            }

            xlib::XSetForeground(self.display, gc, background_color.pixel());
            xlib::XFillRectangle(
                self.display,
                pixmap,
                gc,
                0,
                0,
                self.size.width as _,
                self.size.height as _,
            );

            context.text_renderer.render_single_line(
                self.display,
                draw,
                Text {
                    content: &self.icon_title,
                    color: foreground_color,
                    font_size: self.styles.font_size,
                    font_set: &self.styles.font_set,
                    horizontal_align: HorizontalAlign::Left,
                    vertical_align: VerticalAlign::Middle,
                },
                Rectangle {
                    x: self.position.x + self.styles.icon_size + self.styles.padding * 2.0,
                    y: self.position.y,
                    width: self.size.width - (self.styles.icon_size + self.styles.padding * 2.0),
                    height: self.size.height,
                },
            );

            xlib::XSetWindowBackground(
                self.display,
                self.embedder_window,
                background_color.pixel(),
            );

            // xlib::XUnmapWindow(self.display, self.icon_window);
            xlib::XSetSubwindowMode(self.display, gc, xlib::IncludeInferiors);
            xlib::XCopyArea(
                self.display,
                pixmap,
                self.embedder_window,
                gc,
                0,
                0,
                self.size.width as _,
                self.size.height as _,
                0,
                0,
            );
            // xlib::XMapRaised(self.display, self.icon_window);
            xlib::XFlush(self.display);

            xlib::XFreeGC(self.display, gc);
            xlib::XFreePixmap(self.display, pixmap);

            xft::XftDrawDestroy(draw);
        }
    }

    pub fn layout(&mut self, position: Point) -> Size {
        let size = Size {
            width: self.styles.window_width,
            height: self.styles.icon_size + self.styles.padding * 2.0,
        };

        if self.position != position || self.size != size {
            unsafe {
                xlib::XMoveResizeWindow(
                    self.display,
                    self.embedder_window,
                    position.x as _,
                    position.y as _,
                    size.width as _,
                    size.height as _,
                );
            }

            self.position = position;
            self.size = size;
        }

        println!("TrayItem.layout(): {:?}", size);

        size
    }

    pub fn show_window(&mut self) {
        unsafe {
            xlib::XSelectInput(
                self.display,
                self.icon_window,
                xlib::StructureNotifyMask | xlib::PropertyChangeMask,
            );

            utils::move_resize_window(
                self.display,
                self.icon_window,
                self.styles.padding as _,
                self.styles.padding as _,
                self.styles.icon_size as _,
                self.styles.icon_size as _,
            );

            xlib::XReparentWindow(self.display, self.icon_window, self.embedder_window, 0, 0);
            xlib::XMapRaised(self.display, self.icon_window);
            xlib::XMapWindow(self.display, self.embedder_window);
        }

        self.is_embedded = true;
    }

    pub fn mark_as_destroyed(&mut self) {
        self.is_embedded = false;
    }

    pub fn emit_click(&self, button: c_uint, button_mask: c_uint, x: c_int, y: c_int) -> bool {
        unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(self.display);
            let root = xlib::XRootWindowOfScreen(screen);
            let (original_x, original_y) = utils::get_pointer_position(self.display, root);

            xlib::XWarpPointer(self.display, 0, self.icon_window, 0, 0, 0, 0, x, y);

            let result = utils::emit_button_event(
                self.display,
                self.icon_window,
                xlib::ButtonPress,
                button,
                button_mask,
                x,
                y,
            );
            if !result {
                return false;
            }

            let result = utils::emit_button_event(
                self.display,
                self.icon_window,
                xlib::ButtonRelease,
                button,
                button_mask,
                x,
                y,
            );
            if !result {
                return false;
            }

            xlib::XWarpPointer(self.display, 0, root, 0, 0, 0, 0, original_x, original_y);
            xlib::XFlush(self.display);
        }

        true
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.is_selected = selected;
    }

    pub fn embedder_window(&self) -> xlib::Window {
        self.embedder_window
    }

    pub fn icon_window(&self) -> xlib::Window {
        self.icon_window
    }
}

impl<'a> Drop for TrayItem {
    fn drop(&mut self) {
        unsafe {
            if self.is_embedded {
                let screen = xlib::XDefaultScreenOfDisplay(self.display);
                let root = xlib::XRootWindowOfScreen(screen);

                xlib::XSelectInput(self.display, self.icon_window, xlib::NoEventMask);
                xlib::XReparentWindow(self.display, self.icon_window, root, 0, 0);
                xlib::XUnmapWindow(self.display, self.icon_window);
                xlib::XMapRaised(self.display, self.icon_window);
            }

            xlib::XDestroyWindow(self.display, self.embedder_window);
        }
    }
}
