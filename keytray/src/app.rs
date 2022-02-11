use anyhow::{anyhow, Context as _};
use keytray_shell::event::{ControlFlow, Event, EventLoop, EventLoopContext, KeyState, Modifiers};
use keytray_shell::geometrics::Size;
use keytray_shell::window::Window;
use keytray_shell::xkb;
use std::mem::ManuallyDrop;
use std::process;
use std::rc::Rc;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol;
use x11rb::protocol::damage::ConnectionExt as _;
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

pub struct App {
    connection: Rc<XCBConnection>,
    screen_num: usize,
    atoms: Atoms,
    window: ManuallyDrop<Window<TrayContainer>>,
    window_config: WindowConfig,
    tray_manager: ManuallyDrop<TrayManager<XCBConnection>>,
    keyboard_state: xkb::State,
    hotkey_interpreter: HotkeyInterpreter,
}

impl App {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let (connection, screen_num) =
            XCBConnection::connect(None).context("connect to X server")?;
        let connection = Rc::new(connection);

        setup_xkb_extension(&connection)?;

        setup_damage_extension(&connection)?;

        let screen = &connection.setup().roots[screen_num];
        let (visual_id, depth) =
            match find_visual_from_screen(screen, 32, xproto::VisualClass::TRUE_COLOR) {
                Some(visual) => (visual.visual_id, 32),
                None => (screen.root_visual, screen.root_depth),
            };
        let colormap = connection.generate_id()?;

        connection
            .create_colormap(
                xproto::ColormapAlloc::NONE,
                colormap,
                screen.root,
                visual_id,
            )?
            .check()?;

        let atoms = Atoms::new(connection.as_ref())?
            .reply()
            .context("intern atoms")?;

        let tray_manager = TrayManager::new(
            connection.clone(),
            screen_num,
            visual_id,
            SystemTrayOrientation::VERTICAL,
            SystemTrayColors::new(
                config.ui.normal_item_foreground,
                config.ui.normal_item_foreground,
                config.ui.normal_item_foreground,
                config.ui.normal_item_foreground,
            ),
        )?;

        let window = Window::new(
            TrayContainer::new(Rc::new(config.ui)),
            connection.clone(),
            screen_num,
            depth,
            visual_id,
            colormap,
            Size {
                width: config.window.width,
                height: 0.0,
            },
            config.window.override_redirect,
        )
        .context("create window")?;

        configure_window(&connection, window.id(), &config.window, &atoms)?;

        let keyboard_state = {
            let context = xkb::Context::new();
            let device_id = xkb::DeviceId::core_keyboard(&connection)
                .context("get the core keyboard device ID")?;
            let keymap = xkb::Keymap::from_device(context, &connection, device_id)
                .context("create a keymap from a device")?;
            xkb::State::from_keymap(keymap)
        };

        for key in &config.global_hotkeys {
            let keycode = keyboard_state
                .lookup_keycode(key.keysym())
                .context("lookup keycode")?;
            grab_key(&connection, screen_num, keycode, key.modifiers()).context("grab_key")?;
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
            window_config: config.window,
            tray_manager: ManuallyDrop::new(tray_manager),
            keyboard_state,
            hotkey_interpreter,
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        let mut event_loop =
            EventLoop::new(self.connection.clone()).context("create event loop")?;

        if self.window.override_redirect() {
            // If `override_redirect` is enabled, we need to monitor mapping of other windows.
            let screen = &self.connection.setup().roots[self.screen_num];
            let values = xproto::ChangeWindowAttributesAux::new()
                .event_mask(xproto::EventMask::SUBSTRUCTURE_NOTIFY);
            self.connection
                .change_window_attributes(screen.root, &values)?
                .check()?;
        }

        self.tray_manager
            .acquire_tray_selection()
            .context("acquire tray selection")?;

        event_loop.run(|event, context, control_flow| {
            self.window.process_event(&event, context, control_flow)?;

            match event {
                Event::X11Event(event) => {
                    match self.tray_manager.process_event(&event, self.window.id()) {
                        Ok(Some(event)) => {
                            self.on_tray_event(event, context, control_flow)?;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            log::warn!("Error while processing event by TrayManager: {}", error);
                        }
                    }
                    self.on_x11_event(&event, context, control_flow)?;
                    Ok(())
                }
                Event::Timer(_timer) => Ok(()),
                Event::Signal(_signal) => {
                    *control_flow = ControlFlow::Break;
                    Ok(())
                }
                Event::NextTick => Ok(()),
            }
        })?;

        Ok(())
    }

