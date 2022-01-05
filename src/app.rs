use libdbus_sys as dbus;
use std::ffi::CString;
use std::rc::Rc;
use std::str::FromStr;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt;
use x11rb::xcb_ffi::XCBConnection;

use crate::atoms::Atoms;
use crate::command::Command;
use crate::config::{Config, UiConfig};
use crate::event_loop::{ControlFlow, Event, EventLoop};
use crate::font::FontDescription;
use crate::hotkey::HotkeyInterpreter;
use crate::keyboard::{KeyboardMapping, Modifiers};
use crate::tray_container::TrayContainer;
use crate::tray_manager::{TrayEvent, TrayManager};
use crate::utils;
use crate::window::Window;

#[derive(Debug)]
pub struct App {
    connection: Rc<XCBConnection>,
    screen_num: usize,
    config: Rc<UiConfig>,
    atoms: Rc<Atoms>,
    keyboard_mapping: KeyboardMapping,
    hotkey_interpreter: HotkeyInterpreter,
}

impl App {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let (connection, screen_num) = XCBConnection::connect(None)?;
        let atoms = Atoms::new(&connection, screen_num)?;
        let keyboard_mapping = KeyboardMapping::from_connection(&connection)?;
        Ok(Self {
            connection: Rc::new(connection),
            screen_num,
            config: Rc::new(config.ui),
            atoms: Rc::new(atoms),
            hotkey_interpreter: HotkeyInterpreter::new(config.keys),
            keyboard_mapping,
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let font = FontDescription::new(
            self.config.font.family.clone(),
            self.config.font.style,
            self.config.font.weight.into(),
            self.config.font.stretch,
        );
        let tray_container = TrayContainer::new(self.config.clone(), Rc::new(font));

        let mut window = Window::new(
            tray_container,
            self.connection.clone(),
            self.screen_num,
            &self.atoms,
            &self.config,
        )?;
        let mut tray_manager = TrayManager::new(
            self.connection.clone(),
            self.screen_num,
            window.window(),
            self.atoms.clone(),
        )?;
        let mut event_loop = EventLoop::new(self.connection.clone())?;

        tray_manager.acquire_tray_selection()?;

        window.show()?;

        event_loop.run(|event, control_flow, context| match event {
            Event::X11Event(event) => {
                self.on_x11_event(&event, &mut window, control_flow)?;
                tray_manager.process_event(&event, |event| {
                    self.on_tray_event(&event, &mut window, context, control_flow)
                })?;
                if get_window_from_event(&event) == Some(window.window()) {
                    window.on_event(&event, control_flow)?;
                }
                Ok(())
            }
            Event::DBusMessage(message) => {
                use dbus::DBusMessageType::*;

                match (
                    message.message_type(),
                    message.path().unwrap_or_default(),
                    message.member().unwrap_or_default(),
                ) {
                    (MethodCall, "/", command_str) => {
                        if let Ok(command) = Command::from_str(command_str) {
                            run_command(command, &mut window)?;
                        }
                        context.send_dbus_message(&message.new_method_return());
                    }
                    _ => {}
                }
                Ok(())
            }
            Event::Signal(_) => {
                *control_flow = ControlFlow::Break;
                Ok(())
            }
        })?;

        tray_manager.release_tray_selection()?;

        Ok(())
    }

    fn on_x11_event(
        &mut self,
        event: &protocol::Event,
        window: &mut Window<TrayContainer>,
        _control_flow: &mut ControlFlow,
    ) -> Result<(), ReplyError> {
        use protocol::Event::*;

        match event {
            KeyRelease(event) => {
                let level = if (u32::from(event.state) & u32::from(xproto::KeyButMask::SHIFT)) != 0
                {
                    1
                } else {
                    0
                };
                if let Some(key) = self.keyboard_mapping.get_key(event.detail, level) {
                    let modifiers = Modifiers::from_keymask(event.state);
                    let commands = self.hotkey_interpreter.eval(key, modifiers);
                    for command in commands {
                        if !run_command(*command, window)? {
                            break;
                        }
                    }
                }
            }
            PropertyNotify(event)
                if event.atom == self.atoms.WM_NAME || event.atom == self.atoms._NET_WM_NAME =>
            {
                if window.widget().contains_window(event.window) {
                    let title =
                        get_window_title(self.connection.as_ref(), event.window, &self.atoms)?
                            .unwrap_or_default();
                    let effect = window.widget_mut().change_title(event.window, title);
                    window.apply_effect(effect)?;
                }
            }
            ClientMessage(event)
                if event.type_ == self.atoms.WM_PROTOCOLS && event.format == 32 =>
            {
                let [protocol, ..] = event.data.as_data32();
                if protocol == self.atoms._NET_WM_PING {
                    let screen = &self.connection.setup().roots[self.screen_num];
                    let mut reply_event = event.clone();
                    reply_event.window = screen.root;
                    self.connection.send_event(
                        false,
                        screen.root,
                        xproto::EventMask::SUBSTRUCTURE_NOTIFY
                            | xproto::EventMask::SUBSTRUCTURE_REDIRECT,
                        reply_event,
                    )?;
                } else if protocol == self.atoms._NET_WM_SYNC_REQUEST {
                    window.request_redraw()?;
                } else if protocol == self.atoms.WM_DELETE_WINDOW {
                    window.hide()?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn on_tray_event(
        &mut self,
        event: &TrayEvent,
        window: &mut Window<TrayContainer>,
        context: &mut EventLoop,
        control_flow: &mut ControlFlow,
    ) -> Result<(), ReplyError> {
        match event {
            TrayEvent::BalloonMessageReceived(icon_window, balloon_message) => {
                let summary =
                    get_window_title(self.connection.as_ref(), *icon_window, &self.atoms)
                        .ok()
                        .flatten()
                        .and_then(|title| CString::new(title).ok())
                        .unwrap_or_default();
                context.send_notification(
                    summary.as_c_str(),
                    balloon_message.as_c_str(),
                    *icon_window as u32,
                    Some(balloon_message.timeout()),
                );
            }
            TrayEvent::TrayIconAdded(icon_window) => {
                let title = get_window_title(self.connection.as_ref(), *icon_window, &self.atoms)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let effect = window.widget_mut().add_tray_item(*icon_window, title);
                window.apply_effect(effect)?;
            }
            TrayEvent::TrayIconRemoved(icon_window) => {
                let effect = window.widget_mut().remove_tray_item(*icon_window);
                window.apply_effect(effect)?;
            }
            TrayEvent::SelectionCleared => {
                *control_flow = ControlFlow::Break;
            }
        }

        Ok(())
    }
}

fn run_command(command: Command, window: &mut Window<TrayContainer>) -> Result<bool, ReplyError> {
    match command {
        Command::HideWindow => {
            window.hide()?;
            let effect = window.widget_mut().select_item(None);
            window.apply_effect(effect).map(|_| true)
        }
        Command::ShowWindow => {
            window.move_at_center()?;
            window.show()?;
            Ok(true)
        }
        Command::ToggleWindow => {
            window.toggle()?;
            Ok(true)
        }
        Command::SelectItem(index) => {
            let effect = window.widget_mut().select_item(Some(index));
            window.apply_effect(effect)
        }
        Command::SelectNextItem => {
            let effect = window.widget_mut().select_next_item();
            window.apply_effect(effect)
        }
        Command::SelectPreviousItem => {
            let effect = window.widget_mut().select_previous_item();
            window.apply_effect(effect)
        }
        Command::ClickMouseButton(button) => {
            let effect = window.widget_mut().click_selected_item(button);
            window.apply_effect(effect)
        }
    }
}

fn get_window_from_event(event: &protocol::Event) -> Option<xproto::Window> {
    use protocol::Event::*;

    match event {
        ButtonPress(event) => Some(event.event),
        ButtonRelease(event) => Some(event.event),
        CirculateNotify(event) => Some(event.event),
        CirculateRequest(event) => Some(event.event),
        ClientMessage(event) => Some(event.window),
        ColormapNotify(event) => Some(event.window),
        ConfigureNotify(event) => Some(event.event),
        ConfigureRequest(event) => Some(event.window),
        CreateNotify(event) => Some(event.window),
        DestroyNotify(event) => Some(event.event),
        EnterNotify(event) => Some(event.event),
        Expose(event) => Some(event.window),
        FocusIn(event) => Some(event.event),
        FocusOut(event) => Some(event.event),
        GravityNotify(event) => Some(event.event),
        KeyPress(event) => Some(event.event),
        KeyRelease(event) => Some(event.event),
        LeaveNotify(event) => Some(event.event),
        MapNotify(event) => Some(event.event),
        MapRequest(event) => Some(event.window),
        MotionNotify(event) => Some(event.event),
        PropertyNotify(event) => Some(event.window),
        ReparentNotify(event) => Some(event.event),
        ResizeRequest(event) => Some(event.window),
        SelectionClear(event) => Some(event.owner),
        SelectionNotify(event) => Some(event.requestor),
        SelectionRequest(event) => Some(event.owner),
        UnmapNotify(event) => Some(event.event),
        VisibilityNotify(event) => Some(event.window),
        _ => None,
    }
}

fn get_window_title<Connection: self::Connection>(
    connection: &Connection,
    window: xproto::Window,
    atoms: &Atoms,
) -> Result<Option<String>, ReplyError> {
    if let Some(title) = utils::get_variable_property(
        connection,
        window,
        atoms._NET_WM_NAME,
        xproto::AtomEnum::STRING,
        256,
    )?
    .and_then(null_terminated_bytes_to_string)
    {
        return Ok(Some(title));
    }

    if let Some(title) = utils::get_variable_property(
        connection,
        window,
        atoms.WM_NAME,
        xproto::AtomEnum::STRING,
        256,
    )?
    .and_then(null_terminated_bytes_to_string)
    {
        return Ok(Some(title));
    }

    if let Some(class_name) = utils::get_variable_property(
        connection,
        window,
        atoms.WM_CLASS,
        xproto::AtomEnum::STRING,
        256,
    )?
    .and_then(null_terminated_bytes_to_string)
    {
        return Ok(Some(class_name));
    }

    Ok(None)
}

fn null_terminated_bytes_to_string(mut bytes: Vec<u8>) -> Option<String> {
    if let Some(null_position) = bytes.iter().position(|c| *c == 0) {
        bytes.resize(null_position, 0);
    }
    String::from_utf8(bytes).ok()
}
