use libdbus_sys as dbus;
use std::collections::hash_map;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::env;
use std::ffi::CStr;
use std::ffi::CString;
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use std::time::Duration;
use x11::keysym;
use x11::xlib;

use crate::atoms::Atoms;
use crate::config::Config;
use crate::error_handler;
use crate::event_loop::{self, ControlFlow, Event, EventLoop, X11Event};
use crate::geometrics::{PhysicalPoint, PhysicalSize, Point, Size};
use crate::render_context::RenderContext;
use crate::styles::Styles;
use crate::text::TextRenderer;
use crate::tray::{Tray, TrayMessage};
use crate::utils;
use crate::widget::{Effect, LayoutResult, Widget};
use crate::xembed::{XEmbedInfo, XEmbedMessage};

#[derive(Debug)]
pub struct App {
    display: *mut xlib::Display,
    atoms: Rc<Atoms>,
    tray: Tray,
    window: xlib::Window,
    window_position: PhysicalPoint,
    window_size: PhysicalSize,
    window_mapped: bool,
    layout: LayoutResult,
    embedded_icons: HashMap<xlib::Window, XEmbedInfo>,
    balloon_messages: HashMap<xlib::Window, BalloonMessage>,
    tray_status: TrayStatus,
    system_tray_atom: xlib::Atom,
    text_renderer: TextRenderer,
    old_error_handler:
        Option<unsafe extern "C" fn(*mut xlib::Display, *mut xlib::XErrorEvent) -> c_int>,
}

impl App {
    pub fn new(config: Config) -> Result<Self, String> {
        let old_error_handler = unsafe {
            xlib::XSetErrorHandler(if config.is_debugging {
                Some(error_handler::print_error)
            } else {
                Some(error_handler::ignore_error)
            })
        };

        let display = unsafe { xlib::XOpenDisplay(ptr::null()) };
        if display.is_null() {
            return Err(format!(
                "No display found at {}",
                env::var("DISPLAY").unwrap_or_default()
            ));
        }

        let atoms = Rc::new(Atoms::new(display));
        let styles = Rc::new(Styles::new(display, &config)?);
        let mut tray = Tray::new(styles);

        let layout = tray.layout(Size {
            width: config.window_width,
            height: 0.0,
        });
        let window_size = layout.size.snap();
        let window_position = unsafe { get_centered_position_on_display(display, window_size) };
        let window =
            unsafe { create_window(display, window_position, window_size, &config, &atoms) };

        let system_tray_atom = unsafe {
            let screen_number = xlib::XDefaultScreen(display);
            utils::new_atom(display, &format!("_NET_SYSTEM_TRAY_S{}\0", screen_number))
        };

        Ok(Self {
            display,
            atoms,
            tray,
            window,
            window_position,
            window_size,
            window_mapped: false,
            layout,
            embedded_icons: HashMap::new(),
            tray_status: TrayStatus::Waiting,
            balloon_messages: HashMap::new(),
            system_tray_atom,
            text_renderer: TextRenderer::new(),
            old_error_handler,
        })
    }

