use std::os::raw::*;
use std::rc::Rc;
use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{PhysicalPoint, Point, Rect, Size};
use crate::paint_context::PaintContext;
use crate::styles::Styles;
use crate::text::{HorizontalAlign, Text, VerticalAlign};
use crate::utils;
use crate::widget::{LayoutResult, SideEffect, Widget};

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

    pub fn emit_click(&self, display: *mut xlib::Display, button: c_uint, button_mask: c_uint) {
        let center = (self.styles.icon_size / 2.0) as i32;

        unsafe {
            utils::emit_click_event(
                display,
                self.icon_window,
                button,
                button_mask,
                center,
                center,
            );
        }
    }

    pub fn set_icon_title(&mut self, icon_title: String) {
        self.icon_title = icon_title;
    }

    pub fn set_embedded(&mut self, value: bool) {
        self.is_embedded = value;
    }

    pub fn set_selected(&mut self, value: bool) {
        self.is_selected = value;
    }
}

impl Widget for TrayItem {
    fn render(&mut self, position: Point, layout: &LayoutResult, context: &mut PaintContext) {
        let (bg_color, fg_color) = if self.is_selected {
            (
                self.styles.selected_background,
                self.styles.selected_foreground,
            )
        } else {
            (self.styles.normal_background, self.styles.normal_foreground)
        };

        context.fill_rectange(bg_color, Rect::new(position, layout.size));

        context.render_single_line_text(
            fg_color,
            Text {
                content: &self.icon_title,
                font_size: self.styles.font_size,
                font_set: &self.styles.font_set,
                horizontal_align: HorizontalAlign::Left,
                vertical_align: VerticalAlign::Middle,
            },
            Rect {
                x: position.x + (self.styles.icon_size + self.styles.padding * 2.0),
                y: position.y,
                width: layout.size.width - (self.styles.icon_size + self.styles.padding * 2.0),
                height: layout.size.height,
            },
        );

        if self.is_embedded {
            unsafe {
                xlib::XClearArea(context.display(), self.icon_window, 0, 0, 0, 0, xlib::True);
                xlib::XMapRaised(context.display(), self.icon_window);
                xlib::XMoveResizeWindow(
                    context.display(),
                    self.icon_window,
                    (position.x + self.styles.padding) as _,
                    (position.y + self.styles.padding) as _,
                    self.styles.icon_size as _,
                    self.styles.icon_size as _,
                );
            }
        }
    }

    fn layout(&mut self, container_size: Size) -> LayoutResult {
        LayoutResult {
            size: Size {
                width: container_size.width as f32,
                height: self.styles.item_height(),
            },
            children: Vec::new(),
        }
    }

    fn on_event(
        &mut self,
        display: *mut xlib::Display,
        _window: xlib::Window,
        event: &X11Event,
        position: Point,
        layout: &LayoutResult,
    ) -> SideEffect {
        match event {
            X11Event::ButtonPress(event) => {
                let bounds = Rect::new(position, layout.size);
                let pointer_position = PhysicalPoint {
                    x: event.x as _,
                    y: event.y as _,
                };
                if bounds.snap().contains(pointer_position) {
                    self.is_pressed = true;
                }
            }
            X11Event::ButtonRelease(event) => {
                let bounds = Rect::new(position, layout.size);
                let pointer_position = PhysicalPoint {
                    x: event.x as _,
                    y: event.y as _,
                };
                if self.is_pressed {
                    self.is_pressed = false;
                    if bounds.snap().contains(pointer_position) {
                        self.emit_click(display, event.button, event.state);
                    }
                }
            }
            X11Event::LeaveNotify(_) => {
                self.is_pressed = false;
            }
            _ => {}
        }

        SideEffect::None
    }
}
