use anyhow::{anyhow, Context as _};
use keytray_shell::event::{ControlFlow, Event, EventLoop, KeyState, Modifiers};
use keytray_shell::graphics::{FontDescription, PhysicalPoint, PhysicalSize, Size};
use keytray_shell::window::Window;
use keytray_shell::xkb;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use x11rb::connection::Connection;
use x11rb::protocol;
use x11rb::protocol::xkb::ConnectionExt as _;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

use crate::command::Command;
use crate::config::{Config, WindowConfig};
use crate::hotkey::HotkeyInterpreter;
use crate::tray_container::TrayContainer;
use crate::tray_manager::{SystemTrayColors, SystemTrayOrientation, TrayEvent, TrayManager};

#[derive(Debug)]
pub struct App {
    connection: Rc<XCBConnection>,
    screen_num: usize,
    atoms: Atoms,
    window: ManuallyDrop<Window<TrayContainer>>,
    tray_manager: ManuallyDrop<TrayManager<XCBConnection>>,
    keyboard_state: xkb::State,
    hotkey_interpreter: HotkeyInterpreter,
}

impl App {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let (connection, screen_num) =
            XCBConnection::connect(None).context("connect to X server")?;
        let connection = Rc::new(connection);

        let atoms = Atoms::new(connection.as_ref())?
            .reply()
            .context("intern atoms")?;

        let systemtray_colors = SystemTrayColors::new(
            config.ui.normal_item_foreground,
            config.ui.selected_item_foreground,
            config.ui.selected_item_foreground,
            config.ui.selected_item_foreground,
        );
        let font = FontDescription::new(
            config.ui.font_family.clone(),
            config.ui.font_style,
            config.ui.font_weight.into(),
            config.ui.font_stretch,
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
        )
        .context("create window")?;

        configure_window(&connection, window.id(), config.window, &atoms)?;

        let tray_manager = TrayManager::new(
            connection.clone(),
            screen_num,
            SystemTrayOrientation::VERTICAL,
            systemtray_colors,
        )?;

        setup_xkb_extension(&connection)?;

        let keyboard_state = {
            let context = xkb::Context::new();
            let device_id = xkb::DeviceId::core_keyboard(&connection)
                .context("get the core keyboard device ID")?;
            let keymap = xkb::Keymap::from_device(context, &connection, device_id)
                .context("create a keymap from a device")?;
            xkb::State::from_keymap(keymap)
        };

        {
            let screen = &connection.setup().roots[screen_num];
            for key in &config.global_hotkeys {
                let keycode = keyboard_state
                    .lookup_keycode(key.keysym())
                    .context("lookup keycode")?;
                let modifiers = key.modifiers().without_locks();
                for modifiers in [
                    modifiers,
                    modifiers | Modifiers::CAPS_LOCK,
                    modifiers | Modifiers::NUM_LOCK,
                    modifiers | Modifiers::CAPS_LOCK | Modifiers::NUM_LOCK,
                ] {
                    connection
                        .grab_key(
                            true,
                            screen.root,
                            modifiers,
                            keycode as u8,
                            xproto::GrabMode::ASYNC,
                            xproto::GrabMode::ASYNC,
                        )?
                        .check()
                        .context("grab key")?;
                }
            }
        }

        let all_hotkeys = config
            .hotkeys
            .into_iter()
            .chain(config.global_hotkeys.into_iter());
        let hotkey_interpreter = HotkeyInterpreter::new(all_hotkeys);

