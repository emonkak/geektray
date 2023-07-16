use anyhow::{anyhow, Context as _};
use nix::sys::epoll;
use nix::sys::signal::{SigSet, Signal};
use nix::sys::signalfd::{siginfo, SignalFd};
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;
use x11rb::connection::Connection;
use x11rb::protocol;
use x11rb::protocol::xkb;
use x11rb::protocol::xkb::ConnectionExt as _;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

use crate::atoms::Atoms;
use crate::config::{Action, Config, KeyBinding};
use crate::event::{KeyState, Keysym, Modifiers};
use crate::geometrics::Size;
use crate::render_context::RenderContext;
use crate::tray_manager::{TrayEvent, TrayManager};
use crate::tray_window::TrayWindow;
use crate::xkbcommon;

type ActionTable = HashMap<(Keysym, Modifiers), usize>;

const EVENT_KIND_X11: u64 = 1;
const EVENT_KIND_SIGNAL: u64 = 2;

pub struct App {
    config: Config,
    connection: Rc<XCBConnection>,
    screen_num: usize,
    atoms: Rc<Atoms>,
    xkb_state: xkbcommon::State,
    signal_fd: SignalFd,
    tray_window: TrayWindow<XCBConnection>,
    tray_manager: TrayManager<XCBConnection>,
    action_table: ActionTable,
    render_context: Option<RenderContext>,
}

impl App {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let (connection, screen_num) =
            XCBConnection::connect(None).context("connect to X server")?;
        let connection = Rc::new(connection);

        setup_xkb_extension(&*connection)?;

        let atoms: Rc<_> = Atoms::new(&*connection)?
            .reply()
            .context("intern app atoms")?
            .into();

        let xkb_state = create_xkb_state(&connection)?;

        let signal_fd = create_signal_fd()?;

        let action_table = build_action_table(&config.key_bindings);

        for key_binding in config
            .key_bindings
            .iter()
            .filter(|key_binding| key_binding.global())
        {
            let keycode = xkb_state
                .lookup_keycode(key_binding.keysym())
                .context("lookup keycode")?;
            grab_key(&*connection, screen_num, keycode, key_binding.modifiers())?;
        }

        let window_size = Size {
            width: config.window.default_width,
            height: config.ui.icon_size.max(config.ui.text_size) + config.ui.item_padding * 2.0,
        }
        .snap();
        let tray_window = TrayWindow::new(
            connection.clone(),
            screen_num,
            &*atoms,
            &config.window,
            window_size,
        )?;

        let tray_manager = TrayManager::new(connection.clone(), screen_num, atoms.clone())?;

        Ok(Self {
            config,
            connection,
            screen_num,
            atoms,
            xkb_state,
            signal_fd,
            tray_window,
            tray_manager,
            action_table,
            render_context: None,
        })
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        self.tray_manager
            .acquire_tray_selection(self.tray_window.window())?;

        self.run_event_loop()?;

