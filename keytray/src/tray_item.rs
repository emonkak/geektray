use keytray_shell::event::MouseButton;
use keytray_shell::geometrics::{PhysicalPoint, Point, Rect, Size};
use keytray_shell::graphics::{
    FontDescription, HorizontalAlign, RenderContext, RenderOp, Text, VerticalAlign,
};
use keytray_shell::window::{Effect, Layout, Widget};
use std::rc::Rc;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::xproto;
use x11rb::protocol;

use crate::config::UiConfig;
use crate::tray_manager::TrayIcon;

#[derive(Debug)]
pub struct TrayItem {
    icon: TrayIcon,
    is_selected: bool,
    is_pressed: bool,
    font: FontDescription,
    config: Rc<UiConfig>,
}

impl TrayItem {
    pub fn new(icon: TrayIcon, font: FontDescription, config: Rc<UiConfig>) -> Self {
        Self {
            icon,
            is_selected: false,
            is_pressed: false,
            font,
            config,
        }
    }

    pub fn window(&self) -> xproto::Window {
        self.icon.window
    }

    pub fn update_icon(&mut self, icon: TrayIcon) -> Effect {
        self.icon = icon;
        Effect::RequestRedraw
    }

    pub fn click_item(&mut self, button: MouseButton) -> Effect {
        let center = (self.config.icon_size / 2.0) as i16;
        let container_window = self.icon.container_window;
        let icon_window = self.icon.window;
        let (button, button_mask) = match button {
            MouseButton::Left => (xproto::ButtonIndex::M1, xproto::ButtonMask::M1),
            MouseButton::Right => (xproto::ButtonIndex::M3, xproto::ButtonMask::M3),
            MouseButton::Middle => (xproto::ButtonIndex::M2, xproto::ButtonMask::M2),
            MouseButton::X1 => (xproto::ButtonIndex::M4, xproto::ButtonMask::M4),
            MouseButton::X2 => (xproto::ButtonIndex::M5, xproto::ButtonMask::M5),
        };
        return Effect::Action(Box::new(move |connection, screen_num, _| {
            emit_click_event(
                connection,
                screen_num,
                container_window,
                icon_window,
                button,
                button_mask,
                center,
                center,
            )?;
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
            context.push(RenderOp::RoundedRect(
                bg_color,
                Rect::new(position, layout.size),
                Size {
                    width: self.config.item_corner_radius,
                    height: self.config.item_corner_radius,
                },
            ));
        } else {
            context.push(RenderOp::Rect(bg_color, Rect::new(position, layout.size)));
        }

        let title = if self.config.show_index {
            format!("{}. {}", index + 1, self.icon.title)
        } else {
            self.icon.title.clone()
        };

        context.push(RenderOp::Text(
            fg_color,
            Rect {
                x: position.x + (self.config.icon_size + self.config.item_padding * 2.0),
                y: position.y,
                width: layout.size.width - (self.config.icon_size + self.config.item_padding * 3.0),
                height: layout.size.height,
            },
            Text {
                content: title.into(),
                font_description: self.font.clone(),
                font_size: self.config.font_size,
                horizontal_align: HorizontalAlign::Left,
                vertical_align: VerticalAlign::Middle,
            },
        ));

        context.push(RenderOp::CompositeWindow(
            self.icon.window,
            Rect {
                x: position.x + self.config.item_padding,
                y: position.y + self.config.item_padding,
                width: self.config.icon_size,
                height: self.config.icon_size,
            },
        ));
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
                        let container_window = self.icon.container_window;
                        let icon_window = self.icon.window;
                        let button = event.detail;
                        let button_mask = event.state.into();
                        return Effect::Action(Box::new(move |connection, screen_num, _| {
                            emit_click_event(
                                connection,
                                screen_num,
                                container_window,
                                icon_window,
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

#[inline]
pub fn emit_click_event<C: Connection>(
    connection: &C,
    screen_num: usize,
    container_window: xproto::Window,
    icon_window: xproto::Window,
    button: xproto::ButtonIndex,
    button_mask: xproto::ButtonMask,
    x: i16,
    y: i16,
) -> Result<(), ReplyError> {
    let screen = &connection.setup().roots[screen_num];
    let pointer = connection.query_pointer(screen.root)?.reply()?;

    let saved_position = connection
        .translate_coordinates(screen.root, container_window, 0, 0)?
        .reply()?;

    {
        let values = xproto::ConfigureWindowAux::new()
            .x(pointer.root_x as i32)
            .y(pointer.root_y as i32)
            .stack_mode(xproto::StackMode::ABOVE);
        connection.configure_window(container_window, &values)?.check()?;

        let values = xproto::ConfigureWindowAux::new()
            .stack_mode(xproto::StackMode::ABOVE);
        connection.configure_window(icon_window, &values)?.check()?;
    }

    send_button_event(
        connection,
        screen_num,
        icon_window,
        button,
        button_mask,
        true,
        x,
        y,
        pointer.root_x,
        pointer.root_y,
    )?;

    send_button_event(
        connection,
        screen_num,
        icon_window,
        button,
        button_mask,
        false,
        x,
        y,
        pointer.root_x,
        pointer.root_y,
    )?;

    {
        let values = xproto::ConfigureWindowAux::new()
            // .x(saved_position.dst_x as i32)
            // .y(saved_position.dst_y as i32)
            .stack_mode(xproto::StackMode::BELOW);
        connection.configure_window(container_window, &values)?.check()?;
    }

    connection.flush()?;

    Ok(())
}

#[inline]
fn send_button_event<Connection: self::Connection>(
    connection: &Connection,
    screen_num: usize,
    window: xproto::Window,
    button: xproto::ButtonIndex,
    button_mask: xproto::ButtonMask,
    is_pressed: bool,
    x: i16,
    y: i16,
    root_x: i16,
    root_y: i16,
) -> Result<(), ReplyError> {
    let screen = &connection.setup().roots[screen_num];

    let event = xproto::ButtonPressEvent {
        response_type: if is_pressed {
            xproto::BUTTON_PRESS_EVENT
        } else {
            xproto::BUTTON_RELEASE_EVENT
        },
        detail: button.into(),
        sequence: 0,
        time: x11rb::CURRENT_TIME,
        root: screen.root,
        event: window,
        child: x11rb::NONE,
        event_x: x,
        event_y: y,
        root_x,
        root_y,
        state: button_mask.into(),
        same_screen: true,
    };

    connection
        .send_event(true, window, xproto::EventMask::NO_EVENT, event)?
        .check()?;

    Ok(())
}
