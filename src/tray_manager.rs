use std::collections::hash_map;
use std::collections::HashMap;
use std::ffi::CStr;
use std::rc::Rc;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::errors::{ReplyError, ReplyOrIdError};
use x11rb::protocol;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt;

use crate::atoms::Atoms;
use crate::ui::{XEmbedInfo, XEmbedMessage};
use crate::utils;

const SYSTEM_TRAY_REQUEST_DOCK: u32 = 0;
const SYSTEM_TRAY_BEGIN_MESSAGE: u32 = 1;
const SYSTEM_TRAY_CANCEL_MESSAGE: u32 = 2;

#[derive(Debug)]
pub struct TrayManager<Connection: self::Connection> {
    connection: Rc<Connection>,
    screen_num: usize,
    manager_window: xproto::Window,
    status: TrayStatus,
    atoms: Rc<Atoms>,
    embedded_icons: HashMap<xproto::Window, XEmbedInfo>,
    balloon_messages: HashMap<xproto::Window, BalloonMessage>,
}

impl<Connection: self::Connection> TrayManager<Connection> {
    pub fn new(
        connection: Rc<Connection>,
        screen_num: usize,
        atoms: Rc<Atoms>,
    ) -> Result<Self, ReplyOrIdError> {
        let manager_window = connection.generate_id()?;

        {
            let screen = &connection.setup().roots[screen_num];
            let values = xproto::CreateWindowAux::new()
                .event_mask(xproto::EventMask::PROPERTY_CHANGE);

            connection.create_window(
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
            )?.check()?;
        }

        Ok(Self {
            connection,
            screen_num,
            manager_window,
            status: TrayStatus::Waiting,
            atoms,
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
            .get_selection_owner(self.atoms._NET_SYSTEM_TRAY_S)?
            .reply()?;
        let previous_selection_owner = selection_owner_reply.owner;

        self.connection
            .set_selection_owner(
                self.manager_window,
                self.atoms._NET_SYSTEM_TRAY_S,
                x11rb::CURRENT_TIME,
            )?
            .check()?;

        if previous_selection_owner == x11rb::NONE {
            broadcast_manager_message(
                self.connection.as_ref(),
                self.screen_num,
                self.manager_window,
                &self.atoms,
            )?;
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
                    if self.register_tray_icon(embedder_window, window)? {
                        Some(TrayEvent::TrayIconAdded(window))
                    } else {
                        None
                    }
                } else if opcode == SYSTEM_TRAY_BEGIN_MESSAGE {
                    let balloon_message = BalloonMessage::new(event.data.as_data32());
                    self.balloon_messages.insert(event.window, balloon_message);
                    None
                } else if opcode == SYSTEM_TRAY_CANCEL_MESSAGE {
                    if let hash_map::Entry::Occupied(entry) =
                        self.balloon_messages.entry(event.window)
                    {
                        let [_, _, id, ..] = event.data.as_data32();
                        if entry.get().id == id {
                            entry.remove();
                        }
                    }
                    None
                } else {
                    None
                }
            }
            ClientMessage(event) if event.type_ == self.atoms._NET_SYSTEM_TRAY_MESSAGE_DATA => {
                if let hash_map::Entry::Occupied(mut entry) =
                    self.balloon_messages.entry(event.window)
                {
                    entry.get_mut().write_message(&event.data.as_data8());
                    if entry.get().remaining_len() == 0 {
                        let balloon_message = entry.remove();
                        Some(TrayEvent::BalloonMessageReceived(
                            event.window,
                            balloon_message,
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            SelectionClear(event)
                if event.selection == self.atoms._NET_SYSTEM_TRAY_S
                    && event.owner == self.manager_window =>
            {
                self.embedded_icons.clear();
                self.status = TrayStatus::Waiting;
                Some(TrayEvent::SelectionCleared)
            }
            PropertyNotify(event) if event.atom == self.atoms._XEMBED_INFO => {
                if self.register_tray_icon(embedder_window, event.window)? {
                    Some(TrayEvent::TrayIconAdded(event.window))
                } else {
                    None
                }
            }
            ReparentNotify(event) if event.parent != embedder_window => {
                self.unregister_tray_icon(event.window)?;
                Some(TrayEvent::TrayIconRemoved(event.window))
            }
            DestroyNotify(event) => match self.status {
                TrayStatus::Pending(window) if event.window == window => {
                    broadcast_manager_message(
                        self.connection.as_ref(),
                        self.screen_num,
                        self.manager_window,
                        &self.atoms,
                    )?;
                    None
                }
                _ => {
                    self.unregister_tray_icon(event.window)?;
                    Some(TrayEvent::TrayIconRemoved(event.window))
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
                    self.atoms._NET_SYSTEM_TRAY_S,
                    x11rb::CURRENT_TIME,
                )?
                .check()?;

            self.status = TrayStatus::Waiting;
        }

        Ok(())
    }

    fn register_tray_icon(&mut self, embedder_window: xproto::Window, icon_window: xproto::Window) -> Result<bool, ReplyError> {
        let is_embedded = if let Some(xembed_info) =
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
                    false
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
                    true
                }
                (Some(false), true) => {
                    begin_embedding(self.connection.as_ref(), icon_window, embedder_window)?;
                    true
                }
                (Some(true), false) => {
                    release_embedding(self.connection.as_ref(), self.screen_num, icon_window)?;
                    false
                }
                _ => false,
            }
        } else {
            false
        };

        Ok(is_embedded)
    }

    fn unregister_tray_icon(&mut self, icon_window: xproto::Window) -> Result<(), ReplyError> {
        if let Some(xembed_info) = self.embedded_icons.remove(&icon_window) {
            if xembed_info.is_mapped() {
                release_embedding(self.connection.as_ref(), self.screen_num, icon_window)?;
            }
        }

        self.balloon_messages.remove(&icon_window);

        Ok(())
    }
}

impl<Connection: self::Connection> Drop for TrayManager<Connection> {
    fn drop(&mut self) {
        self.release_tray_selection().ok();
        self.connection.destroy_window(self.manager_window).ok();
    }
}

#[derive(Debug)]
pub enum TrayEvent {
    TrayIconAdded(xproto::Window),
    TrayIconRemoved(xproto::Window),
    BalloonMessageReceived(xproto::Window, BalloonMessage),
    SelectionCleared,
}

#[derive(Debug)]
enum TrayStatus {
    Waiting,
    Pending(xproto::Window),
    Managed,
}

#[derive(Debug)]
pub struct BalloonMessage {
    buffer: Vec<u8>,
    timeout: Duration,
    length: usize,
    id: u32,
}

impl BalloonMessage {
    fn new(data: [u32; 5]) -> Self {
        let [_, _, timeout, length, id] = data;
        let length = length as usize;
        Self {
            buffer: Vec::with_capacity(length + 1),
            timeout: Duration::from_millis(timeout as u64),
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

fn broadcast_manager_message<Connection: self::Connection>(
    connection: &Connection,
    screen_num: usize,
    window: xproto::Window,
    atoms: &Atoms,
) -> Result<(), ReplyError> {
    let screen = &connection.setup().roots[screen_num];
    let event = xproto::ClientMessageEvent::new(
        32,
        screen.root,
        atoms.MANAGER,
        [x11rb::CURRENT_TIME, atoms._NET_SYSTEM_TRAY_S, window, 0, 0],
    );

    connection
        .send_event(
            false,
            screen.root,
            xproto::EventMask::STRUCTURE_NOTIFY,
            event,
        )?
        .check()
}

fn begin_embedding<Connection: self::Connection>(
    connection: &Connection,
    icon_window: xproto::Window,
    embedder_window: xproto::Window,
) -> Result<(), ReplyError> {
    {
        let values = xproto::ChangeWindowAttributesAux::new()
            .event_mask(Some((xproto::EventMask::PROPERTY_CHANGE | xproto::EventMask::STRUCTURE_NOTIFY).into()));
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
        let values = xproto::ChangeWindowAttributesAux::new()
            .event_mask(xproto::EventMask::PROPERTY_CHANGE);
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
    let xembed_info =
        utils::get_fixed_property::<_, u32, 2>(connection, window, atoms._XEMBED_INFO)?.map(
            |data| XEmbedInfo {
                version: data[0],
                flags: data[1],
            },
        );
    Ok(xembed_info)
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
