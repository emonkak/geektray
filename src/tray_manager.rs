use std::collections::hash_map;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::*;
use std::rc::Rc;
use std::time::Duration;
use x11::xlib;

use crate::atoms::Atoms;
use crate::event_loop::X11Event;
use crate::utils;
use crate::xembed::{XEmbedInfo, XEmbedMessage};

const SYSTEM_TRAY_REQUEST_DOCK: c_long = 0;
const SYSTEM_TRAY_BEGIN_MESSAGE: c_long = 1;
const SYSTEM_TRAY_CANCEL_MESSAGE: c_long = 2;

#[derive(Debug)]
pub struct TrayManager {
    display: *mut xlib::Display,
    window: xlib::Window,
    status: TrayStatus,
    system_tray_atom: xlib::Atom,
    atoms: Rc<Atoms>,
    embedded_icons: HashMap<xlib::Window, XEmbedInfo>,
    balloon_messages: HashMap<xlib::Window, BalloonMessage>,
}

impl TrayManager {
    pub fn new(display: *mut xlib::Display, window: xlib::Window, atoms: Rc<Atoms>) -> Self {
        let system_tray_atom = unsafe {
            let screen_number = xlib::XDefaultScreen(display);
            utils::new_atom(display, &format!("_NET_SYSTEM_TRAY_S{}\0", screen_number))
        };

        Self {
            display,
            window,
            status: TrayStatus::Waiting,
            system_tray_atom,
            atoms,
            embedded_icons: HashMap::new(),
            balloon_messages: HashMap::new(),
        }
    }

    pub fn acquire_tray_selection(&mut self) {
        if matches!(self.status, TrayStatus::Managed | TrayStatus::Pending(_)) {
            return;
        }

        unsafe {
            let previous_selection_owner =
                xlib::XGetSelectionOwner(self.display, self.system_tray_atom);
            xlib::XSetSelectionOwner(
                self.display,
                self.system_tray_atom,
                self.window,
                xlib::CurrentTime,
            );
            if previous_selection_owner == 0 {
                broadcast_manager_message(
                    self.display,
                    self.window,
                    self.system_tray_atom,
                    &self.atoms,
                );
                self.status = TrayStatus::Managed;
            } else {
                xlib::XSelectInput(
                    self.display,
                    previous_selection_owner,
                    xlib::StructureNotifyMask,
                );
                self.status = TrayStatus::Pending(previous_selection_owner);
            }
        }
    }

    pub fn release_tray_selection(&mut self) {
        if matches!(self.status, TrayStatus::Managed) {
            for (window, xembed_info) in self.embedded_icons.drain() {
                if xembed_info.is_mapped() {
                    unsafe {
                        release_embedding(self.display, window);
                    }
                }
            }

            unsafe {
                xlib::XSetSelectionOwner(self.display, self.system_tray_atom, 0, xlib::CurrentTime);
            }

            self.status = TrayStatus::Waiting;
        }
    }

    pub fn process_event<F>(&mut self, event: &X11Event, mut callback: F)
    where
        F: FnMut(TrayEvent),
    {
        match event {
            X11Event::ClientMessage(event)
                if event.message_type == self.atoms.NET_SYSTEM_TRAY_OPCODE =>
            {
                let opcode = event.data.get_long(1);
                if opcode == SYSTEM_TRAY_REQUEST_DOCK {
                    let icon_window = event.data.get_long(2) as xlib::Window;
                    if let Some(xembed_info) =
                        unsafe { get_xembed_info(self.display, icon_window, &self.atoms) }
                    {
                        if let Some(event) = self.register_tray_icon(icon_window, xembed_info) {
                            callback(event);
                        }
                    }
                } else if opcode == SYSTEM_TRAY_BEGIN_MESSAGE {
                    let balloon_message = BalloonMessage::new(&event.data);
                    self.balloon_messages.insert(event.window, balloon_message);
                } else if opcode == SYSTEM_TRAY_CANCEL_MESSAGE {
                    if let hash_map::Entry::Occupied(entry) =
                        self.balloon_messages.entry(event.window)
                    {
                        let id = event.data.get_long(2);
                        if entry.get().id == id {
                            entry.remove();
                        }
                    }
                }
            }
            X11Event::ClientMessage(event)
                if event.message_type == self.atoms.NET_SYSTEM_TRAY_MESSAGE_DATA =>
            {
                if let hash_map::Entry::Occupied(mut entry) =
                    self.balloon_messages.entry(event.window)
                {
                    entry.get_mut().write_message(event.data.as_ref());
                    if entry.get().remaining_len() == 0 {
                        let balloon_message = entry.remove();
                        callback(TrayEvent::BalloonMessageReceived(
                            event.window,
                            balloon_message,
                        ));
                    }
                }
            }
            X11Event::SelectionClear(event) if event.selection == self.system_tray_atom => {
                if event.window == self.window {
                    self.embedded_icons.clear();
                    self.status = TrayStatus::Waiting;
                    callback(TrayEvent::SelectionCleared);
                }
            }
            X11Event::PropertyNotify(event) if event.atom == self.atoms.XEMBED_INFO => {
                if let Some(xembed_info) =
                    unsafe { get_xembed_info(self.display, event.window, &self.atoms) }
                {
                    if let Some(event) = self.register_tray_icon(event.window, xembed_info) {
                        callback(event);
                    }
                }
            }
            X11Event::ReparentNotify(event) => {
                if event.parent != self.window {
                    let event = self.unregister_tray_icon(event.window);
                    callback(event);
                }
            }
            X11Event::DestroyNotify(event) => match self.status {
                TrayStatus::Pending(window) if event.window == window => unsafe {
                    broadcast_manager_message(
                        self.display,
                        self.window,
                        self.system_tray_atom,
                        &self.atoms,
                    );
                },
                _ => {
                    let event = self.unregister_tray_icon(event.window);
                    callback(event);
                }
            },
            _ => {}
        }
    }