        Ok(())
    }

    fn run_event_loop(&mut self) -> anyhow::Result<()> {
        let epoll_fd = epoll::epoll_create()?;

        add_interest_entry(epoll_fd, &*self.connection, EVENT_KIND_X11)?;
        add_interest_entry(epoll_fd, &self.signal_fd, EVENT_KIND_SIGNAL)?;

        let mut epoll_events = vec![epoll::EpollEvent::empty(); 2];

        'outer: loop {
            let available_fds = epoll::epoll_wait(epoll_fd, &mut epoll_events, -1).unwrap_or(0);
            let mut control_flow = ControlFlow::Continue(());

            for epoll_event in &epoll_events[0..available_fds] {
                if epoll_event.data() == EVENT_KIND_X11 {
                    while let Some(event) = self.connection.poll_for_event()? {
                        self.handle_x11_event(&event, &mut control_flow)?;

                        if matches!(control_flow, ControlFlow::Break(())) {
                            break 'outer;
                        }
                    }
                } else if epoll_event.data() == EVENT_KIND_SIGNAL {
                    if let Some(signal) = self.signal_fd.read_signal()? {
                        self.handle_signal(signal, &mut control_flow)?;

                        if matches!(control_flow, ControlFlow::Break(())) {
                            break 'outer;
                        }
                    }
                } else {
                    unreachable!();
                }

                self.handle_tick()?;
            }
        }

        Ok(())
    }

    fn handle_x11_event(
        &mut self,
        event: &protocol::Event,
        control_flow: &mut ControlFlow<()>,
    ) -> anyhow::Result<()> {
        use protocol::Event::*;

        match event {
            FocusOut(event) => {
                if self.config.window.auto_hide
                    && event.mode == xproto::NotifyMode::NORMAL
                    && event.detail == xproto::NotifyDetail::NONLINEAR
                    && event.event == self.tray_window.window()
                {
                    self.tray_window.hide()?;
                }
            }
            KeyPress(event) => {
                self.xkb_state
                    .update_key(event.detail as u32, KeyState::Down);
            }
            KeyRelease(event) => {
                self.xkb_state.update_key(event.detail as u32, KeyState::Up);
                let keysym = self.xkb_state.get_keysym(event.detail as u32);
                let modifiers = self.xkb_state.get_modifiers();
                if let Some(index) = self.action_table.get(&(keysym, modifiers)) {
                    self.handle_key_binding(*index)?;
                }
            }
            LeaveNotify(event) => {
                if self.config.window.auto_hide
                    && event.mode == xproto::NotifyMode::NORMAL
                    && event.detail == xproto::NotifyDetail::ANCESTOR
                    && event.event == self.tray_window.window()
                {
                    self.tray_window.hide()?;
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
                    self.tray_window.request_redraw();
                } else if protocol == self.atoms.WM_DELETE_WINDOW {
                    self.tray_window.hide()?;
                }
            }
            XkbStateNotify(event) => self.xkb_state.update_mask(&event),
            _ => {}
        }

        self.tray_window.handle_x11_event(&event, control_flow)?;

        if let Some(tray_event) = self.tray_manager.translate_event(&event)? {
            self.handle_tray_event(tray_event);
        }

        Ok(())
    }

    fn handle_signal(
        &mut self,
        signal: siginfo,
        control_flow: &mut ControlFlow<()>,
    ) -> anyhow::Result<()> {
        log::info!("signal {:?} received", signal.ssi_signo);

        *control_flow = ControlFlow::Break(());

        Ok(())
    }

    fn handle_key_binding(&mut self, index: usize) -> anyhow::Result<()> {
        let key_binding = &self.config.key_bindings[index];
        for action in key_binding.actions() {
            match action {
                Action::HideWindow => {
                    self.tray_window.hide()?;
                }
                Action::ShowWindow => {
                    self.tray_window.show()?;
                }
                Action::ToggleWindow => {
                    if self.tray_window.is_mapped() {
                        self.tray_window.hide()?;
                    } else {
                        self.tray_window.show()?;
                    }
                }
                Action::DeselectItem => {
                    self.tray_window.deselect_item();
                }
                Action::SelectItem { index } => {
                    self.tray_window.select_item(*index);
                }
                Action::SelectNextItem => {
                    self.tray_window.select_next_item();
                }
                Action::SelectPreviousItem => {
                    self.tray_window.select_previous_item();
                }
                Action::ClickSelectedItem { button } => {
                    self.tray_window.click_selected_item(*button)?;
                }
            }
        }
        Ok(())
    }

    fn handle_tick(&mut self) -> anyhow::Result<()> {
        if self.tray_window.is_mapped() {
            let should_layout = self.tray_window.should_layout() || self.render_context.is_none();

            if should_layout {
                let new_size = self.tray_window.layout(&self.config.ui)?;
                self.render_context = Some(RenderContext::new(
                    self.connection.clone(),
                    self.screen_num,
                    self.tray_window.window(),
                    new_size,
                )?);
            }

            if should_layout || self.tray_window.should_redraw() {
                let render_context = self.render_context.as_ref().unwrap();
                self.tray_window
                    .draw(should_layout, &self.config.ui, &render_context)?;
            }
        }

        Ok(())
    }

    fn handle_tray_event(&mut self, event: TrayEvent) {
        match event {
            TrayEvent::IconAdded(icon, title, is_embdded) => {
                self.tray_window.add_icon(icon, title, is_embdded);
            }
            TrayEvent::IconRemoved(icon) => {
                self.tray_window.remove_icon(icon);
            }
            TrayEvent::VisibilityChanged(icon, is_embdded) => {
                self.tray_window.change_visibility(icon, is_embdded);
            }
            TrayEvent::TitleChanged(icon, title) => {
                self.tray_window.change_title(icon, title);
            }
            TrayEvent::MessageReceived(_message) => {}
            TrayEvent::SelectionCleared => {
                self.tray_window.clear_icons();
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.tray_manager.release_tray_selection().ok();

        for key_binding in self
            .config
            .key_bindings
            .iter()
            .filter(|key_binding| key_binding.global())
        {
            if let Some(keycode) = self
                .xkb_state
                .lookup_keycode(key_binding.keysym())
                .context("lookup keycode")
                .ok()
            {
                ungrab_key(
                    &*self.connection,
                    self.screen_num,
                    keycode,
                    key_binding.modifiers(),
                )
                .ok();
            }
        }
    }
}

fn add_interest_entry(epoll_fd: RawFd, resource: &impl AsRawFd, kind: u64) -> anyhow::Result<()> {
    let raw_fd = resource.as_raw_fd();
    let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, kind);
    epoll::epoll_ctl(
        epoll_fd,
        epoll::EpollOp::EpollCtlAdd,
        raw_fd,
        Some(&mut event),
    )
    .context("add an interest entry to epoll fd")?;
    Ok(())
}