        Ok(Self {
            connection,
            screen_num,
            atoms,
            window: ManuallyDrop::new(window),
            tray_manager: ManuallyDrop::new(tray_manager),
            keyboard_state,
            hotkey_interpreter,
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut event_loop =
            EventLoop::new(self.connection.clone()).context("create event loop")?;

        self.tray_manager
            .acquire_tray_selection()
            .context("acquire tray selection")?;
        self.window.show().context("show window")?;

        event_loop.run(|event, _event_loop, control_flow| match event {
            Event::X11Event(event) => {
                match self.tray_manager.process_event(self.window.id(), &event) {
                    Ok(Some(event)) => {
                        self.on_tray_event(&event, control_flow)?;
                    }
                    Ok(None) => {}
                    Err(_error) => {
                        // TODO: log error
                    }
                }
                self.window.on_event(&event, control_flow)?;
                self.on_x11_event(&event, control_flow)?;
                Ok(())
            }
            Event::Signal(_) => {
                *control_flow = ControlFlow::Break;
                Ok(())
            }
        })
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
                    .update_key(event.detail as u32, KeyState::Down);
            }
            KeyRelease(event) => {
                self.keyboard_state
                    .update_key(event.detail as u32, KeyState::Up);
                let keysym = self.keyboard_state.get_keysym(event.detail as u32);
                let modifiers = self.keyboard_state.get_modifiers();
                let commands = self.hotkey_interpreter.eval(keysym, modifiers);
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
                if let Some(tray_item) = self.window.widget_mut().get_item_mut(event.window) {
                    let title =
                        get_window_title(self.connection.as_ref(), event.window, &self.atoms)?
                            .unwrap_or_default();
                    let effect = tray_item.change_title(title);
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
            XkbStateNotify(event) => self.keyboard_state.update_mask(event),
            _ => {}
        }

        Ok(())
    }

    fn on_tray_event(
        &mut self,
        event: &TrayEvent,
        control_flow: &mut ControlFlow,
    ) -> anyhow::Result<()> {
        match event {
            TrayEvent::MessageReceived(_icon_window, _message) => {
                // TODO: Handle balloon message
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

fn configure_window(
    connection: &XCBConnection,
    window: xproto::Window,
    config: WindowConfig,
    atoms: &Atoms,
) -> anyhow::Result<()> {
    connection
        .change_property32(
            xproto::PropMode::REPLACE,
            window,
            atoms.WM_PROTOCOLS,
            xproto::AtomEnum::ATOM,
            &[
                atoms._NET_WM_PING,
                atoms._NET_WM_SYNC_REQUEST,
                atoms.WM_DELETE_WINDOW,
            ],
        )?
        .check()
        .context("set WM_PROTOCOLS property")?;

    {
        connection
            .change_property8(
                xproto::PropMode::REPLACE,
                window,
                xproto::AtomEnum::WM_NAME,
                xproto::AtomEnum::STRING,
                config.name.as_bytes(),
            )?
            .check()
            .context("set WM_NAME property")?;
        connection
            .change_property8(
                xproto::PropMode::REPLACE,
                window,
                atoms._NET_WM_NAME,
                atoms.UTF8_STRING,
                config.name.as_bytes(),
            )?
            .check()
            .context("set _NET_WM_NAME property")?;
    }

    {
        let class_string = format!("{}\0{}", config.class.as_ref(), config.class.as_ref());
        connection
            .change_property8(
                xproto::PropMode::REPLACE,
                window,
                xproto::AtomEnum::WM_CLASS,
                xproto::AtomEnum::STRING,
                class_string.as_bytes(),
            )?
            .check()
            .context("set WM_CLASS property")?;
    }

    connection
        .change_property32(
            xproto::PropMode::REPLACE,
            window,
            atoms._NET_WM_WINDOW_TYPE,
            xproto::AtomEnum::ATOM,
            &[atoms._NET_WM_WINDOW_TYPE_DIALOG],
        )?
        .check()
        .context("set _NET_WM_WINDOW_TYPE property")?;

    if config.sticky {
        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                atoms._NET_WM_STATE,
                xproto::AtomEnum::ATOM,
                &[atoms._NET_WM_STATE_STICKY],
            )?
            .check()
            .context("set _NET_WM_STATE property")?;
    }

    Ok(())
}

fn setup_xkb_extension(connection: &XCBConnection) -> anyhow::Result<()> {
    let reply = connection
        .xkb_use_extension(1, 0)?
        .reply()
        .context("init xkb extension")?;
    if !reply.supported {
        anyhow!("xkb extension not supported.");
    }

    {
        let values = x11rb::protocol::xkb::SelectEventsAux::new();
        connection
            .xkb_select_events(
                protocol::xkb::ID::USE_CORE_KBD.into(), // device_spec
                0u16,                                   // clear
                protocol::xkb::EventType::STATE_NOTIFY, // select_all
                0u16,                                   // affect_map
                0u16,                                   // map
                &values,                                // details
            )?
            .check()
            .context("select xkb events")?;
    }

    Ok(())
}

fn run_command(
    connection: &XCBConnection,
    screen_num: usize,
    window: &mut Window<TrayContainer>,
    command: Command,
) -> anyhow::Result<bool> {
    match command {
        Command::HideWindow => {
            if window.is_mapped() {
                window.hide().context("hide window")?;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Command::ShowWindow => {
            if window.is_mapped() {
                Ok(false)
            } else {
                let position = get_window_position(connection, screen_num, window.size());
                window
                    .move_position(position)
                    .context("move window position")?;
                window.show().context("show window")?;
                Ok(true)
            }
        }
        Command::ToggleWindow => {
            if window.is_mapped() {
                window.hide().context("hide window")?;
            } else {
                let position = get_window_position(connection, screen_num, window.size());
                window
                    .move_position(position)
                    .context("move window position")?;
                window.show().context("show window")?;
            }
            Ok(true)
        }
        Command::DeselectItem => {
            let effect = window.widget_mut().select_item(None);
            Ok(window.apply_effect(effect)?)
        }
        Command::SelectItem(index) => {
            let effect = window.widget_mut().select_item(Some(index));
            Ok(window.apply_effect(effect)?)
        }
        Command::SelectNextItem => {
            let effect = window.widget_mut().select_next_item();
            Ok(window.apply_effect(effect)?)
        }
        Command::SelectPreviousItem => {
            let effect = window.widget_mut().select_previous_item();
            Ok(window.apply_effect(effect)?)
        }
        Command::ClickMouseButton(button) => {
            let effect = window.widget_mut().click_selected_item(button);
            Ok(window.apply_effect(effect)?)
        }
    }
}

fn get_window_title<Connection: self::Connection>(
    connection: &Connection,
    window: xproto::Window,
    atoms: &Atoms,
) -> anyhow::Result<Option<String>> {
    let reply = connection
        .get_property(
            false,
            window,
            atoms._NET_WM_NAME,
            atoms.UTF8_STRING,
            0,
            256 / 4,
        )
        .context("get _NET_WM_NAME property")?
        .reply()?;
    if let Some(title) = reply
        .value8()
        .and_then(|bytes| String::from_utf8(bytes.collect()).ok())
    {
        return Ok(Some(title));
    }

    let reply = connection
        .get_property(
            false,
            window,
            xproto::AtomEnum::WM_NAME,
            xproto::AtomEnum::STRING,
            0,
            256 / 4,
        )
        .context("get WM_NAME property")?
        .reply()?;
    if let Some(title) = reply
        .value8()
        .and_then(|bytes| String::from_utf8(bytes.collect()).ok())
    {
        return Ok(Some(title));
    }

    let reply = connection
        .get_property(
            false,
            window,
            xproto::AtomEnum::WM_CLASS,
            xproto::AtomEnum::STRING,
            0,
            256 / 4,
        )
        .context("get WM_CLASS property")?
        .reply()?;
    if let Some(class_name) = reply
        .value8()
        .and_then(|bytes| null_terminated_bytes_to_string(bytes.collect()))
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
        x: (screen.width_in_pixels as i32 - size.width as i32) / 2,
        y: (screen.height_in_pixels as i32 - size.height as i32) / 2,
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
