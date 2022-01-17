use keytray_shell::graphics::Color;
use std::collections::hash_map;
use std::collections::HashMap;
use std::rc::Rc;
use std::str;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::errors::{ReplyError, ReplyOrIdError};
use x11rb::protocol;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt;
use x11rb::wrapper::ConnectionExt as _;

use crate::xembed::{XEmbedInfo, XEmbedMessage};

const SYSTEM_TRAY_REQUEST_DOCK: u32 = 0;
const SYSTEM_TRAY_BEGIN_MESSAGE: u32 = 1;
const SYSTEM_TRAY_CANCEL_MESSAGE: u32 = 2;

#[derive(Debug)]
pub struct TrayManager<Connection: self::Connection> {
    connection: Rc<Connection>,
    screen_num: usize,
    manager_window: xproto::Window,
    status: TrayStatus,
    atoms: Atoms,
    system_tray_selection_atom: xproto::Atom,
    embedded_icons: HashMap<xproto::Window, XEmbedInfo>,
    balloon_messages: HashMap<xproto::Window, BalloonMessage>,
}

impl<Connection: self::Connection> TrayManager<Connection> {
    pub fn new(
        connection: Rc<Connection>,
        screen_num: usize,
        orientation: SystemTrayOrientation,
        colors: SystemTrayColors,
    ) -> Result<Self, ReplyOrIdError> {
        let manager_window = connection.generate_id()?;
        let atoms = Atoms::new(connection.as_ref())?.reply()?;
        let system_tray_selection_atom = connection
            .intern_atom(
                false,
                &format!("_NET_SYSTEM_TRAY_S{}", screen_num).as_bytes(),
            )?
            .reply()?
            .atom;

        {
            let screen = &connection.setup().roots[screen_num];
            let values =
                xproto::CreateWindowAux::new().event_mask(xproto::EventMask::PROPERTY_CHANGE);

            connection
                .create_window(
                    0,
                    manager_window,
                    screen.root,
                    0, // x
                    0, // y
                    1, // width
                    1, // height
                    0, // border_width
                    xproto::WindowClass::INPUT_ONLY,
                    x11rb::COPY_FROM_PARENT,
                    &values,
                )?
                .check()?;

            connection
                .change_property32(
                    xproto::PropMode::REPLACE,
                    manager_window,
                    atoms._NET_SYSTEM_TRAY_ORIENTATION,
                    xproto::AtomEnum::CARDINAL,
                    &[orientation.0],
                )?
                .check()?;

            connection
                .change_property32(
                    xproto::PropMode::REPLACE,
                    manager_window,
                    atoms._NET_SYSTEM_TRAY_VISUAL,
                    xproto::AtomEnum::VISUALID,
                    &[screen.root_visual],
                )?
                .check()?;

            connection
                .change_property(
                    xproto::PropMode::REPLACE,
                    manager_window,
                    atoms._NET_SYSTEM_TRAY_COLORS,
                    xproto::AtomEnum::CARDINAL,
                    32,
                    12,
                    colors.as_bytes(),
                )?
                .check()?;
        }

        Ok(Self {
            connection,
            screen_num,
            manager_window,
            status: TrayStatus::Unmanaged,
            atoms,
            system_tray_selection_atom,
            embedded_icons: HashMap::new(),
            balloon_messages: HashMap::new(),
        })
    }

    pub fn acquire_tray_selection(&mut self) -> Result<bool, ReplyError> {
        if matches!(self.status, TrayStatus::Managed | TrayStatus::Pending(_)) {
            return Ok(false);
        }

        let selection_owner_reply = self
            .connection
            .get_selection_owner(self.system_tray_selection_atom)?
            .reply()?;
        let previous_selection_owner = selection_owner_reply.owner;

        self.connection
            .set_selection_owner(
                self.manager_window,
                self.system_tray_selection_atom,
                x11rb::CURRENT_TIME,
            )?
            .check()?;

        if previous_selection_owner == x11rb::NONE {
            self.broadcast_manager_message()?;
            self.status = TrayStatus::Managed;
        } else {
            let values = xproto::ChangeWindowAttributesAux::new()
                .event_mask(Some(xproto::EventMask::STRUCTURE_NOTIFY.into()));

            self.connection
                .change_window_attributes(previous_selection_owner, &values)?
                .check()?;

            self.status = TrayStatus::Pending(previous_selection_owner);
        }

        Ok(true)
    }