    pub fn run(&mut self) -> Result<(), event_loop::Error> {
        unsafe {
            let previous_selection_owner =
                acquire_tray_selection(self.display, self.window, self.system_tray_atom);
            if previous_selection_owner == 0 {
                broadcast_manager_message(
                    self.display,
                    self.window,
                    self.system_tray_atom,
                    &self.atoms,
                );
                self.tray_status = TrayStatus::Managed;
            } else {
                xlib::XSelectInput(
                    self.display,
                    previous_selection_owner,
                    xlib::StructureNotifyMask,
                );
                self.tray_status = TrayStatus::Pending(previous_selection_owner);
            }
            xlib::XMapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }

        let mut event_loop = EventLoop::new(self.display)?;

        event_loop.run(move |event, context| match event {
            Event::X11Event(event) => {
                let control_flow = match event {
                    X11Event::KeyRelease(event) => self.on_key_release(event),
                    X11Event::Expose(event) => self.on_expose(event),
                    X11Event::ConfigureNotify(event) => self.on_configure_notify(event),
                    X11Event::DestroyNotify(event) => self.on_destroy_notify(event),
                    X11Event::MapNotify(event) => self.on_map_notify(event),
                    X11Event::UnmapNotify(event) => self.on_unmap_notify(event),
                    X11Event::ReparentNotify(event) => self.on_reparent_notify(event),
                    X11Event::ClientMessage(event) => self.on_client_message(event, context),
                    X11Event::PropertyNotify(event) => self.on_property_notify(event),
                    X11Event::SelectionClear(event) => self.on_selection_clear(event),
                    _ => ControlFlow::Continue,
                };
                if event.window() == self.window {
                    let effect = self.tray.on_event(&event, Point::default(), &self.layout);
                    self.apply_effect(effect);
                }
                control_flow
            }
            Event::DBusMessage(message) => {
                use dbus::DBusMessageType::*;

                match (
                    message.message_type(),
                    message.path().unwrap_or_default(),
                    message.member().unwrap_or_default(),
                ) {
                    (MethodCall, "/", "ShowWindow") => {
                        self.show_window();
                        context.send_dbus_message(&message.new_method_return());
                    }
                    (MethodCall, "/", "HideWindow") => {
                        self.hide_window();
                        context.send_dbus_message(&message.new_method_return());
                    }
                    (MethodCall, "/", "ToggleWindow") => {
                        self.toggle_window();
                        context.send_dbus_message(&message.new_method_return());
                    }
                    _ => {}
                }

                ControlFlow::Continue
            }
            Event::Signal(_) => ControlFlow::Break,
        });

        Ok(())
    }

    fn on_key_release(&mut self, event: xlib::XKeyEvent) -> ControlFlow {
        let keysym = unsafe {
            xlib::XkbKeycodeToKeysym(
                self.display,
                event.keycode as c_uchar,
                if event.state & xlib::ShiftMask != 0 {
                    1
                } else {
                    0
                },
                0,
            )
        };
        match keysym as c_uint {
            keysym::XK_Down | keysym::XK_j => {
                self.dispatch_message(TrayMessage::SelectNextItem);
            }
            keysym::XK_Up | keysym::XK_k => {
                self.dispatch_message(TrayMessage::SelectPreviousItem);
            }
            keysym::XK_Right | keysym::XK_l => {
                self.dispatch_message(TrayMessage::ClickSelectedItem {
                    button: xlib::Button1,
                    button_mask: xlib::Button1Mask,
                });
            }
            keysym::XK_Left | keysym::XK_h => {
                self.dispatch_message(TrayMessage::ClickSelectedItem {
                    button: xlib::Button3,
                    button_mask: xlib::Button3Mask,
                });
            }
            keysym::XK_Escape => {
                self.hide_window();
            }
            _ => (),
        }
        ControlFlow::Continue
    }

    fn on_expose(&mut self, event: xlib::XExposeEvent) -> ControlFlow {
        if self.window == event.window && event.count == 0 {
            self.redraw();
        }
        ControlFlow::Continue
    }

    fn on_configure_notify(&mut self, event: xlib::XConfigureEvent) -> ControlFlow {
        if self.window == event.window {
            self.window_position = PhysicalPoint {
                x: event.x as i32,
                y: event.y as i32,
            };
            let window_size = PhysicalSize {
                width: event.width as u32,
                height: event.height as u32,
            };
            if self.window_size != window_size {
                self.window_size = window_size;
                self.recaclulate_layout();
            }
        }
        ControlFlow::Continue
    }