    fn on_x11_event(
        &mut self,
        event: &protocol::Event,
        context: &mut EventLoopContext,
        _control_flow: &mut ControlFlow,
    ) -> anyhow::Result<()> {
        use protocol::Event::*;

        match event {
            FocusOut(event) => {
                if event.mode == xproto::NotifyMode::NORMAL && event.event == self.window.id() {
                    self.window.hide().context("hide window")?;
                }
            }
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
                    if !run_command(&mut self.window, *command, context)? {
                        break;
                    }
                }
            }
            LeaveNotify(event) => {
                if self.window_config.auto_close
                    && event.event == self.window.id()
                    && matches!(
                        event.detail,
                        xproto::NotifyDetail::ANCESTOR | xproto::NotifyDetail::NONLINEAR
                    )
                {
                    self.window.hide().context("hide window")?;
                }
            }
            MapNotify(event) => {
                if event.window == event.event {
                    // from STRUCTURE_NOTIFY
                    if self.window_config.override_redirect && event.window == self.window.id() {
                        grab_keyboard(self.connection.as_ref(), self.screen_num)
                            .context("grab keyboard")?;
                    }
                } else {
                    // from SUBSTRUCTURE_NOTIFY
                    if self.window_config.override_redirect
                        && event.window != self.window.id()
                        && !event.override_redirect
                    {
                        // It maybe hidden under other windows, so lift the window.
                        self.window.raise().context("raise window")?;
                    }
                }
            }
            UnmapNotify(event) => {
                if event.window == event.event && event.window == self.window.id() {
                    if self.window_config.override_redirect {
                        ungrab_keyboard(self.connection.as_ref()).context("ungrab keyboard")?;
                    }
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
                    self.connection
                        .send_event(
                            false,
                            screen.root,
                            xproto::EventMask::SUBSTRUCTURE_NOTIFY
                                | xproto::EventMask::SUBSTRUCTURE_REDIRECT,
                            reply_event,
                        )
                        .context("reply _NET_WM_PING")?;
                } else if protocol == self.atoms._NET_WM_SYNC_REQUEST {
                    self.window.request_redraw();
                } else if protocol == self.atoms.WM_DELETE_WINDOW {
                    self.window.hide().context("hide window")?;
                }
            }
            XkbStateNotify(event) => self.keyboard_state.update_mask(event),
            _ => {}
        }

        Ok(())
    }

    fn on_tray_event(
        &mut self,
        event: TrayEvent,
        context: &mut EventLoopContext,
        control_flow: &mut ControlFlow,
    ) -> anyhow::Result<()> {
        match event {
            TrayEvent::MessageReceived(icon_window, message) => {
                log::info!(
                    "Tray message from window {}: {}",
                    icon_window,
                    message.as_str()
                )
            }
            TrayEvent::TrayIconAdded(icon) => {
                let effect = self.window.widget_mut().add_tray_item(icon);
                self.window.apply_effect(effect, context)?;
            }
            TrayEvent::TrayIconUpdated(icon) => {
                let effect = self.window.widget_mut().update_tray_item(icon);
                self.window.apply_effect(effect, context)?;
            }
            TrayEvent::TrayIconRemoved(icon) => {
                let effect = self.window.widget_mut().remove_tray_item(icon.window());
                self.window.apply_effect(effect, context)?;
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
    window: &mut Window<TrayContainer>,
    command: Command,
    context: &mut EventLoopContext,
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
                window.show().context("show window")?;
                Ok(true)
            }
        }
        Command::ToggleWindow => {
            if window.is_mapped() {
                window.hide().context("hide window")?;
            } else {
                window.show().context("show window")?;
            }
            Ok(true)
        }
        Command::DeselectItem => {
            let effect = window.widget_mut().select_item(None);
            Ok(window.apply_effect(effect, context)?)
        }
        Command::SelectItem(index) => {
            let effect = window.widget_mut().select_item(Some(index));
            Ok(window.apply_effect(effect, context)?)
        }
        Command::SelectNextItem => {
            let effect = window.widget_mut().select_next_item();
            Ok(window.apply_effect(effect, context)?)
        }
        Command::SelectPreviousItem => {
            let effect = window.widget_mut().select_previous_item();
            Ok(window.apply_effect(effect, context)?)
        }
        Command::ClickMouseButton(button) => {
            let effect = window.widget_mut().click_selected_item(button);
            Ok(window.apply_effect(effect, context)?)
        }
    }
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

