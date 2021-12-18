use std::mem;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use x11::xft;
use x11::xlib;

use crate::app::RenderContext;
use crate::atoms::Atoms;
use crate::geometrics::{Rect, Size};
use crate::styles::Styles;
use crate::text_renderer::{HorizontalAlign, Text, VerticalAlign};
use crate::utils;
use crate::widget::Widget;
use crate::xembed::XEmbedMessage;

#[derive(Debug)]
pub struct TrayItem {
    icon_window: xlib::Window,
    icon_title: String,
    atoms: Rc<Atoms>,
    styles: Rc<Styles>,
    xembed_version: u64,
    will_embeding: bool,
    is_embedded: bool,
    is_hovered: bool,
    is_selected: bool,
}

impl TrayItem {
    pub fn new(
        icon_window: xlib::Window,
        icon_title: String,
        atoms: Rc<Atoms>,
        styles: Rc<Styles>,
        xembed_version: u64,
        will_embeding: bool,
    ) -> Self {
        Self {
            icon_window,
            icon_title,
            atoms,
            styles,
            xembed_version,
            will_embeding,
            is_embedded: false,
            is_hovered: false,
            is_selected: false,
        }
    }

    pub fn icon_window(&self) -> xlib::Window {
        self.icon_window
    }

    pub fn embed_icon(&mut self, display: *mut xlib::Display, window: xlib::Window) {
        unsafe {
            xlib::XMoveResizeWindow(
                display,
                self.icon_window,
                self.styles.padding as _,
                self.styles.padding as _,
                self.styles.icon_size as _,
                self.styles.icon_size as _,
            );
            xlib::XReparentWindow(display, self.icon_window, window, 0, 0);
            xlib::XMapRaised(display, self.icon_window);
            xlib::XMapWindow(display, window);
        }

        self.will_embeding = false;
        self.is_embedded = true;
    }

    pub fn request_embed_icon(&mut self) {
        self.will_embeding = true;
    }

    pub fn change_icon_title(&mut self, icon_title: String) {
        self.icon_title = icon_title;
    }

    pub fn emit_click(
        &self,
        display: *mut xlib::Display,
        button: c_uint,
        button_mask: c_uint,
        x: c_int,
        y: c_int,
    ) -> bool {
        unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(display);
            let root = xlib::XRootWindowOfScreen(screen);
            let (original_x, original_y) = utils::get_pointer_position(display, root);

            xlib::XWarpPointer(display, 0, self.icon_window, 0, 0, 0, 0, x, y);

            let result = utils::emit_button_event(
                display,
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
                display,
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

            xlib::XWarpPointer(display, 0, root, 0, 0, 0, 0, original_x, original_y);
            xlib::XFlush(display);
        }

        true
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.is_selected = selected;
    }

    pub fn set_embedded(&mut self, embedded: bool) {
        self.is_embedded = embedded;
    }

    fn unembed_icon(&mut self, display: *mut xlib::Display) {
        unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(display);
            let root = xlib::XRootWindowOfScreen(screen);

            xlib::XSelectInput(display, self.icon_window, xlib::NoEventMask);
            xlib::XReparentWindow(display, self.icon_window, root, 0, 0);
            xlib::XUnmapWindow(display, self.icon_window);
            xlib::XMapRaised(display, self.icon_window);
        }

        self.is_embedded = false;
    }
}

