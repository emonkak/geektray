use libdbus_sys as dbus;
use std::collections::hash_map;
use std::collections::HashMap;
use std::env;
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use std::str;
use std::time::Duration;
use x11::keysym;
use x11::xlib;

use crate::atoms::Atoms;
use crate::config::Config;
use crate::error_handler;
use crate::event_loop::{self, ControlFlow, Event, EventLoop, EventLoopContext, X11Event};
use crate::geometrics::{PhysicalPoint, PhysicalSize, Size};
use crate::render_context::RenderContext;
use crate::styles::Styles;
use crate::text::TextRenderer;
use crate::tray::Tray;
use crate::tray_item::TrayItem;
use crate::utils;
use crate::widget::{SideEffect, WidgetPod};
use crate::xembed::{XEmbedInfo, XEmbedMessage};

const SYSTEM_TRAY_REQUEST_DOCK: i64 = 0;
const SYSTEM_TRAY_BEGIN_MESSAGE: i64 = 1;
const SYSTEM_TRAY_CANCEL_MESSAGE: i64 = 2;

#[derive(Debug)]
pub struct App {
    display: *mut xlib::Display,
    atoms: Rc<Atoms>,
    styles: Rc<Styles>,
    tray: WidgetPod<Tray>,
    window: xlib::Window,
    window_position: PhysicalPoint,
    window_size: PhysicalSize,
    window_mapped: bool,
    text_renderer: TextRenderer,
    previous_selection_owner: Option<xlib::Window>,
    old_error_handler:
        Option<unsafe extern "C" fn(*mut xlib::Display, *mut xlib::XErrorEvent) -> c_int>,
    pending_tray_messages: HashMap<xlib::Window, TrayMessage>,
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
        let mut tray = WidgetPod::new(Tray::new(styles.clone()));

        let window_size = tray
            .layout(Size {
                width: config.window_width,
                height: 0.0,
            })
            .snap();
        let window_position = unsafe { get_centered_position_on_display(display, window_size) };
        let window = unsafe { create_window(display, window_position, window_size) };

