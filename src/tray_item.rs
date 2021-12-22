use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use x11::xft;
use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{PhysicalPoint, Rect, Size};
use crate::styles::Styles;
use crate::text_renderer::{HorizontalAlign, Text, VerticalAlign};
use crate::utils;
use crate::widget::{Command, RenderContext, Widget};

#[derive(Debug)]
pub struct TrayItem {
    icon_window: xlib::Window,
    icon_title: String,
    is_embedded: bool,
    is_selected: bool,
    is_pressed: bool,
    styles: Rc<Styles>,
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
            is_selected: false,
            is_pressed: false,
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
    ) {
        let center = (self.styles.icon_size / 2.0) as i32;

        unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(display);
            let root = xlib::XRootWindowOfScreen(screen);
            let (cursor_x, cursor_y) = utils::get_pointer_position(display, root);

            xlib::XWarpPointer(display, 0, self.icon_window, 0, 0, 0, 0, center, center);

            let mut x_root = 0;
            let mut y_root = 0;
            let mut _subwindow = 0;

            xlib::XTranslateCoordinates(
                display,
                self.icon_window,
                root,
                center,
                center,
                &mut x_root,
                &mut y_root,
                &mut _subwindow
            );

            let result = utils::send_button_event(
                display,
                self.icon_window,
                true,
                button,
                button_mask,
                center,
                center,
                x_root,
                y_root,
            );
            if !result {
                return;
            }

            let result = utils::send_button_event(
                display,
                self.icon_window,
                false,
                button,
                button_mask,
                center,
                center,
                x_root,
                y_root,
            );
            if !result {
                return;
            }

            xlib::XWarpPointer(display, 0, root, 0, 0, 0, 0, cursor_x, cursor_y);
            xlib::XFlush(display);
        }
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
            } else {
                background_color = &self.styles.normal_background;
                foreground_color = &self.styles.normal_foreground;
            }

            xlib::XSetSubwindowMode(display, gc, xlib::IncludeInferiors);
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
                    x: self.styles.icon_size + self.styles.padding * 2.0,
                    y: 0.0,
                    width: bounds.width - (self.styles.icon_size + self.styles.padding * 2.0),
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

                xlib::XClearArea(display, self.icon_window, 0, 0, 0, 0, xlib::True);
                xlib::XMapRaised(display, self.icon_window);
                xlib::XMoveResizeWindow(
                    display,
                    self.icon_window,
                    (bounds.x + self.styles.padding) as _,
                    (bounds.y + self.styles.padding) as _,
                    self.styles.icon_size as _,
                    self.styles.icon_size as _,
                );
            }
        }
    }

    fn layout(&mut self, container_size: Size) -> Size {
        Size {
            width: container_size.width as f32,
            height: self.styles.item_height(),
        }
    }

    fn on_event(
        &mut self,
        display: *mut xlib::Display,
        _window: xlib::Window,
        event: &X11Event,
        bounds: Rect,
    ) -> Command {
        match event {
            X11Event::ButtonPress(event) => {
                let pointer_position = PhysicalPoint {
                    x: event.x as _,
                    y: event.y as _,
                };
                if bounds.snap().contains(pointer_position) {
                    self.is_pressed = true;
                }
            }
            X11Event::ButtonRelease(event) => {
                let pointer_position = PhysicalPoint {
                    x: event.x as _,
                    y: event.y as _,
                };
                if self.is_pressed {
                    self.is_pressed = false;
                    if bounds.snap().contains(pointer_position) {
                        self.emit_click(
                            display,
                            event.button,
                            event.state,
                        );
                    }
                }
            }
            X11Event::LeaveNotify(_) => {
                self.is_pressed = false;
            }
            _ => {}
        }

        Command::None
    }
}