impl Widget for TrayItem {
    fn realize(
        &mut self,
        display: *mut xlib::Display,
        parent_window: xlib::Window,
        bounds: Rect,
    ) -> xlib::Window {
        unsafe {
            let mut attributes: xlib::XSetWindowAttributes =
                mem::MaybeUninit::uninit().assume_init();

            attributes.backing_store = xlib::WhenMapped;

            let window = xlib::XCreateWindow(
                display,
                parent_window,
                bounds.x as _,
                bounds.y as _,
                bounds.width as _,
                bounds.height as _,
                0,
                xlib::CopyFromParent,
                xlib::InputOutput as u32,
                xlib::CopyFromParent as *mut xlib::Visual,
                xlib::CWBackingStore,
                &mut attributes,
            );

            xlib::XSelectInput(
                display,
                window,
                xlib::StructureNotifyMask | xlib::PropertyChangeMask | xlib::ExposureMask,
            );

            send_embedded_notify(
                display,
                &self.atoms,
                self.icon_window,
                xlib::CurrentTime,
                window,
                self.xembed_version,
            );

            if self.will_embeding {
                self.embed_icon(display, window);
            } else {
                xlib::XSelectInput(display, self.icon_window, xlib::PropertyChangeMask);
            }

            window
        }
    }

    fn render(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        bounds: Rect,
        context: &mut RenderContext,
    ) {
        unsafe {
            let screen_number = xlib::XDefaultScreen(display);
            let visual = xlib::XDefaultVisual(display, screen_number);
            let colormap = xlib::XDefaultColormap(display, screen_number);
            let depth = xlib::XDefaultDepth(display, screen_number);
            let pixmap = xlib::XCreatePixmap(
                display,
                window,
                bounds.width as _,
                bounds.height as _,
                depth as _,
            );
            let gc = xlib::XCreateGC(display, pixmap, 0, ptr::null_mut());
            let draw = xft::XftDrawCreate(display, pixmap, visual, colormap);

            let background_color;
            let foreground_color;

            if self.is_selected {
                background_color = &self.styles.selected_background;
                foreground_color = &self.styles.selected_foreground;
            } else if self.is_hovered {
                background_color = &self.styles.hover_background;
                foreground_color = &self.styles.hover_foreground;
            } else {
                background_color = &self.styles.normal_background;
                foreground_color = &self.styles.normal_foreground;
            }

            xlib::XSetForeground(display, gc, background_color.pixel());
            xlib::XFillRectangle(
                display,
                pixmap,
                gc,
                0,
                0,
                bounds.width as _,
                bounds.height as _,
            );

            context.text_renderer.render_single_line(
                display,
                draw,
                &Text {
                    content: &self.icon_title,
                    color: foreground_color,
                    font_size: self.styles.font_size,
                    font_set: &self.styles.font_set,
                    horizontal_align: HorizontalAlign::Left,
                    vertical_align: VerticalAlign::Middle,
                },
                Rect {
                    x: bounds.x + self.styles.item_height(),
                    y: 0.0,
                    width: bounds.width - (self.styles.item_height()),
                    height: bounds.height,
                },
            );

            xlib::XSetWindowBackground(display, window, background_color.pixel());

            xlib::XClearWindow(display, window);

            // Request redraw icon window
            xlib::XClearArea(display, self.icon_window, 0, 0, 0, 0, xlib::True);

            xlib::XCopyArea(
                display,
                pixmap,
                window,
                gc,
                0,
                0,
                bounds.width as _,
                bounds.height as _,
                0,
                0,
            );

            xlib::XFreeGC(display, gc);
            xlib::XFreePixmap(display, pixmap);
            xft::XftDrawDestroy(draw);
        }
    }

    fn layout(&mut self, container_size: Size) -> Size {
        Size {
            width: container_size.width as f32,
            height: self.styles.item_height(),
        }
    }

    fn finalize(&mut self, display: *mut xlib::Display, _window: xlib::Window) {
        if self.is_embedded {
            self.unembed_icon(display);
        }
    }
}

unsafe fn send_embedded_notify(
    display: *mut xlib::Display,
    atoms: &Atoms,
    window: xlib::Window,
    timestamp: xlib::Time,
    embedder_window: xlib::Window,
    version: u64,
) {
    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, timestamp as c_long);
    data.set_long(1, XEmbedMessage::EmbeddedNotify as c_long);
    data.set_long(2, embedder_window as c_long);
    data.set_long(3, version as c_long);

    utils::send_client_message(display, window, window, atoms.XEMBED, data);
    xlib::XFlush(display);
}
