use std::os::raw::*;
use std::rc::Rc;
use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{PhysicalPoint, Point, Rect, Size};
use crate::render_context::RenderContext;
use crate::styles::Styles;
use crate::text::{HorizontalAlign, Text, VerticalAlign};
use crate::utils;
use crate::widget::{Layout, Widget};
use crate::window::WindowEffcet;

#[derive(Debug)]
pub struct TrayItem {
    window: xlib::Window,
    title: String,
    is_selected: bool,
    is_pressed: bool,
    styles: Rc<Styles>,
}

impl TrayItem {
    pub fn new(window: xlib::Window, title: String, styles: Rc<Styles>) -> Self {
        Self {
            window,
            title,
            is_selected: false,
            is_pressed: false,
            styles,
        }
    }

    pub fn window(&self) -> xlib::Window {
        self.window
    }

    pub fn click_item(&mut self, button: c_uint, button_mask: c_uint) -> WindowEffcet {
        let center = (self.styles.icon_size / 2.0) as i32;
        let window = self.window;
        return WindowEffcet::Action(Box::new(move |display, _| unsafe {
            utils::emit_click_event(display, window, button, button_mask, center, center);
        }));
    }

    pub fn change_title(&mut self, title: String) -> WindowEffcet {
        self.title = title;
        WindowEffcet::RequestRedraw
    }

    pub fn select_item(&mut self) -> WindowEffcet {
        self.is_selected = true;
        WindowEffcet::RequestRedraw
    }

    pub fn deselect_item(&mut self) -> WindowEffcet {
        self.is_selected = false;
        WindowEffcet::RequestRedraw
    }
}

impl Widget for TrayItem {
    fn render(&self, position: Point, layout: &Layout, index: usize, context: &mut RenderContext) {
        let (bg_color, fg_color) = if self.is_selected {
            (
                self.styles.selected_item_background,
                self.styles.selected_item_foreground,
            )
        } else {
            (
                self.styles.normal_item_background,
                self.styles.normal_item_foreground,
            )
        };

        context.fill_rectange(bg_color, Rect::new(position, layout.size));

        let title = if self.styles.show_index {
            format!("{}. {}", index + 1, self.title)
        } else {
            self.title.clone()
        };

        context.render_single_line_text(
            fg_color,
            Text {
                content: &title,
                font_size: self.styles.font_size,
                font_set: &self.styles.font_set,
                horizontal_align: HorizontalAlign::Left,
                vertical_align: VerticalAlign::Middle,
            },
            Rect {
                x: position.x + (self.styles.icon_size + self.styles.item_padding * 2.0),
                y: position.y,
                width: layout.size.width - (self.styles.icon_size + self.styles.item_padding * 2.0),
                height: layout.size.height,
            },
        );

        unsafe {
            xlib::XClearArea(context.display(), self.window, 0, 0, 0, 0, xlib::True);
            xlib::XMapRaised(context.display(), self.window);
            xlib::XMoveResizeWindow(
                context.display(),
                self.window,
                (position.x + self.styles.item_padding) as _,
                (position.y + self.styles.item_padding) as _,
                self.styles.icon_size as _,
                self.styles.icon_size as _,
            );
        }
    }

    fn layout(&self, container_size: Size) -> Layout {
        Layout {
            size: Size {
                width: container_size.width as f32,
                height: self.styles.item_height(),
            },
            children: Vec::new(),
        }
    }

    fn on_event(&mut self, event: &X11Event, position: Point, layout: &Layout) -> WindowEffcet {
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
                        let center = (self.styles.icon_size / 2.0) as i32;
                        let window = self.window;
                        let button = event.button;
                        let button_mask = event.state;
                        return WindowEffcet::Action(Box::new(move |display, _| unsafe {
                            utils::emit_click_event(
                                display,
                                window,
                                button,
                                button_mask,
                                center,
                                center,
                            );
                        }));
                    }
                }
            }
            X11Event::LeaveNotify(_) => {
                self.is_pressed = false;
            }
            _ => {}
        }

        WindowEffcet::None
    }
}
