use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use x11::xft;
use x11::xlib;

use crate::geometrics::{Rect, Size};
use crate::styles::Styles;
use crate::text_renderer::{HorizontalAlign, Text, VerticalAlign};
use crate::utils;
use crate::widget::{RenderContext, Widget};

#[derive(Debug)]
pub struct TrayItem {
    icon_window: xlib::Window,
    icon_title: String,
    styles: Rc<Styles>,
    is_embedded: bool,
    is_selected: bool,
    is_hovered: bool,
}

impl TrayItem {
    pub fn new(
        icon_window: xlib::Window,
        icon_title: String,
        is_embedded: bool,
        styles: Rc<Styles>,
    ) -> Self {
        Self {
            icon_window,
            icon_title,
            is_embedded,
            is_hovered: false,
            is_selected: false,
            styles,
        }
    }

    pub fn icon_window(&self) -> xlib::Window {
        self.icon_window
    }

    pub fn is_embedded(&self) -> bool {
        self.is_embedded
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

    pub fn set_embedded(&mut self, value: bool) {
        self.is_embedded = value;
    }

    pub fn set_selected(&mut self, value: bool) {
        self.is_selected = value;
    }
}

impl Widget for TrayItem {
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
                    x: self.styles.item_width(),
                    y: 0.0,
                    width: bounds.width - self.styles.item_height(),
                    height: bounds.height,
                },
            );

            xlib::XCopyArea(
                display,
                pixmap,
                window,
                gc,
                0,
                0,
                bounds.width as _,
                bounds.height as _,
                bounds.x as _,
                bounds.y as _,
            );

            xlib::XFreeGC(display, gc);
            xlib::XFreePixmap(display, pixmap);
            xft::XftDrawDestroy(draw);

            if self.is_embedded {
                xlib::XSetWindowBackground(display, self.icon_window, background_color.pixel());
                xlib::XMoveResizeWindow(
                    display,
                    self.icon_window,
                    (bounds.x + self.styles.padding) as _,
                    (bounds.y + self.styles.padding) as _,
                    self.styles.icon_size as _,
                    self.styles.icon_size as _,
                );
                xlib::XMapRaised(display, self.icon_window);
                xlib::XClearArea(display, self.icon_window, 0, 0, 0, 0, xlib::True);
            }
        }
    }

    fn layout(&mut self, container_size: Size) -> Size {
        Size {
            width: container_size.width as f32,
            height: self.styles.item_height(),
        }
    }
}