    pub fn process_event(
        &mut self,
        embedder_window: xproto::Window,
        event: &protocol::Event,
    ) -> Result<Option<TrayEvent>, ReplyError> {
        use protocol::Event::*;

        let response = match event {
            ClientMessage(event) if event.type_ == self.atoms._NET_SYSTEM_TRAY_OPCODE => {
                let data = event.data.as_data32();
                let opcode = data[1];
                if opcode == SYSTEM_TRAY_REQUEST_DOCK {
                    let window = data[2];
                    self.register_tray_icon(embedder_window, window)?;
                } else if opcode == SYSTEM_TRAY_BEGIN_MESSAGE {
                    let balloon_message = BalloonMessage::new(event.data.as_data32());
                    self.balloon_messages.insert(event.window, balloon_message);
                } else if opcode == SYSTEM_TRAY_CANCEL_MESSAGE {
                    if let hash_map::Entry::Occupied(entry) =
                        self.balloon_messages.entry(event.window)
                    {
                        let [_, _, id, ..] = event.data.as_data32();
                        if entry.get().id == id {
                            entry.remove();
                        }
                    }
                }
                None
            }
            ClientMessage(event) if event.type_ == self.atoms._NET_SYSTEM_TRAY_MESSAGE_DATA => {
                if let hash_map::Entry::Occupied(mut entry) =
                    self.balloon_messages.entry(event.window)
                {
                    entry.get_mut().write_message(&event.data.as_data8());
                    if entry.get().remaining_len() == 0 {
                        let balloon_message = entry.remove();
                        Some(TrayEvent::MessageReceived(event.window, balloon_message))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            SelectionClear(event)
                if event.selection == self.system_tray_selection_atom
                    && event.owner == self.manager_window =>
            {
                for (window, xembed_info) in self.embedded_icons.drain() {
                    if xembed_info.is_mapped() {
                        release_embedding(self.connection.as_ref(), self.screen_num, window)?;
                    }
                }
                self.status = TrayStatus::Unmanaged;
                Some(TrayEvent::SelectionCleared)
            }
            PropertyNotify(event) if event.atom == self.atoms._XEMBED_INFO => {
                self.register_tray_icon(embedder_window, event.window)?;
                None
            }
            // Ignore from SUBSTRUCTURE_NOTIFY.
            ReparentNotify(event) if event.event == event.window => {
                if event.parent == embedder_window {
                    Some(TrayEvent::TrayIconAdded(event.window))
                } else {
                    if self.unregister_tray_icon(event.window) {
                        Some(TrayEvent::TrayIconRemoved(event.window))
                    } else {
                        None
                    }
                }
            }
            DestroyNotify(event) => match self.status {
                TrayStatus::Pending(window) if event.window == window => {
                    self.broadcast_manager_message()?;
                    None
                }
                _ => {
                    if self.unregister_tray_icon(event.window) {
                        Some(TrayEvent::TrayIconRemoved(event.window))
                    } else {
                        None
                    }
                }
            },
            _ => None,
        };

        Ok(response)
    }

    fn release_tray_selection(&mut self) -> Result<(), ReplyError> {
        if matches!(self.status, TrayStatus::Managed) {
            for (window, xembed_info) in self.embedded_icons.drain() {
                if xembed_info.is_mapped() {
                    release_embedding(self.connection.as_ref(), self.screen_num, window)?;
                }
            }

            self.connection
                .set_selection_owner(
                    x11rb::NONE,
                    self.system_tray_selection_atom,
                    x11rb::CURRENT_TIME,
                )?
                .check()?;

            self.status = TrayStatus::Unmanaged;
        }

        Ok(())
    }

    fn register_tray_icon(
        &mut self,
        embedder_window: xproto::Window,
        icon_window: xproto::Window,
    ) -> Result<(), ReplyError> {
        if let Some(xembed_info) =
            get_xembed_info(self.connection.as_ref(), icon_window, &self.atoms)?
        {
            let old_state = self
                .embedded_icons
                .insert(icon_window, xembed_info)
                .map(|xembed_info| xembed_info.is_mapped());

            match (old_state, xembed_info.is_mapped()) {
                (None, false) => {
                    send_xembed_message(
                        self.connection.as_ref(),
                        icon_window,
                        embedder_window,
                        XEmbedMessage::EmbeddedNotify,
                        xembed_info.version,
                        &self.atoms,
                    )?;
                    wait_for_embedding(self.connection.as_ref(), icon_window)?;
                }
                (None, true) => {
                    send_xembed_message(
                        self.connection.as_ref(),
                        icon_window,
                        embedder_window,
                        XEmbedMessage::EmbeddedNotify,
                        xembed_info.version,
                        &self.atoms,
                    )?;
                    begin_embedding(self.connection.as_ref(), icon_window, embedder_window)?;
                }
                (Some(false), true) => {
                    begin_embedding(self.connection.as_ref(), icon_window, embedder_window)?;
                }
                (Some(true), false) => {
                    release_embedding(self.connection.as_ref(), self.screen_num, icon_window)?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn unregister_tray_icon(&mut self, icon_window: xproto::Window) -> bool {
        self.balloon_messages.remove(&icon_window);
        self.embedded_icons.remove(&icon_window).is_some()
    }

    fn broadcast_manager_message(&self) -> Result<(), ReplyError> {
        let screen = &self.connection.setup().roots[self.screen_num];
        let event = xproto::ClientMessageEvent::new(
            32,
            screen.root,
            self.atoms.MANAGER,
            [
                x11rb::CURRENT_TIME,
                self.system_tray_selection_atom,
                self.manager_window,
                0,
                0,
            ],
        );

        self.connection
            .send_event(
                false,
                screen.root,
                xproto::EventMask::STRUCTURE_NOTIFY,
                event,
            )?
            .check()
    }
}

impl<Connection: self::Connection> Drop for TrayManager<Connection> {
    fn drop(&mut self) {
        self.release_tray_selection().ok();
        self.connection.destroy_window(self.manager_window).ok();
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SystemTrayOrientation(u32);

#[allow(unused)]
impl SystemTrayOrientation {
    pub const HORZONTAL: Self = Self(0);
    pub const VERTICAL: Self = Self(1);
}

#[repr(C)]
#[derive(Debug)]
pub struct SystemTrayColors {
    normal: [u32; 3],
    error: [u32; 3],
    warning: [u32; 3],
    success: [u32; 3],
}

impl SystemTrayColors {
    pub fn new(normal: Color, success: Color, warning: Color, error: Color) -> SystemTrayColors {
        let normal_components = normal.to_u16_rgba();
        let success_components = success.to_u16_rgba();
        let warning_components = warning.to_u16_rgba();
        let error_components = error.to_u16_rgba();
        SystemTrayColors {
            normal: [
                normal_components[0] as u32,
                normal_components[1] as u32,
                normal_components[2] as u32,
            ],
            success: [
                success_components[0] as u32,
                success_components[1] as u32,
                success_components[2] as u32,
            ],
            warning: [
                warning_components[0] as u32,
                warning_components[1] as u32,
                warning_components[2] as u32,
            ],
            error: [
                error_components[0] as u32,
                error_components[1] as u32,
                error_components[2] as u32,
            ],
        }
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self) as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}

#[derive(Debug)]
pub enum TrayEvent {
    TrayIconAdded(xproto::Window),
    TrayIconRemoved(xproto::Window),
    MessageReceived(xproto::Window, BalloonMessage),
    SelectionCleared,
}

#[derive(Debug)]
enum TrayStatus {
    Unmanaged,
    Pending(xproto::Window),
    Managed,
}

#[derive(Debug)]
#[allow(unused)]
pub struct BalloonMessage {
    buffer: Vec<u8>,
    timeout: Duration,
    length: usize,
    id: u32,
}

#[allow(unused)]
impl BalloonMessage {
    fn new(data: [u32; 5]) -> Self {
        let [_, _, timeout, length, id] = data;
        let length = length as usize;
        Self {
            buffer: Vec::with_capacity(length),
            timeout: Duration::from_millis(timeout as u64),
            length,
            id,
        }
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.buffer.as_slice())
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
                assert_eq!(self.buffer.capacity(), self.buffer.len());
            }
        }
    }
}

fn begin_embedding<Connection: self::Connection>(
    connection: &Connection,
    icon_window: xproto::Window,
    embedder_window: xproto::Window,
) -> Result<(), ReplyError> {
    {
        let values = xproto::ChangeWindowAttributesAux::new().event_mask(Some(
            (xproto::EventMask::PROPERTY_CHANGE | xproto::EventMask::STRUCTURE_NOTIFY).into(),
        ));
        connection
            .change_window_attributes(icon_window, &values)?
            .check()?;
    }

    connection
        .reparent_window(icon_window, embedder_window, 0, 0)?
        .check()?;
    connection.flush()?;

    Ok(())
}

fn wait_for_embedding<Connection: self::Connection>(
    connection: &Connection,
    icon_window: xproto::Window,
) -> Result<(), ReplyError> {
    {
        let values =
            xproto::ChangeWindowAttributesAux::new().event_mask(xproto::EventMask::PROPERTY_CHANGE);
        connection
            .change_window_attributes(icon_window, &values)?
            .check()?;
    }
    connection.flush()?;

    Ok(())
}

fn release_embedding<Connection: self::Connection>(
    connection: &Connection,
    screen_num: usize,
    icon_window: xproto::Window,
) -> Result<(), ReplyError> {
    let screen = &connection.setup().roots[screen_num];

    {
        let values = xproto::ChangeWindowAttributesAux::new()
            .event_mask(Some(xproto::EventMask::NO_EVENT.into()));
        connection
            .change_window_attributes(icon_window, &values)?
            .check()?;
    }

    connection
        .reparent_window(icon_window, screen.root, 0, 0)?
        .check()?;
    connection.unmap_window(icon_window)?.check()?;
    connection.flush()?;

    Ok(())
}

fn get_xembed_info<Connection: self::Connection>(
    connection: &Connection,
    window: xproto::Window,
    atoms: &Atoms,
) -> Result<Option<XEmbedInfo>, ReplyError> {
    let reply = connection
        .get_property(
            false,
            window,
            atoms._XEMBED_INFO,
            xproto::AtomEnum::ANY,
            0,
            2,
        )?
        .reply()?;
    if let Some(data) = reply
        .value32()
        .map(|iter| iter.collect::<Vec<_>>())
        .filter(|data| data.len() == 2)
    {
        Ok(Some(XEmbedInfo {
            version: data[0],
            flags: data[1],
        }))
    } else {
        Ok(None)
    }
}

fn send_xembed_message<Connection: self::Connection>(
    connection: &Connection,
    window: xproto::Window,
    embedder_window: xproto::Window,
    xembed_message: XEmbedMessage,
    xembed_version: u32,
    atoms: &Atoms,
) -> Result<(), ReplyError> {
    let event = xproto::ClientMessageEvent::new(
        32,
        window,
        atoms._XEMBED,
        [
            x11rb::CURRENT_TIME,
            xembed_message.into(),
            embedder_window,
            xembed_version,
            0,
        ],
    );

    connection
        .send_event(false, window, xproto::EventMask::STRUCTURE_NOTIFY, event)?
        .check()
}

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        MANAGER,
        _NET_SYSTEM_TRAY_COLORS,
        _NET_SYSTEM_TRAY_MESSAGE_DATA,
        _NET_SYSTEM_TRAY_OPCODE,
        _NET_SYSTEM_TRAY_ORIENTATION,
        _NET_SYSTEM_TRAY_VISUAL,
        _XEMBED,
        _XEMBED_INFO,
    }
}
