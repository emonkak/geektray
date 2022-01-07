use libdbus_sys as dbus;
use std::ffi::CString;
use std::mem::ManuallyDrop;
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
use crate::config::Config;
use crate::event_loop::{ControlFlow, Event, EventLoop};
use crate::graphics::FontDescription;
use crate::main_window::MainWindow;
use crate::tray_container::TrayContainer;
use crate::tray_manager::{TrayEvent, TrayManager};
use crate::ui::{KeyMappingManager, KeyboardMapping, Modifiers};
use crate::utils;

#[derive(Debug)]
pub struct App {
    connection: Rc<XCBConnection>,
    screen_num: usize,
    atoms: Rc<Atoms>,
    main_window: ManuallyDrop<MainWindow<TrayContainer>>,
    tray_manager: ManuallyDrop<TrayManager<XCBConnection>>,
    keyboard_mapping: KeyboardMapping,
    key_mapping_manager: KeyMappingManager,
}

impl App {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let (connection, screen_num) = XCBConnection::connect(None)?;
        let connection = Rc::new(connection);

        let atoms = Rc::new(Atoms::new(connection.as_ref(), screen_num)?);

        let font = FontDescription::new(
            config.ui.font.family.clone(),
            config.ui.font.style,
            config.ui.font.weight.into(),
            config.ui.font.stretch,
        );
        let tray_container = TrayContainer::new(Rc::new(config.ui), Rc::new(font));

        let main_window = MainWindow::new(
            tray_container,
            connection.clone(),
            screen_num,
            atoms.as_ref(),
            &config.window,
        )?;

        let tray_manager = TrayManager::new(
            connection.clone(),
            screen_num,
            main_window.id(),
            atoms.clone(),
        );

        let keyboard_mapping = KeyboardMapping::from_connection(connection.as_ref())?;
        let key_mapping_manager = KeyMappingManager::new(config.keys);

        Ok(Self {
            connection,
            screen_num,
            atoms,
            main_window: ManuallyDrop::new(main_window),
            tray_manager: ManuallyDrop::new(tray_manager),
            keyboard_mapping,
            key_mapping_manager,
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut event_loop = EventLoop::new(self.connection.clone())?;

        self.tray_manager.acquire_tray_selection()?;

        self.main_window.show()?;

        event_loop.run(|event, control_flow, context| match event {
            Event::X11Event(event) => {
                if let Some(event) = self.tray_manager.process_event(&event)? {
                    self.on_tray_event(&event, context, control_flow)?;
                }
                if get_window_from_event(&event) == Some(self.main_window.id()) {
                    self.main_window.on_event(&event, control_flow)?;
                }
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
                            run_command(command, &mut self.main_window)?;
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
            KeyRelease(event) => {
                let level = if (u32::from(event.state) & u32::from(xproto::KeyButMask::SHIFT)) != 0
                {
                    1
                } else {
                    0
                };
                if let Some(key) = self.keyboard_mapping.get_key(event.detail, level) {
                    let modifiers = Modifiers::from_keymask(event.state);
                    let commands = self.key_mapping_manager.eval(key, modifiers);
                    for command in commands {
                        if !run_command(*command, &mut self.main_window)? {
                            break;
                        }
                    }
                }
            }
            PropertyNotify(event)
                if event.atom == u32::from(xproto::AtomEnum::WM_NAME)
                    || event.atom == u32::from(xproto::AtomEnum::WM_CLASS)
                    || event.atom == self.atoms._NET_WM_NAME =>
            {
                if self.main_window.widget().contains_window(event.window) {
                    let title =
                        get_window_title(self.connection.as_ref(), event.window, &self.atoms)?
                            .unwrap_or_default();
                    let effect = self
                        .main_window
                        .widget_mut()
                        .change_title(event.window, title);
                    self.main_window.apply_effect(effect)?;
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
                    self.main_window.request_redraw()?;
                } else if protocol == self.atoms.WM_DELETE_WINDOW {
                    self.main_window.hide()?;
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
                let effect = self
                    .main_window
                    .widget_mut()
                    .add_tray_item(*icon_window, title);
                self.main_window.apply_effect(effect)?;
            }
            TrayEvent::TrayIconRemoved(icon_window) => {
                let effect = self.main_window.widget_mut().remove_tray_item(*icon_window);
                self.main_window.apply_effect(effect)?;
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
            ManuallyDrop::drop(&mut self.main_window);
        }
    }
}

fn run_command(
    command: Command,
    main_window: &mut MainWindow<TrayContainer>,
) -> Result<bool, ReplyError> {
    match command {
        Command::HideWindow => {
            main_window.hide()?;
            let effect = main_window.widget_mut().select_item(None);
            main_window.apply_effect(effect).map(|_| true)
        }
        Command::ShowWindow => {
            main_window.adjust_position()?;
            main_window.show()?;
            Ok(true)
        }
        Command::ToggleWindow => {
            main_window.toggle()?;
            Ok(true)
        }
        Command::SelectItem(index) => {
            let effect = main_window.widget_mut().select_item(Some(index));
            main_window.apply_effect(effect)
        }
        Command::SelectNextItem => {
            let effect = main_window.widget_mut().select_next_item();
            main_window.apply_effect(effect)
        }
        Command::SelectPreviousItem => {
            let effect = main_window.widget_mut().select_previous_item();
            main_window.apply_effect(effect)
        }
        Command::ClickMouseButton(button) => {
            let effect = main_window.widget_mut().click_selected_item(button);
            main_window.apply_effect(effect)
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

fn null_terminated_bytes_to_string(mut bytes: Vec<u8>) -> Option<String> {
    if let Some(null_position) = bytes.iter().position(|c| *c == 0) {
        bytes.resize(null_position, 0);
    }
    String::from_utf8(bytes).ok()
}
