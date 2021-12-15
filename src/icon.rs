use std::mem;
use std::os::raw::*;
use std::ptr;
use x11::xft;
use x11::xlib;

use context::Context;
use font::FontRenderer;
use layout::Layoutable;
use utils;

pub struct TrayIcon<'a> {
    context: &'a Context,
    embedder_window: xlib::Window,
    icon_window: xlib::Window,
    status: Status,
    is_selected: bool,
    title: String,
    width: u32,
    height: u32,
}

#[derive(Debug, PartialEq)]
enum Status {
    Initialized,
    Embedded,
    Invalidated,
}

impl<'a> TrayIcon<'a> {
    pub fn new(context: &'a Context, tray_window: xlib::Window, icon_window: xlib::Window, x: i32, y: i32, width: u32, height: u32) -> Self {
        unsafe {
            let mut attributes: xlib::XSetWindowAttributes = mem::MaybeUninit::uninit().assume_init();
            attributes.background_pixel = context.normal_background.pixel();
            attributes.backing_store = xlib::WhenMapped;
            attributes.win_gravity = xlib::NorthWestGravity;

            let embedder_window = xlib::XCreateWindow(
                context.display,
                tray_window,
                x,
                y,
                width,
                height,
                0,
                xlib::CopyFromParent,
                xlib::InputOutput as u32,
                xlib::CopyFromParent as *mut xlib::Visual,
                xlib::CWBackPixel | xlib::CWBackingStore | xlib::CWWinGravity,
                &mut attributes
            );

            xlib::XSelectInput(
                context.display,
                embedder_window,
                xlib::StructureNotifyMask | xlib::PropertyChangeMask | xlib::ExposureMask
            );

            let title = context.get_window_title(icon_window).unwrap_or_default();

            TrayIcon {
                context,
                embedder_window,
                icon_window,
                status: Status::Initialized,
                is_selected: false,
                title,
                width,
                height,
            }
        }
    }

    pub fn render(&self, font_renderer: &mut FontRenderer) {
        println!("TrayIcon.render: {:?} (embedder_window: {}) (icon_window: {})", self.title, self.embedder_window, self.icon_window);

        unsafe {
            let screen_number = xlib::XDefaultScreen(self.context.display);
            let visual = xlib::XDefaultVisual(self.context.display, screen_number);
            let colormap = xlib::XDefaultColormap(self.context.display, screen_number);
            let depth = xlib::XDefaultDepth(self.context.display, screen_number);
            let pixmap = xlib::XCreatePixmap(self.context.display, self.embedder_window, self.width, self.height, depth as u32);
            let gc = xlib::XCreateGC(self.context.display, pixmap, 0, ptr::null_mut());
            let draw = xft::XftDrawCreate(self.context.display, pixmap, visual, colormap);

            let background_color;
            let foreground_color;

            if self.is_selected {
                background_color = &self.context.selected_background;
                foreground_color = &self.context.selected_foreground;
            } else {
                background_color = &self.context.normal_background;
                foreground_color = &self.context.normal_foreground;
            }

            xlib::XSetForeground(self.context.display, gc, background_color.pixel());
            xlib::XFillRectangle(self.context.display, pixmap, gc, 0, 0, self.width, self.height);

            font_renderer.render_line_text(
                self.context.display,
                draw,
                &mut foreground_color.xft_color(),
                &self.context.font_set,
                self.context.icon_size as i32 + self.context.padding as i32 * 2,
                (self.height / 2) as i32 - (self.context.font_set.font_descriptor().pixel_size / 2) as i32,
                &self.title
            );

            xlib::XSetWindowBackground(
                self.context.display,
                self.embedder_window,
                background_color.pixel()
            );

            xlib::XUnmapWindow(self.context.display, self.icon_window);
            xlib::XSetSubwindowMode(self.context.display,gc, xlib::IncludeInferiors);
            xlib::XCopyArea(self.context.display, pixmap, self.embedder_window, gc, 0, 0, self.width, self.height, 0, 0);
            xlib::XMapRaised(self.context.display, self.icon_window);
            xlib::XFlush(self.context.display);

            xlib::XFreeGC(self.context.display, gc);
            xlib::XFreePixmap(self.context.display, pixmap);
            xft::XftDrawDestroy(draw);
        }
    }

    pub fn show(&mut self) {
        if self.status == Status::Embedded {
            return;
        }

        self.status = Status::Embedded;
        utils::move_resize_window(
            self.context.display,
            self.icon_window,
            self.context.padding as i32,
            self.context.padding as i32,
            self.context.icon_size,
            self.context.icon_size
        );

        unsafe {
            xlib::XSelectInput(self.context.display, self.icon_window, xlib::StructureNotifyMask | xlib::PropertyChangeMask);
            xlib::XReparentWindow(self.context.display, self.icon_window, self.embedder_window, 0, 0);
            xlib::XMapRaised(self.context.display, self.icon_window);
            xlib::XMapWindow(self.context.display, self.embedder_window);
            xlib::XFlush(self.context.display);
        }
    }

    pub fn wait_for_embedding(&mut self) {
        unsafe {
            xlib::XSelectInput(self.context.display, self.icon_window, xlib::PropertyChangeMask);
            xlib::XFlush(self.context.display);
        }
    }

    pub fn invalidate(self) {
        if self.status == Status::Invalidated {
            return;
        }

        let mut self_mut = self;
        self_mut.status = Status::Invalidated;
    }

    pub fn emit_icon_click(&self, button: c_uint, button_mask: c_uint, x: c_int, y: c_int) -> bool {
        unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(self.context.display);
            let root = xlib::XRootWindowOfScreen(screen);
            let (original_x, original_y) = utils::get_pointer_position(self.context.display, root);

            xlib::XWarpPointer(self.context.display, 0, self.icon_window, 0, 0, 0, 0, x, y);

            let result = utils::emit_button_event(
                self.context.display,
                self.icon_window,
                xlib::ButtonPress,
                button,
                button_mask,
                x,
                y
            );
            if !result {
                return false;
            }

            let result = utils::emit_button_event(
                self.context.display,
                self.icon_window,
                xlib::ButtonRelease,
                button,
                button_mask,
                x,
                y
            );
            if !result {
                return false;
            }

            xlib::XWarpPointer(self.context.display, 0, root, 0, 0, 0, 0, original_x, original_y);
            xlib::XFlush(self.context.display);
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

impl<'a> Layoutable for TrayIcon<'a> {
    fn update_layout(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.width = width;
        self.height = height;

        unsafe {
            xlib::XMoveResizeWindow(
                self.context.display,
                self.embedder_window,
                x,
                y,
                width,
                height
            );
        }
    }
}

impl<'a> Drop for TrayIcon<'a> {
    fn drop(&mut self) {
        unsafe {
            if self.status == Status::Embedded {
                let screen = xlib::XDefaultScreenOfDisplay(self.context.display);
                let root = xlib::XRootWindowOfScreen(screen);

                xlib::XSelectInput(self.context.display, self.icon_window, xlib::NoEventMask);
                xlib::XUnmapWindow(self.context.display, self.icon_window);
                xlib::XReparentWindow(self.context.display, self.icon_window, root, 0, 0);
                xlib::XMapRaised(self.context.display, self.icon_window);
            }

            xlib::XDestroyWindow(self.context.display, self.embedder_window);
        }
    }
}
