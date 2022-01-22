use keytray_shell::event::MouseButton;
use keytray_shell::geometrics::{PhysicalPoint, Point, Rect, Size};
use keytray_shell::graphics::{
    CacheKey, FontDescription, HorizontalAlign, RenderContext, RenderOp, Text, VerticalAlign,
};
use keytray_shell::window::{Effect, Layout, Widget};
use std::rc::Rc;
use x11rb::protocol;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;

use crate::config::UiConfig;
use crate::tray_manager::TrayIcon;

#[derive(Debug)]
pub struct TrayItem {
    icon: TrayIcon,
    is_selected: bool,
    is_pressed: bool,
    font: FontDescription,
    config: Rc<UiConfig>,
    image_cache_key: CacheKey,
}

impl TrayItem {
    pub fn new(icon: TrayIcon, font: FontDescription, config: Rc<UiConfig>) -> Self {
        Self {
            icon,
            is_selected: false,
            is_pressed: false,
            font,
            config,
            image_cache_key: CacheKey::next(),
        }
    }

    pub fn window(&self) -> xproto::Window {
        self.icon.window()
    }

    pub fn update_icon(&mut self, icon: TrayIcon) -> Effect {
        self.icon = icon;
        Effect::RequestRedraw
    }

    pub fn click_item(&mut self, button: MouseButton) -> Effect {
        let icon = self.icon.clone();
        let (button, button_mask) = match button {
            MouseButton::Left => (xproto::ButtonIndex::M1, xproto::ButtonMask::M1),
            MouseButton::Right => (xproto::ButtonIndex::M3, xproto::ButtonMask::M3),
            MouseButton::Middle => (xproto::ButtonIndex::M2, xproto::ButtonMask::M2),
            MouseButton::X1 => (xproto::ButtonIndex::M4, xproto::ButtonMask::M4),
            MouseButton::X2 => (xproto::ButtonIndex::M5, xproto::ButtonMask::M5),
        };
        return Effect::Action(Box::new(move |connection, screen_num, _| {
            icon.click(connection, screen_num, button, button_mask)?;
            Ok(Effect::Success)
        }));
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
    fn render(
        &self,
        position: Point,
        layout: &Layout,
        index: usize,
        _context: &mut RenderContext,
    ) -> RenderOp {
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

        let mut result = RenderOp::None;

        if self.config.item_corner_radius > 0.0 {
            result = result
                + RenderOp::RoundedRect(
                    bg_color,
                    Rect::new(position, layout.size),
                    Size {
                        width: self.config.item_corner_radius,
                        height: self.config.item_corner_radius,
                    },
                );
        } else {
            result = result + RenderOp::Rect(bg_color, Rect::new(position, layout.size));
        }

        let title = if self.config.show_index {
            format!("{}. {}", index + 1, self.icon.title())
        } else {
            self.icon.title().to_owned()
        };

        result = result
            + RenderOp::Text(
                fg_color,
                Rect {
                    x: position.x + (self.config.icon_size + self.config.item_padding * 2.0),
                    y: position.y,
                    width: layout.size.width
                        - (self.config.icon_size + self.config.item_padding * 3.0),
                    height: layout.size.height,
                },
                Text {
                    content: title.into(),
                    font_description: self.font.clone(),
                    font_size: self.config.font_size,
                    horizontal_align: HorizontalAlign::Left,
                    vertical_align: VerticalAlign::Middle,
                },
            );

        if self.icon.should_map() {
            let icon_window = self.icon.window();
            let bounds = Rect::new(
                Point {
                    x: position.x + self.config.item_padding,
                    y: position.y + self.config.item_padding,
                },
                Size {
                    width: self.config.icon_size,
                    height: self.config.icon_size,
                },
            );

            result = result
                + RenderOp::memoize(
                    self.image_cache_key,
                    (self.icon.version(), bounds),
                    move |connection, _, _| {
                        {
                            let values = xproto::ConfigureWindowAux::new()
                                .x(bounds.x as i32)
                                .y(bounds.y as i32)
                                .width(bounds.width as u32)
                                .height(bounds.height as u32);
                            connection.configure_window(icon_window, &values)?.check()?;
                        }

                        connection.map_window(icon_window)?.check()?;

                        let render_op = connection
                            .get_image(
                                xproto::ImageFormat::Z_PIXMAP,
                                icon_window,
                                0,
                                0,
                                bounds.width as u16,
                                bounds.height as u16,
                                0xffffffff,
                            )?
                            .reply()
                            .ok()
                            .map_or(RenderOp::None, |reply| {
                                RenderOp::Image(Rc::new(reply.data), bounds, reply.depth)
                            });

                        Ok(render_op)
                    },
                );
        }

        result
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
                        let icon = self.icon.clone();
                        let button = event.detail.into();
                        let button_mask = event.state.into();
                        return Effect::Action(Box::new(move |connection, screen_num, _| {
                            icon.click(connection, screen_num, button, button_mask)?;
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