fn build_action_table(key_bindings: &[KeyBinding]) -> ActionTable {
    let mut action_table = HashMap::new();
    for (i, key_binding) in key_bindings.iter().enumerate() {
        action_table.insert(
            (
                key_binding.keysym(),
                key_binding.modifiers().without_locks(),
            ),
            i,
        );
    }
    action_table
}

fn create_signal_fd() -> anyhow::Result<SignalFd> {
    let mut sigset = SigSet::empty();
    sigset.add(Signal::SIGINT);
    sigset.thread_block().context("add set of signals")?;
    Ok(SignalFd::new(&sigset).context("create signal fd")?)
}

fn create_xkb_state(connection: &XCBConnection) -> anyhow::Result<xkbcommon::State> {
    let context = xkbcommon::Context::new();
    let device_id =
        xkbcommon::DeviceId::core_keyboard(connection).context("get the core keyboard")?;
    let keymap = xkbcommon::Keymap::from_device(context, &connection, device_id)
        .context("create a keymap from the device")?;
    Ok(xkbcommon::State::from_keymap(keymap))
}

fn grab_key(
    connection: &impl Connection,
    screen_num: usize,
    keycode: u32,
    modifiers: Modifiers,
) -> anyhow::Result<()> {
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
                u16::from(modifiers).into(),
                keycode as u8,
                xproto::GrabMode::ASYNC,
                xproto::GrabMode::ASYNC,
            )?
            .check()
            .context("grab key")?;
    }
    Ok(())
}

fn setup_xkb_extension(connection: &impl Connection) -> anyhow::Result<()> {
    let reply = connection
        .xkb_use_extension(1, 0)?
        .reply()
        .context("init xkb extension")?;
    if !reply.supported {
        return Err(anyhow!("xkb extension not supported."));
    }

    {
        let values = xkb::SelectEventsAux::new();
        connection
            .xkb_select_events(
                xkb::ID::USE_CORE_KBD.into(), // device_spec
                xkb::EventType::from(0u16),   // clear
                xkb::EventType::STATE_NOTIFY, // select_all
                xkb::MapPart::from(0u16),     // affect_map
                xkb::MapPart::from(0u16),     // map
                &values,                      // details
            )?
            .check()
            .context("select xkb events")?;
    }

    Ok(())
}

fn ungrab_key(
    connection: &impl Connection,
    screen_num: usize,
    keycode: u32,
    modifiers: Modifiers,
) -> anyhow::Result<()> {
    let screen = &connection.setup().roots[screen_num];
    let modifiers = modifiers.without_locks();
    for modifiers in [
        modifiers,
        modifiers | Modifiers::CAPS_LOCK,
        modifiers | Modifiers::NUM_LOCK,
        modifiers | Modifiers::CAPS_LOCK | Modifiers::NUM_LOCK,
    ] {
        connection
            .ungrab_key(keycode as u8, screen.root, u16::from(modifiers).into())?
            .check()
            .context("ungrab key")?;
    }
    Ok(())
}