    fn on_destroy_notify(&mut self, event: xlib::XDestroyWindowEvent) -> ControlFlow {
        if event.window == self.window {
            return ControlFlow::Break;
        }
        match self.tray_status {
            TrayStatus::Pending(previous_selection_owner)
                if event.window == previous_selection_owner =>
            unsafe {
                broadcast_manager_message(
                    self.display,
                    self.window,
                    self.system_tray_atom,
                    &self.atoms,
                );
            }
            _ => {
                self.unregister_tray_icon(event.window);
            }
        }
        ControlFlow::Continue
    }

    fn on_map_notify(&mut self, event: xlib::XMapEvent) -> ControlFlow {
        if self.window == event.window {
            self.window_mapped = true;
        }
        ControlFlow::Continue
    }

    fn on_unmap_notify(&mut self, event: xlib::XUnmapEvent) -> ControlFlow {
        if self.window == event.window {
            self.window_mapped = false;
        }
        ControlFlow::Continue
    }

    fn on_reparent_notify(&mut self, event: xlib::XReparentEvent) -> ControlFlow {
        if event.parent != self.window {
            self.unregister_tray_icon(event.window);
        }
        ControlFlow::Continue
    }

    fn on_property_notify(&mut self, event: xlib::XPropertyEvent) -> ControlFlow {
        if event.atom == xlib::XA_WM_NAME || event.atom == self.atoms.NET_WM_NAME {
            if self.embedded_icons.contains_key(&event.window) {
                let title = unsafe {
                    utils::get_window_title(self.display, event.window, self.atoms.NET_WM_NAME)
                        .unwrap_or_default()
                };
                self.dispatch_message(TrayMessage::ChangeTitle {
                    window: event.window,
                    title,
                });
            }
        } else if event.atom == self.atoms.XEMBED_INFO {
            if let Some(xembed_info) =
                unsafe { get_xembed_info(self.display, event.window, &self.atoms) }
            {
                self.register_tray_icon(event.window, xembed_info);
            }
        }
        ControlFlow::Continue
    }

    fn on_selection_clear(&mut self, event: xlib::XSelectionClearEvent) -> ControlFlow {
        if event.selection == self.system_tray_atom {
            if event.window == self.window {
                self.tray_status = TrayStatus::Waiting;
                return ControlFlow::Break;
            }
        }
        ControlFlow::Continue
    }

    fn on_client_message(
        &mut self,
        event: xlib::XClientMessageEvent,
        context: &mut EventLoop,
    ) -> ControlFlow {
        if event.message_type == self.atoms.WM_PROTOCOLS && event.format == 32 {
            let protocol = event.data.get_long(0) as xlib::Atom;
            if protocol == self.atoms.NET_WM_PING {
                unsafe {
                    let root = xlib::XDefaultRootWindow(self.display);
                    let mut reply_event = event;
                    reply_event.window = root;
                    xlib::XSendEvent(
                        self.display,
                        root,
                        xlib::False,
                        xlib::SubstructureNotifyMask | xlib::SubstructureRedirectMask,
                        &mut reply_event.into(),
                    );
                }
            } else if protocol == self.atoms.NET_WM_SYNC_REQUEST {
                self.request_redraw();
            } else if protocol == self.atoms.WM_DELETE_WINDOW {
                self.hide_window();
            }
        } else if event.message_type == self.atoms.NET_SYSTEM_TRAY_OPCODE {
            let opcode = event.data.get_long(1);
            if opcode == SystemTrayOpcode::RequestDock as _ {
                let icon_window = event.data.get_long(2) as xlib::Window;
                if let Some(xembed_info) =
                    unsafe { get_xembed_info(self.display, icon_window, &self.atoms) }
                {
                    self.register_tray_icon(icon_window, xembed_info);
                }
            } else if opcode == SystemTrayOpcode::BeginMessage as _ {
                let balloon_message = BalloonMessage::new(&event.data);
                self.balloon_messages.insert(event.window, balloon_message);
            } else if opcode == SystemTrayOpcode::CancelMessage as _ {
                if let hash_map::Entry::Occupied(entry) = self.balloon_messages.entry(event.window)
                {
                    let id = event.data.get_long(2);
                    if entry.get().id == id {
                        entry.remove();
                    }
                }
            }
        } else if event.message_type == self.atoms.NET_SYSTEM_TRAY_MESSAGE_DATA {
            if let hash_map::Entry::Occupied(mut entry) = self.balloon_messages.entry(event.window)
            {
                entry.get_mut().write_message(event.data.as_ref());
                if entry.get().remaining_len() == 0 {
                    let balloon_message = entry.remove();
                    let summary = unsafe {
                        utils::get_window_title(self.display, event.window, self.atoms.NET_WM_NAME)
                    }
                    .and_then(|title| CString::new(title).ok())
                    .unwrap_or_default();
                    context.send_notification(
                        summary.as_c_str(),
                        balloon_message.as_c_str(),
                        balloon_message.id as u32,
                        Some(balloon_message.timeout),
                    );
                }
            }
        }
        ControlFlow::Continue
    }