        Ok(Self {
            display,
            atoms,
            styles,
            tray,
            window,
            window_position,
            window_size,
            window_mapped: false,
            text_renderer: TextRenderer::new(),
            previous_selection_owner: None,
            old_error_handler,
            pending_tray_messages: HashMap::new(),
        })
    }

    pub fn run(&mut self) -> Result<(), event_loop::Error> {
        unsafe {
            self.previous_selection_owner = Some(acquire_tray_selection(self.display, self.window));

            let mut protocol_atoms = [
                self.atoms.NET_WM_PING,
                self.atoms.NET_WM_SYNC_REQUEST,
                self.atoms.WM_DELETE_WINDOW,
            ];

            xlib::XSetWMProtocols(
                self.display,
                self.window,
                protocol_atoms.as_mut_ptr(),
                protocol_atoms.len() as i32,
            );

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
                    _ => ControlFlow::Continue,
                };
                if event.window() == self.window {
                    match self.tray.on_event(self.display, self.window, &event) {
                        SideEffect::None => {}
                        SideEffect::RequestLayout => self.recaclulate_layout(),
                        SideEffect::RequestRedraw => self.request_redraw(),
                    }
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
                self.tray.widget.select_next();
                self.request_redraw();
            }
            keysym::XK_Up | keysym::XK_k => {
                self.tray.widget.select_previous();
                self.request_redraw();
            }
            keysym::XK_Right | keysym::XK_l => {
                self.tray
                    .widget
                    .click_selected_icon(self.display, xlib::Button1, xlib::Button1Mask)
            }
            keysym::XK_Left | keysym::XK_h => {
                self.tray
                    .widget
                    .click_selected_icon(self.display, xlib::Button3, xlib::Button3Mask)
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
        if self.window == event.window {
            return ControlFlow::Break;
        }
        if let Some(_) = self.unregister_tray_item(event.window) {
            self.recaclulate_layout();
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
            self.unregister_tray_item(event.window);
        }
        ControlFlow::Continue
    }

    fn on_property_notify(&mut self, event: xlib::XPropertyEvent) -> ControlFlow {
        if event.atom == self.atoms.NET_WM_NAME {
            if let Some(tray_item) = self.tray.widget.find_tray_item_mut(event.window) {
                let icon_title = unsafe {
                    utils::get_window_title(self.display, event.window).unwrap_or_default()
                };
                tray_item.widget.change_icon_title(icon_title);
            }
        } else if event.atom == self.atoms.XEMBED_INFO {
            let xembed_info = unsafe { get_xembed_info(self.display, event.window, &self.atoms) };
            match xembed_info {
                Some(xembed_info) if xembed_info.is_mapped() => {
                    if let Some(tray_item) = self.tray.widget.find_tray_item_mut(event.window) {
                        tray_item.widget.set_embedded(true);
                        unsafe {
                            begin_embedding(self.display, event.window, self.window);
                        }
                        self.request_redraw();
                    }
                }
                _ => {
                    if let Some(tray_item) = self.unregister_tray_item(event.window) {
                        if tray_item.widget.is_embedded() {
                            unsafe {
                                release_embedding(self.display, event.window);
                            }
                        }
                        self.recaclulate_layout();
                    }
                }
            }
        }
        ControlFlow::Continue
    }

    fn on_client_message(
        &mut self,
        event: xlib::XClientMessageEvent,
        context: &mut EventLoopContext,
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
            let icon_window = event.data.get_long(2) as xlib::Window;
            if opcode == SYSTEM_TRAY_REQUEST_DOCK {
                if let Some(xembed_info) =
                    unsafe { get_xembed_info(self.display, icon_window, &self.atoms) }
                {
                    self.register_tray_item(icon_window, &xembed_info);
                    self.recaclulate_layout();
                }
            } else if opcode == SYSTEM_TRAY_BEGIN_MESSAGE {
                let tray_message = TrayMessage::new(&event.data);
                self.pending_tray_messages
                    .insert(event.window, tray_message);
            } else if opcode == SYSTEM_TRAY_CANCEL_MESSAGE {
                if let hash_map::Entry::Occupied(entry) =
                    self.pending_tray_messages.entry(event.window)
                {
                    let id = event.data.get_long(2);
                    if entry.get().id == id {
                        entry.remove();
                    }
                }
            }
        } else if event.message_type == self.atoms.NET_SYSTEM_TRAY_MESSAGE_DATA {
            if let hash_map::Entry::Occupied(mut entry) =
                self.pending_tray_messages.entry(event.window)
            {
                let remaining_len = entry.get_mut().receive_message(event.data.as_ref());
                if remaining_len == 0 {
                    let tray_message = entry.remove();
                    let summary = unsafe {
                        utils::get_window_title(self.display, event.window).unwrap_or_default()
                    };
                    context.send_notification(
                        &summary,
                        tray_message.as_string(),
                        tray_message.id as u32,
                        tray_message.timeout,
                    );
                }
            }
        }
        ControlFlow::Continue
    }

    fn show_window(&self) {
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

    fn hide_window(&self) {
        unsafe {
            xlib::XUnmapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }

    fn toggle_window(&self) {
        if self.window_mapped {
            self.hide_window();
        } else {
            self.show_window();
        }
    }

    fn register_tray_item(&mut self, icon_window: xlib::Window, xembed_info: &XEmbedInfo) {
        let icon_title =
            unsafe { utils::get_window_title(self.display, icon_window).unwrap_or_default() };
        let tray_item = WidgetPod::new(TrayItem::new(
            icon_window,
            icon_title,
            xembed_info.is_mapped(),
            self.styles.clone(),
        ));
        self.tray.widget.add_tray_item(tray_item);
        unsafe {
            request_embedding(
                self.display,
                icon_window,
                self.window,
                xembed_info,
                &self.atoms,
            );
        }
    }

    fn unregister_tray_item(&mut self, icon_window: xlib::Window) -> Option<WidgetPod<TrayItem>> {
        self.pending_tray_messages.remove(&icon_window);
        self.tray.widget.remove_tray_item(icon_window)
    }

    fn recaclulate_layout(&mut self) {
        let size = self.tray.layout(self.window_size.unsnap()).snap();

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

        self.tray.render(&mut context);

        context.commit();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        for tray_item in self.tray.widget.tray_items() {
            if tray_item.widget.is_embedded() {
                unsafe {
                    release_embedding(self.display, tray_item.widget.icon_window());
                }
            }
        }

        self.text_renderer.clear_caches(self.display);

        if let Some(previous_selection_owner) = self.previous_selection_owner.take() {
            unsafe {
                release_tray_selection(self.display, previous_selection_owner);
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

#[derive(Debug)]
struct TrayMessage {
    buffer: Vec<u8>,
    timeout: Duration,
    id: i64,
}

impl TrayMessage {
    fn new(data: &xlib::ClientMessageData) -> Self {
        Self {
            buffer: Vec::with_capacity(data.get_long(3) as usize),
            timeout: Duration::from_millis(data.get_long(2) as u64),
            id: data.get_long(4),
        }
    }

    fn receive_message(&mut self, bytes: &[u8]) -> usize {
        let remaining_len = self.buffer.capacity() - self.buffer.len();
        let receiving_len = remaining_len.max(20);
        self.buffer.extend_from_slice(&bytes[..receiving_len]);
        self.buffer.capacity() - self.buffer.len()
    }

    fn as_string(&self) -> &str {
        str::from_utf8(self.buffer.as_slice())
            .ok()
            .unwrap_or_default()
    }
}

unsafe fn create_window(
    display: *mut xlib::Display,
    position: PhysicalPoint,
    size: PhysicalSize,
) -> xlib::Window {
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
}

unsafe fn acquire_tray_selection(
    display: *mut xlib::Display,
    window: xlib::Window,
) -> xlib::Window {
    let screen = xlib::XDefaultScreenOfDisplay(display);
    let screen_number = xlib::XScreenNumberOfScreen(screen);
    let root = xlib::XRootWindowOfScreen(screen);
    let manager_atom = utils::new_atom(display, "MANAGER\0");
    let net_system_tray_atom =
        utils::new_atom(display, &format!("_NET_SYSTEM_TRAY_S{}\0", screen_number));

    let previous_selection_owner = xlib::XGetSelectionOwner(display, net_system_tray_atom);
    xlib::XSetSelectionOwner(display, net_system_tray_atom, window, xlib::CurrentTime);

    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, xlib::CurrentTime as c_long);
    data.set_long(1, net_system_tray_atom as c_long);
    data.set_long(2, window as c_long);

    utils::send_client_message(display, root, root, manager_atom, data);

    previous_selection_owner
}

unsafe fn release_tray_selection(
    display: *mut xlib::Display,
    previous_selection_owner: xlib::Window,
) {
    let screen = xlib::XDefaultScreenOfDisplay(display);
    let screen_number = xlib::XScreenNumberOfScreen(screen);
    let net_system_tray_atom =
        utils::new_atom(display, &format!("_NET_SYSTEM_TRAY_S{}\0", screen_number));

    xlib::XSetSelectionOwner(
        display,
        net_system_tray_atom,
        previous_selection_owner,
        xlib::CurrentTime,
    );
}

unsafe fn request_embedding(
    display: *mut xlib::Display,
    icon_window: xlib::Window,
    embedder_window: xlib::Window,
    xembed_info: &XEmbedInfo,
    atoms: &Atoms,
) {
    send_xembed_message(
        display,
        atoms,
        icon_window,
        embedder_window,
        XEmbedMessage::EmbeddedNotify,
        xembed_info.version,
    );

    if xembed_info.is_mapped() {
        xlib::XSelectInput(
            display,
            icon_window,
            xlib::PropertyChangeMask | xlib::StructureNotifyMask,
        );
        xlib::XReparentWindow(display, icon_window, embedder_window, 0, 0);
    } else {
        xlib::XSelectInput(display, icon_window, xlib::PropertyChangeMask);
    }

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
}

unsafe fn release_embedding(display: *mut xlib::Display, icon_window: xlib::Window) {
    let screen = xlib::XDefaultScreenOfDisplay(display);
    let root = xlib::XRootWindowOfScreen(screen);

    xlib::XSelectInput(display, icon_window, xlib::NoEventMask);
    xlib::XReparentWindow(display, icon_window, root, 0, 0);
    xlib::XUnmapWindow(display, icon_window);
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