fn setup_damage_extension(connection: &XCBConnection) -> anyhow::Result<()> {
    let reply = connection
        .damage_query_version(1, 1)?
        .reply()
        .context("init damage extension")?;
    if reply.major_version != 1 || reply.minor_version != 1 {
        anyhow!("damage extension not supported");
    }
    Ok(())
}

fn configure_window(
    connection: &XCBConnection,
    window: xproto::Window,
    config: &WindowConfig,
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
            atoms._NET_WM_PID,
            xproto::AtomEnum::CARDINAL,
            &[process::id()],
        )?
        .check()
        .context("set _NET_WM_PID property")?;

    connection
        .change_property32(
            xproto::PropMode::REPLACE,
            window,
            atoms._NET_WM_WINDOW_TYPE,
            xproto::AtomEnum::ATOM,
            &[
                atoms._NET_WM_WINDOW_TYPE_NORMAL,
                atoms._NET_WM_WINDOW_TYPE_UTILITY,
            ],
        )?
        .check()
        .context("set _NET_WM_WINDOW_TYPE property")?;

    connection
        .change_property32(
            xproto::PropMode::REPLACE,
            window,
            atoms._NET_WM_STATE,
            xproto::AtomEnum::ATOM,
            &[
                atoms._NET_WM_STATE_ABOVE,
                atoms._NET_WM_STATE_STAYS_ON_TOP,
                atoms._NET_WM_STATE_STICKY,
            ],
        )?
        .check()
        .context("set _NET_WM_STATE property")?;

    connection
        .change_property32(
            xproto::PropMode::REPLACE,
            window,
            atoms._NET_WM_DESKTOP,
            xproto::AtomEnum::CARDINAL,
            &[0xffffffff],
        )?
        .check()
        .context("set _NET_WM_DESKTOP property")?;

    Ok(())
}

fn find_visual_from_screen(
    screen: &xproto::Screen,
    depth: u8,
    visual_class: xproto::VisualClass,
) -> Option<&xproto::Visualtype> {
    screen
        .allowed_depths
        .iter()
        .filter(|visualdepth| visualdepth.depth == depth)
        .flat_map(|visualdepth| visualdepth.visuals.iter())
        .find(|visualtype| visualtype.class == visual_class)
}

fn grab_key(
    connection: &XCBConnection,
    screen_num: usize,
    keycode: u32,
    modifiers: Modifiers,
) -> Result<(), ReplyError> {
    let screen = &connection.setup().roots[screen_num];
    let modifiers = modifiers.without_locks();
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
            .check()?;
    }
    Ok(())
}

fn grab_keyboard<C: Connection>(connection: &C, screen_num: usize) -> Result<(), ReplyError> {
    let screen = &connection.setup().roots[screen_num];
    connection
        .grab_keyboard(
            true,
            screen.root,
            x11rb::CURRENT_TIME,
            xproto::GrabMode::ASYNC,
            xproto::GrabMode::ASYNC,
        )?
        .discard_reply_and_errors();
    connection.flush()?;
    Ok(())
}

fn ungrab_keyboard<C: Connection>(connection: &C) -> Result<(), ReplyError> {
    connection
        .ungrab_keyboard(x11rb::CURRENT_TIME)?
        .ignore_error();
    connection.flush()?;
    Ok(())
}

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        UTF8_STRING,
        WM_DELETE_WINDOW,
        WM_PROTOCOLS,
        _NET_WM_DESKTOP,
        _NET_WM_NAME,
        _NET_WM_PID,
        _NET_WM_PING,
        _NET_WM_STATE,
        _NET_WM_STATE_ABOVE,
        _NET_WM_STATE_STAYS_ON_TOP,
        _NET_WM_STATE_STICKY,
        _NET_WM_SYNC_REQUEST,
        _NET_WM_WINDOW_TYPE,
        _NET_WM_WINDOW_TYPE_NORMAL,
        _NET_WM_WINDOW_TYPE_UTILITY,
    }
}