    fn show_window(&mut self) {
        unsafe {
            let window_position = get_centered_position_on_display(self.display, self.window_size);
            xlib::XMoveWindow(
                self.display,
                self.window,
                window_position.x,
                window_position.y,
            );
            xlib::XMapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }

    fn hide_window(&mut self) {
        unsafe {
            self.dispatch_message(TrayMessage::DeselectItem);
            xlib::XUnmapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }

    fn toggle_window(&mut self) {
        if self.window_mapped {
            self.hide_window();
        } else {
            self.show_window();
        }
    }

    fn register_tray_icon(&mut self, icon_window: xlib::Window, xembed_info: XEmbedInfo) {
        let old_is_mapped = self
            .embedded_icons
            .insert(icon_window, xembed_info)
            .map(|xembed_info| xembed_info.is_mapped());
        match (old_is_mapped, xembed_info.is_mapped()) {
            (None, false) => unsafe {
                send_xembed_message(
                    self.display,
                    &self.atoms,
                    icon_window,
                    self.window,
                    XEmbedMessage::EmbeddedNotify,
                    xembed_info.version,
                );
                wait_for_embedding(self.display, icon_window);
            },
            (None, true) => {
                let title = unsafe {
                    utils::get_window_title(self.display, icon_window, self.atoms.NET_WM_NAME)
                        .unwrap_or_default()
                };
                self.dispatch_message(TrayMessage::AddTrayIcon {
                    window: icon_window,
                    title,
                });
                unsafe {
                    send_xembed_message(
                        self.display,
                        &self.atoms,
                        icon_window,
                        self.window,
                        XEmbedMessage::EmbeddedNotify,
                        xembed_info.version,
                    );
                    begin_embedding(self.display, icon_window, self.window);
                }
            }
            (Some(false), true) => {
                let title = unsafe {
                    utils::get_window_title(self.display, icon_window, self.atoms.NET_WM_NAME)
                        .unwrap_or_default()
                };
                self.dispatch_message(TrayMessage::AddTrayIcon {
                    window: icon_window,
                    title,
                });
                unsafe {
                    begin_embedding(self.display, icon_window, self.window);
                }
            }
            (Some(true), false) => {
                self.dispatch_message(TrayMessage::RemoveTrayIcon {
                    window: icon_window,
                });

                unsafe {
                    release_embedding(self.display, icon_window);
                }
            }
            _ => {}
        }
    }

    fn unregister_tray_icon(&mut self, icon_window: xlib::Window) {
        self.balloon_messages.remove(&icon_window);

        self.dispatch_message(TrayMessage::RemoveTrayIcon {
            window: icon_window,
        });

        if let Some(xembed_info) = self.embedded_icons.remove(&icon_window) {
            if xembed_info.is_mapped() {
                unsafe {
                    release_embedding(self.display, icon_window);
                }
            }
        }
    }

    fn dispatch_message(&mut self, message: TrayMessage) {
        let effect = self.tray.on_message(message);
        self.apply_effect(effect);
    }

    fn apply_effect(&mut self, effect: Effect) {
        let mut pending_effects = VecDeque::new();
        let mut current = effect;

        let mut redraw_requested = false;
        let mut layout_requested = false;

        loop {
            match current {
                Effect::None => {}
                Effect::Batch(effects) => {
                    pending_effects.extend(effects);
                }
                Effect::Action(action) => {
                    action(self.display, self.window);
                }
                Effect::RequestRedraw => {
                    redraw_requested = true;
                }
                Effect::RequestLayout => {
                    layout_requested = true;
                }
            }
            if let Some(next) = pending_effects.pop_front() {
                current = next;
            } else {
                break;
            }
        }

        if layout_requested {
            self.recaclulate_layout();
        } else if redraw_requested {
            self.request_redraw();
        }
    }

    fn recaclulate_layout(&mut self) {
        self.layout = self.tray.layout(self.window_size.unsnap());

        let size = self.layout.size.snap();

        unsafe {
            if self.window_size != size {
                let mut size_hints = mem::MaybeUninit::<xlib::XSizeHints>::zeroed().assume_init();
                size_hints.flags = xlib::PMinSize | xlib::PMaxSize;
                size_hints.min_height = size.height as c_int;
                size_hints.max_height = size.height as c_int;

                xlib::XSetWMSizeHints(
                    self.display,
                    self.window,
                    &mut size_hints,
                    xlib::XA_WM_NORMAL_HINTS,
                );

                let x = self.window_position.x;
                let y = self.window_position.y
                    - (((size.height as i32 - self.window_size.height as i32) / 2) as i32);

                xlib::XMoveResizeWindow(self.display, self.window, x, y, size.width, size.height);
            } else {
                self.request_redraw();
            }

            xlib::XFlush(self.display);
        }
    }

    fn request_redraw(&self) {
        unsafe {
            xlib::XClearArea(self.display, self.window, 0, 0, 0, 0, xlib::True);
            xlib::XFlush(self.display);
        }
    }

    fn redraw(&mut self) {
        let mut context = RenderContext::new(
            self.display,
            self.window,
            self.window_size,
            &mut self.text_renderer,
        );

        self.tray
            .render(Point::default(), &self.layout, &mut context);

        context.commit();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        for (window, xembed_info) in &self.embedded_icons {
            if xembed_info.is_mapped() {
                unsafe {
                    release_embedding(self.display, *window);
                }
            }
        }

        self.text_renderer.clear_caches(self.display);

        if matches!(self.tray_status, TrayStatus::Managed) {
            unsafe {
                release_tray_selection(self.display, self.system_tray_atom);
            }
        }

        unsafe {
            xlib::XDestroyWindow(self.display, self.window);
            xlib::XSync(self.display, xlib::True);
            xlib::XCloseDisplay(self.display);
            xlib::XSetErrorHandler(self.old_error_handler);
        }
    }
}

#[repr(i64)]
#[derive(Debug)]
enum SystemTrayOpcode {
    RequestDock = 0,
    BeginMessage = 1,
    CancelMessage = 2,
}

#[derive(Debug)]
enum TrayStatus {
    Waiting,
    Pending(xlib::Window),
    Managed,
}

#[derive(Debug)]
struct BalloonMessage {
    buffer: Vec<u8>,
    timeout: Duration,
    length: usize,
    id: i64,
}

impl BalloonMessage {
    fn new(data: &xlib::ClientMessageData) -> Self {
        let timeout = data.get_long(2) as u64;
        let length = data.get_long(3) as usize;
        let id = data.get_long(4) as i64;
        Self {
            buffer: Vec::with_capacity(length + 1),
            timeout: Duration::from_millis(timeout),
            length,
            id,
        }
    }

    fn remaining_len(&self) -> usize {
        self.length.saturating_sub(self.buffer.len())
    }

    fn write_message(&mut self, bytes: &[u8]) {
        let incoming_len = self.remaining_len().min(20);
        if incoming_len > 0 {
            self.buffer.extend_from_slice(&bytes[..incoming_len]);
            if self.remaining_len() == 0 {
                assert_eq!(self.buffer.capacity().saturating_sub(self.buffer.len()), 1);
                self.buffer.push(0); // Add NULL to last
            }
        }
    }

    fn as_c_str(&self) -> &CStr {
        CStr::from_bytes_with_nul(self.buffer.as_slice())
            .ok()
            .unwrap_or_default()
    }
}

#[allow(dead_code)]
#[repr(i32)]
enum SystemTrayOrientation {
    Horzontal = 0,
    Vertical = 1,
}

unsafe fn create_window(
    display: *mut xlib::Display,
    position: PhysicalPoint,
    size: PhysicalSize,
    config: &Config,
    atoms: &Atoms,
) -> xlib::Window {
    let window = {
        let screen = xlib::XDefaultScreenOfDisplay(display);
        let root = xlib::XRootWindowOfScreen(screen);

        let mut attributes: xlib::XSetWindowAttributes = mem::MaybeUninit::uninit().assume_init();
        attributes.backing_store = xlib::WhenMapped;
        attributes.bit_gravity = xlib::CenterGravity;
        attributes.event_mask = xlib::KeyPressMask
            | xlib::ButtonPressMask
            | xlib::ButtonReleaseMask
            | xlib::EnterWindowMask
            | xlib::ExposureMask
            | xlib::FocusChangeMask
            | xlib::LeaveWindowMask
            | xlib::KeyReleaseMask
            | xlib::PropertyChangeMask
            | xlib::StructureNotifyMask;

        xlib::XCreateWindow(
            display,
            root,
            position.x as i32,
            position.y as i32,
            size.width,
            size.height,
            0,
            xlib::CopyFromParent,
            xlib::InputOutput as u32,
            xlib::CopyFromParent as *mut xlib::Visual,
            xlib::CWBackingStore | xlib::CWBitGravity | xlib::CWEventMask,
            &mut attributes,
        )
    };

    {
        let mut protocol_atoms = [
            atoms.NET_WM_PING,
            atoms.NET_WM_SYNC_REQUEST,
            atoms.WM_DELETE_WINDOW,
        ];
        xlib::XSetWMProtocols(
            display,
            window,
            protocol_atoms.as_mut_ptr(),
            protocol_atoms.len() as i32,
        );
    }

    {
        let name_string = format!("{}\0", config.program_name);
        let class_string = format!("{}\0{}\0", config.program_name, config.program_name);

        let mut class_hint = mem::MaybeUninit::<xlib::XClassHint>::uninit().assume_init();
        class_hint.res_name = name_string.as_ptr() as *mut c_char;
        class_hint.res_class = class_string.as_ptr() as *mut c_char;

        xlib::XSetClassHint(display, window, &mut class_hint);
    }

    utils::set_window_property(
        display,
        window,
        atoms.NET_WM_WINDOW_TYPE,
        xlib::XA_ATOM,
        &[atoms.NET_WM_WINDOW_TYPE_DIALOG],
    );

    utils::set_window_property(
        display,
        window,
        atoms.NET_WM_STATE,
        xlib::XA_ATOM,
        &[atoms.NET_WM_STATE_STICKY],
    );

    utils::set_window_property(
        display,
        window,
        atoms.NET_SYSTEM_TRAY_ORIENTATION,
        xlib::XA_CARDINAL,
        &[SystemTrayOrientation::Vertical],
    );

    {
        let screen = xlib::XDefaultScreenOfDisplay(display);
        let visual = xlib::XDefaultVisualOfScreen(screen);
        let visual_id = xlib::XVisualIDFromVisual(visual);
        utils::set_window_property(
            display,
            window,
            atoms.NET_SYSTEM_TRAY_VISUAL,
            xlib::XA_VISUALID,
            &[visual_id],
        );
    }

    window
}

unsafe fn acquire_tray_selection(
    display: *mut xlib::Display,
    window: xlib::Window,
    system_tray_atom: xlib::Atom,
) -> xlib::Window {
    let previous_selection_owner = xlib::XGetSelectionOwner(display, system_tray_atom);
    xlib::XSetSelectionOwner(display, system_tray_atom, window, xlib::CurrentTime);
    previous_selection_owner
}

unsafe fn broadcast_manager_message(
    display: *mut xlib::Display,
    window: xlib::Window,
    system_tray_atom: xlib::Atom,
    atoms: &Atoms,
) {
    let root = xlib::XDefaultRootWindow(display);

    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, xlib::CurrentTime as c_long);
    data.set_long(1, system_tray_atom as c_long);
    data.set_long(2, window as c_long);

    utils::send_client_message(display, root, root, atoms.MANAGER, data);
    xlib::XFlush(display);
}

unsafe fn release_tray_selection(display: *mut xlib::Display, system_tray_atom: xlib::Atom) {
    xlib::XSetSelectionOwner(display, system_tray_atom, 0, xlib::CurrentTime);
}

unsafe fn wait_for_embedding(display: *mut xlib::Display, icon_window: xlib::Window) {
    xlib::XSelectInput(display, icon_window, xlib::PropertyChangeMask);
    xlib::XFlush(display);
}

unsafe fn begin_embedding(
    display: *mut xlib::Display,
    icon_window: xlib::Window,
    embedder_window: xlib::Window,
) {
    xlib::XSelectInput(
        display,
        icon_window,
        xlib::PropertyChangeMask | xlib::StructureNotifyMask,
    );
    xlib::XReparentWindow(display, icon_window, embedder_window, 0, 0);
    xlib::XFlush(display);
}

unsafe fn release_embedding(display: *mut xlib::Display, icon_window: xlib::Window) {
    let screen = xlib::XDefaultScreenOfDisplay(display);
    let root = xlib::XRootWindowOfScreen(screen);

    xlib::XSelectInput(display, icon_window, xlib::NoEventMask);
    xlib::XReparentWindow(display, icon_window, root, 0, 0);
    xlib::XUnmapWindow(display, icon_window);
    xlib::XFlush(display);
}

unsafe fn send_xembed_message(
    display: *mut xlib::Display,
    atoms: &Atoms,
    window: xlib::Window,
    embedder_window: xlib::Window,
    xembed_message: XEmbedMessage,
    xembed_version: u64,
) {
    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, xlib::CurrentTime as c_long);
    data.set_long(1, xembed_message as c_long);
    data.set_long(2, embedder_window as c_long);
    data.set_long(3, xembed_version as c_long);

    utils::send_client_message(display, window, window, atoms.XEMBED, data);
}

unsafe fn get_xembed_info(
    display: *mut xlib::Display,
    window: xlib::Window,
    atoms: &Atoms,
) -> Option<XEmbedInfo> {
    utils::get_window_fixed_property::<c_ulong, 2>(display, window, atoms.XEMBED_INFO).map(|prop| {
        XEmbedInfo {
            version: (*prop)[0],
            flags: (*prop)[1],
        }
    })
}

unsafe fn get_centered_position_on_display(
    display: *mut xlib::Display,
    window_size: PhysicalSize,
) -> PhysicalPoint {
    let screen_number = xlib::XDefaultScreen(display);
    let display_width = xlib::XDisplayWidth(display, screen_number);
    let display_height = xlib::XDisplayHeight(display, screen_number);
    PhysicalPoint {
        x: (display_width as f32 / 2.0 - window_size.width as f32 / 2.0) as i32,
        y: (display_height as f32 / 2.0 - window_size.height as f32 / 2.0) as i32,
    }
}
