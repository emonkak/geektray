use anyhow::Context as _;
use libdbus_sys as dbus;
use std::ffi::CString;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use std::str::FromStr;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xkb::ConnectionExt as _;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::xproto;
use x11rb::protocol;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

use crate::command::Command;
use crate::config::Config;
use crate::event_loop::{Event, EventLoop};
use crate::graphics::{FontDescription, PhysicalPoint, PhysicalSize, Size};
use crate::tray_container::TrayContainer;
use crate::tray_manager::{SystemTrayOrientation, TrayEvent, TrayManager};
use crate::ui::xkb;
use crate::ui::{ControlFlow, KeyInterpreter, KeyState, Window};
use crate::utils;

#[derive(Debug)]
pub struct App {
    connection: Rc<XCBConnection>,
    screen_num: usize,
    atoms: Atoms,
    window: ManuallyDrop<Window<TrayContainer>>,
    tray_manager: ManuallyDrop<TrayManager<XCBConnection>>,
    keyboard_state: xkb::State,
    key_interpreter: KeyInterpreter,
}

impl App {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let (connection, screen_num) = XCBConnection::connect(None)?;
        let connection = Rc::new(connection);

        let atoms = Atoms::new(connection.as_ref())?.reply()?;

        let font = FontDescription::new(
            config.ui.font.family.clone(),
            config.ui.font.style,
            config.ui.font.weight.into(),
            config.ui.font.stretch,
        );
        let tray_container = TrayContainer::new(Rc::new(config.ui), Rc::new(font));