    fn register_tray_icon(
        &mut self,
        icon_window: xlib::Window,
        xembed_info: XEmbedInfo,
    ) -> Option<TrayEvent> {
        let old_is_mapped = self
            .embedded_icons
            .insert(icon_window, xembed_info)
            .map(|xembed_info| xembed_info.is_mapped());

        match (old_is_mapped, xembed_info.is_mapped()) {
            (None, false) => unsafe {
                send_xembed_message(
                    self.display,
                    icon_window,
                    self.window,
                    XEmbedMessage::EmbeddedNotify,
                    xembed_info.version,
                    &self.atoms,
                );
                wait_for_embedding(self.display, icon_window);
                None
            },
            (None, true) => {
                unsafe {
                    send_xembed_message(
                        self.display,
                        icon_window,
                        self.window,
                        XEmbedMessage::EmbeddedNotify,
                        xembed_info.version,
                        &self.atoms,
                    );
                    begin_embedding(self.display, icon_window, self.window);
                }
                Some(TrayEvent::TrayIconAdded(icon_window))
            }
            (Some(false), true) => {
                unsafe {
                    begin_embedding(self.display, icon_window, self.window);
                }
                Some(TrayEvent::TrayIconAdded(icon_window))
            }
            (Some(true), false) => {
                unsafe {
                    release_embedding(self.display, icon_window);
                }
                Some(TrayEvent::TrayIconRemoved(icon_window))
            }
            _ => None,
        }
    }

    fn unregister_tray_icon(&mut self, icon_window: xlib::Window) -> TrayEvent {
        if let Some(xembed_info) = self.embedded_icons.remove(&icon_window) {
            if xembed_info.is_mapped() {
                unsafe {
                    release_embedding(self.display, icon_window);
                }
            }
        }

        self.balloon_messages.remove(&icon_window);

        TrayEvent::TrayIconRemoved(icon_window)
    }
}

#[derive(Debug)]
pub enum TrayEvent {
    TrayIconAdded(xlib::Window),
    TrayIconRemoved(xlib::Window),
    BalloonMessageReceived(xlib::Window, BalloonMessage),
    SelectionCleared,
}

#[derive(Debug)]
enum TrayStatus {
    Waiting,
    Pending(xlib::Window),
    Managed,
}

#[derive(Debug)]
pub struct BalloonMessage {
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

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn as_c_str(&self) -> &CStr {
        CStr::from_bytes_with_nul(self.buffer.as_slice())
            .ok()
            .unwrap_or_default()
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

unsafe fn wait_for_embedding(display: *mut xlib::Display, icon_window: xlib::Window) {
    xlib::XSelectInput(display, icon_window, xlib::PropertyChangeMask);
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

unsafe fn send_xembed_message(
    display: *mut xlib::Display,
    window: xlib::Window,
    embedder_window: xlib::Window,
    xembed_message: XEmbedMessage,
    xembed_version: u64,
    atoms: &Atoms,
) {
    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, xlib::CurrentTime as c_long);
    data.set_long(1, xembed_message as c_long);
    data.set_long(2, embedder_window as c_long);
    data.set_long(3, xembed_version as c_long);

    utils::send_client_message(display, window, window, atoms.XEMBED, data);
}
