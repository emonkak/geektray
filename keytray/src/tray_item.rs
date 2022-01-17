use keytray_shell::event::MouseButton;
use keytray_shell::graphics::{
    FontDescription, HorizontalAlign, PhysicalPoint, Point, Rect, RenderContext, Size, Text,
    VerticalAlign,
};
use keytray_shell::window::{Effect, Layout, Widget};
use std::rc::Rc;
use x11rb::protocol;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;

use crate::config::UiConfig;
use crate::utils;

#[derive(Debug)]
pub struct TrayItem {
    window: xproto::Window,
    title: String,
    is_selected: bool,
    is_pressed: bool,
    config: Rc<UiConfig>,
    font: Rc<FontDescription>,
}

impl TrayItem {
    pub fn new(
        window: xproto::Window,
        title: String,
        config: Rc<UiConfig>,
        font: Rc<FontDescription>,
    ) -> Self {
        Self {
            window,
            title,
            is_selected: false,
            is_pressed: false,
            config,
            font,
        }
    }

    pub fn window(&self) -> xproto::Window {
        self.window
    }

    pub fn click_item(&mut self, button: MouseButton) -> Effect {
        let center = (self.config.icon_size / 2.0) as i16;
        let window = self.window;
        let (button, button_mask) = match button {
            MouseButton::Left => (xproto::ButtonIndex::M1, xproto::ButtonMask::M1),
            MouseButton::Right => (xproto::ButtonIndex::M3, xproto::ButtonMask::M3),
            MouseButton::Middle => (xproto::ButtonIndex::M2, xproto::ButtonMask::M2),
            MouseButton::X1 => (xproto::ButtonIndex::M4, xproto::ButtonMask::M4),
            MouseButton::X2 => (xproto::ButtonIndex::M5, xproto::ButtonMask::M5),
        };
        return Effect::Action(Box::new(move |connection, screen_num, _| {
            utils::emit_click_event(
                connection,
                screen_num,
                window,
                button,
                button_mask,
                center,
                center,
            )?;
            Ok(Effect::Success)
        }));
    }

    pub fn change_title(&mut self, title: String) -> Effect {
        self.title = title;
        Effect::RequestRedraw
    }

    pub fn select_item(&mut self) -> Effect {
        self.is_selected = true;
        Effect::RequestRedraw
    }

    pub fn deselect_item(&mut self) -> Effect {
        self.is_selected = false;
        Effect::RequestRedraw
    }
}

impl Widget for TrayItem {
    fn render(&self, position: Point, layout: &Layout, index: usize, context: &mut RenderContext) {
        let (bg_color, fg_color) = if self.is_selected {
            (
                self.config.selected_item_background,
                self.config.selected_item_foreground,
            )
        } else {
            (
                self.config.normal_item_background,
                self.config.normal_item_foreground,
            )
        };

        if self.config.item_corner_radius > 0.0 {
            context.fill_rounded_rectange(
                bg_color,
                Rect::new(position, layout.size),
                Size {
                    width: self.config.item_corner_radius,
                    height: self.config.item_corner_radius,
                },
            );
        } else {
            context.fill_rectange(bg_color, Rect::new(position, layout.size));
        }

        let title = if self.config.show_index {
            format!("{}. {}", index + 1, self.title)
        } else {
            self.title.clone()
        };

        context.render_text(
            fg_color,
            Text {
                content: &title,
                font_description: &self.font,
                font_size: self.config.font_size,
                horizontal_align: HorizontalAlign::Left,
                vertical_align: VerticalAlign::Middle,
            },
            Rect {
                x: position.x + (self.config.icon_size + self.config.item_padding * 2.0),
                y: position.y,
                width: layout.size.width - (self.config.icon_size + self.config.item_padding * 2.0),
                height: layout.size.height,
            },
        );

        {
            let window = self.window;
            let values = xproto::ConfigureWindowAux::new()
                .x((position.x + self.config.item_padding) as i32)
                .y((position.y + self.config.item_padding) as i32)
                .width(self.config.icon_size as u32)
                .height(self.config.icon_size as u32)
                .stack_mode(xproto::StackMode::ABOVE);

            context.schedule_action(move |connection, _, _| {
                // without check() calling, because window maybe already destoryed.
                connection.configure_window(window, &values)?;
                connection.map_window(window)?;
                connection.clear_area(true, window, 0, 0, 0, 0)?;
                Ok(())
            });
        }
    }

    fn layout(&self, container_size: Size) -> Layout {
        Layout {
            size: Size {
                width: container_size.width as f64,
                height: self.config.item_height(),
            },
            children: Vec::new(),
        }
    }

    fn on_event(&mut self, event: &protocol::Event, position: Point, layout: &Layout) -> Effect {
        use protocol::Event::*;

        match event {
            ButtonPress(event) => {
                let bounds = Rect::new(position, layout.size);
                let pointer_position = PhysicalPoint {
                    x: event.event_x as _,
                    y: event.event_y as _,
                };
                if bounds.snap().contains(pointer_position) {
                    self.is_pressed = true;
                }
            }
            ButtonRelease(event) => {
                let bounds = Rect::new(position, layout.size);
                let pointer_position = PhysicalPoint {
                    x: event.event_x as _,
                    y: event.event_y as _,
                };
                if self.is_pressed {
                    self.is_pressed = false;
                    if bounds.snap().contains(pointer_position) {
                        let center = (self.config.icon_size / 2.0) as i16;
                        let window = self.window;
                        let button = event.detail;
                        let button_mask = event.state.into();
                        return Effect::Action(Box::new(move |connection, screen_num, _| {
                            utils::emit_click_event(
                                connection,
                                screen_num,
                                window,
                                button.into(),
                                button_mask,
                                center,
                                center,
                            )?;
                            Ok(Effect::Success)
                        }));
                    }
                }
            }
            LeaveNotify(_) => {
                self.is_pressed = false;
            }
            _ => {}
        }

        Effect::Success
    }
}