        let window = Window::new(
            tray_container,
            connection.clone(),
            screen_num,
            Size {
                width: config.window.initial_width,
                height: 0.0,
            },
            get_window_position,
        )?;

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window.id(),
                atoms.WM_PROTOCOLS,
                xproto::AtomEnum::ATOM,
                &[
                    atoms._NET_WM_PING,
                    atoms._NET_WM_SYNC_REQUEST,
                    atoms.WM_DELETE_WINDOW,
                ],
            )?
            .check()?;

        {
            connection
                .change_property8(
                    xproto::PropMode::REPLACE,
                    window.id(),
                    xproto::AtomEnum::WM_NAME,
                    xproto::AtomEnum::STRING,
                    config.window.name.as_bytes(),
                )?
                .check()?;
            connection
                .change_property8(
                    xproto::PropMode::REPLACE,
                    window.id(),
                    atoms._NET_WM_NAME,
                    atoms.UTF8_STRING,
                    config.window.name.as_bytes(),
                )?
                .check()?;
        }

        {
            let class_string = format!(
                "{}\0{}",
                config.window.class.as_ref(),
                config.window.class.as_ref()
            );
            connection
                .change_property8(
                    xproto::PropMode::REPLACE,
                    window.id(),
                    xproto::AtomEnum::WM_CLASS,
                    xproto::AtomEnum::STRING,
                    class_string.as_bytes(),
                )?
                .check()?;
        }

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window.id(),
                atoms._NET_WM_WINDOW_TYPE,
                xproto::AtomEnum::ATOM,
                &[atoms._NET_WM_WINDOW_TYPE_DIALOG],
            )?
            .check()?;

        if config.window.sticky {
            connection
                .change_property32(
                    xproto::PropMode::REPLACE,
                    window.id(),
                    atoms._NET_WM_STATE,
                    xproto::AtomEnum::ATOM,
                    &[atoms._NET_WM_STATE_STICKY],
                )?
                .check()?;
        }

        connection.xkb_use_extension(1, 0)?
            .reply()
            .context("init xkb extension")?;

        let tray_manager = TrayManager::new(connection.clone(), screen_num, SystemTrayOrientation::VERTICAL)?;
        let keyboard_state = {
            let context = xkb::Context::new();
            let device_id = xkb::DeviceId::core_keyboard(&connection)
                .context("get the core keyboard device ID")?;
            let keymap = xkb::Keymap::from_device(context, &connection, device_id)
                .context("create a keymap from a device")?;
            xkb::State::from_keymap(keymap)
        };
        let key_interpreter = KeyInterpreter::new(config.keys);

        Ok(Self {
            connection,
            screen_num,
            atoms,
            window: ManuallyDrop::new(window),
            tray_manager: ManuallyDrop::new(tray_manager),
            keyboard_state,
            key_interpreter,
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut event_loop = EventLoop::new(self.connection.clone())?;

        self.tray_manager.acquire_tray_selection()?;
        self.window.show()?;

        event_loop.run(|event, control_flow, context| match event {
            Event::X11Event(event) => {
                if let Ok(Some(event)) = self.tray_manager.process_event(self.window.id(), &event) {
                    self.on_tray_event(&event, context, control_flow)?;
                }
                self.window.on_event(&event, control_flow)?;
                self.on_x11_event(&event, control_flow)?;
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
                            run_command(
                                &self.connection,
                                self.screen_num,
                                &mut self.window,
                                command,
                            )?;
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

        Ok(())
    }

    fn on_x11_event(
        &mut self,
        event: &protocol::Event,
        _control_flow: &mut ControlFlow,
    ) -> anyhow::Result<()> {
        use protocol::Event::*;

        match event {
            KeyPress(event) => {
                self.keyboard_state
                    .update(event.detail as u32, KeyState::Down);
            }
            KeyRelease(event) => {
                self.keyboard_state
                    .update(event.detail as u32, KeyState::Up);
                let event = self
                    .keyboard_state
                    .key_event(event.detail as u32, KeyState::Up);
                let commands = self
                    .key_interpreter
                    .eval(event.keysym, event.modifiers);
                for command in commands {
                    if !run_command(
                        &self.connection,
                        self.screen_num,
                        &mut self.window,
                        *command,
                    )? {
                        break;
                    }
                }
            }
            PropertyNotify(event)
                if event.atom == u32::from(xproto::AtomEnum::WM_NAME)
                    || event.atom == u32::from(xproto::AtomEnum::WM_CLASS)
                    || event.atom == self.atoms._NET_WM_NAME =>
            {
                if self.window.widget().contains_window(event.window) {
                    let title =
                        get_window_title(self.connection.as_ref(), event.window, &self.atoms)?
                            .unwrap_or_default();
                    let effect = self.window.widget_mut().change_title(event.window, title);
                    self.window.apply_effect(effect)?;
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
                    self.window.request_redraw()?;
                } else if protocol == self.atoms.WM_DELETE_WINDOW {
                    self.window.hide()?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn on_tray_event(
        &mut self,
        event: &TrayEvent,
        context: &mut EventLoop<XCBConnection>,
        control_flow: &mut ControlFlow,
    ) -> anyhow::Result<()> {
        match event {
            TrayEvent::BalloonMessageReceived(icon_window, balloon_message) => {
                let summary =
                    get_window_title(self.connection.as_ref(), *icon_window, &self.atoms)?
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
                let title = get_window_title(self.connection.as_ref(), *icon_window, &self.atoms)?
                    .unwrap_or_default();
                let effect = self.window.widget_mut().add_tray_item(*icon_window, title);
                self.window.apply_effect(effect)?;
            }
            TrayEvent::TrayIconRemoved(icon_window) => {
                let effect = self.window.widget_mut().remove_tray_item(*icon_window);
                self.window.apply_effect(effect)?;
            }
            TrayEvent::SelectionCleared => {
                *control_flow = ControlFlow::Break;
            }
        }

        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.tray_manager);
            ManuallyDrop::drop(&mut self.window);
        }
    }
}

fn run_command(
    connection: &XCBConnection,
    screen_num: usize,
    window: &mut Window<TrayContainer>,
    command: Command,
) -> Result<bool, ReplyError> {
    match command {
        Command::HideWindow => {
            window.hide()?;
            let effect = window.widget_mut().select_item(None);
            window.apply_effect(effect).map(|_| true)
        }
        Command::ShowWindow => {
            let position = get_window_position(connection, screen_num, window.size());
            window.move_position(position)?;
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

fn get_window_title<Connection: self::Connection>(
    connection: &Connection,
    window: xproto::Window,
    atoms: &Atoms,
) -> Result<Option<String>, ReplyError> {
    if let Some(title) = utils::get_variable_property(
        connection,
        window,
        atoms._NET_WM_NAME,
        atoms.UTF8_STRING,
        256,
    )?
    .and_then(|bytes| String::from_utf8(bytes).ok())
    {
        return Ok(Some(title));
    }

    if let Some(title) = utils::get_variable_property(
        connection,
        window,
        xproto::AtomEnum::WM_NAME.into(),
        xproto::AtomEnum::STRING.into(),
        256,
    )?
    .and_then(|bytes| String::from_utf8(bytes).ok())
    {
        return Ok(Some(title));
    }

    if let Some(class_name) = utils::get_variable_property(
        connection,
        window,
        xproto::AtomEnum::WM_CLASS.into(),
        xproto::AtomEnum::STRING.into(),
        256,
    )?
    .and_then(null_terminated_bytes_to_string)
    {
        return Ok(Some(class_name));
    }

    Ok(None)
}

fn get_window_position(
    connection: &XCBConnection,
    screen_num: usize,
    size: PhysicalSize,
) -> PhysicalPoint {
    let screen = &connection.setup().roots[screen_num];
    PhysicalPoint {
        x: (screen.width_in_pixels as f64 / 2.0 - size.width as f64 / 2.0) as i32,
        y: (screen.height_in_pixels as f64 / 2.0 - size.height as f64 / 2.0) as i32,
    }
}

fn null_terminated_bytes_to_string(mut bytes: Vec<u8>) -> Option<String> {
    if let Some(null_position) = bytes.iter().position(|c| *c == 0) {
        bytes.resize(null_position, 0);
    }
    String::from_utf8(bytes).ok()
}

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        UTF8_STRING,
        WM_DELETE_WINDOW,
        WM_PROTOCOLS,
        _NET_WM_NAME,
        _NET_WM_PING,
        _NET_WM_STATE,
        _NET_WM_STATE_STICKY,
        _NET_WM_SYNC_REQUEST,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_DIALOG,
    }
}
