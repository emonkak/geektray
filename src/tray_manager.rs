use anyhow::Context as _;
use std::mem;
use std::rc::Rc;
use std::str;
use x11rb::connection::Connection;
use x11rb::protocol;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt;

use crate::atoms::Atoms;

const SYSTEM_TRAY_REQUEST_DOCK: u32 = 0;
const SYSTEM_TRAY_BEGIN_MESSAGE: u32 = 1;
const SYSTEM_TRAY_CANCEL_MESSAGE: u32 = 2;

const XEMBED_MAPPED: u32 = 1 << 0;

#[derive(Debug)]
pub struct TrayManager<C: Connection> {
    connection: Rc<C>,
    screen_num: usize,
    system_tray_selection_atom: xproto::Atom,
    atoms: Rc<Atoms>,
    ownership: Ownership,
    icons: Vec<xproto::Window>,
    balloon_messages: Vec<BalloonMessage>,
}

impl<C: Connection> TrayManager<C> {
    pub fn new(connection: Rc<C>, screen_num: usize, atoms: Rc<Atoms>) -> anyhow::Result<Self> {
        let system_tray_selection_atom =
            intern_system_tray_selection_atom(&*connection, screen_num)?;

        Ok(Self {
            connection,
            screen_num,
            atoms,
            system_tray_selection_atom,
            ownership: Ownership::Unmanaged,
            icons: Vec::new(),
            balloon_messages: Vec::new(),
        })
    }

    pub fn acquire_tray_selection(
        &mut self,
        new_selection_owner: xproto::Window,
    ) -> anyhow::Result<()> {
        let current_selection_owner = self
            .connection
            .get_selection_owner(self.system_tray_selection_atom)?
            .reply()
            .context("get selection owner")?
            .owner;

        log::info!(
            "acquire tray selection (current_selection_owner: {}, new_selection_owner: {})",
            current_selection_owner,
            new_selection_owner,
        );

        self.connection
            .set_selection_owner(
                new_selection_owner,
                self.system_tray_selection_atom,
                x11rb::CURRENT_TIME,
            )?
            .check()
            .context("set selection owner")?;

        if current_selection_owner == x11rb::NONE {
            self.broadcast_manager_message(new_selection_owner)?;
        } else {
            self.wait_for_destroy_selection_owner(current_selection_owner, new_selection_owner)?;
        }

        Ok(())
    }

    pub fn release_tray_selection(&mut self) -> anyhow::Result<()> {
        match self.ownership {
            Ownership::Managed(current_selection_owner) => {
                log::info!(
                    "release tray selection (current_selection_owner: {})",
                    current_selection_owner
                );

                self.connection
                    .set_selection_owner(
                        x11rb::NONE,
                        self.system_tray_selection_atom,
                        x11rb::CURRENT_TIME,
                    )?
                    .check()
                    .context("reset tray selection")?;

                self.clear_embeddings()?;
            }
            _ => {}
        }

        self.ownership = Ownership::Unmanaged;

        Ok(())
    }

    pub fn translate_event(
        &mut self,
        event: &protocol::Event,
    ) -> anyhow::Result<Option<TrayEvent>> {
        use x11rb::protocol::Event::*;

        let event = match (event, self.ownership) {
            (ClientMessage(event), Ownership::Managed(current_selection_owner))
                if event.type_ == self.atoms._NET_SYSTEM_TRAY_OPCODE =>
            {
                let data = event.data.as_data32();
                let opcode = data[1];
                if opcode == SYSTEM_TRAY_REQUEST_DOCK {
                    let icon = data[2];
                    self.request_dock(icon, current_selection_owner)?;
                } else if opcode == SYSTEM_TRAY_BEGIN_MESSAGE {
                    log::info!("begin message (icon: {})", event.window);
                    let [_, _, timeout, length, id] = event.data.as_data32();
                    let balloon_message =
                        BalloonMessage::new(event.window, id, length as usize, timeout);
                    self.begin_message(balloon_message);
                } else if opcode == SYSTEM_TRAY_CANCEL_MESSAGE {
                    let [_, _, id, ..] = event.data.as_data32();
                    self.cancel_message(id);
                }
                None
            }
            (ClientMessage(event), Ownership::Managed(_))
                if event.type_ == self.atoms._NET_SYSTEM_TRAY_MESSAGE_DATA =>
            {
                if let Some(balloon_message) = self.receive_message(event.window, &event.data) {
                    log::info!(
                        "receive balloon message (icon: {}, timeout: {}, message: {})",
                        balloon_message.icon(),
                        balloon_message.timeout_millis(),
                        balloon_message.as_str()
                    );
                    Some(TrayEvent::MessageReceived(balloon_message))
                } else {
                    None
                }
            }
            (SelectionClear(event), Ownership::Managed(current_selection_owner))
                if event.selection == self.system_tray_selection_atom
                    && event.owner == current_selection_owner =>
            {
                self.clear_embeddings()?;
                Some(TrayEvent::SelectionCleared)
            }
            (PropertyNotify(event), Ownership::Managed(_))
                if event.atom == self.atoms._XEMBED_INFO && self.icons.contains(&event.window) =>
            {
                log::info!("change xembed info (icon: {})", event.window);
                get_xembed_info(event.window, self.connection.as_ref(), &self.atoms)?.map(
                    |xembed_info| {
                        TrayEvent::VisibilityChanged(event.window, xembed_info.is_mapped())
                    },
                )
            }
            (PropertyNotify(event), Ownership::Managed(_))
                if event.atom == self.atoms._NET_WM_NAME && self.icons.contains(&event.window) =>
            {
                log::info!("change window title (icon: {})", event.window);
                let title = get_window_title(event.window, &*self.connection, &self.atoms)?
                    .unwrap_or_default();
                Some(TrayEvent::TitleChanged(event.window, title))
            }
            (ReparentNotify(event), Ownership::Managed(current_selection_owner))
                if event.event == event.window =>
            {
                if event.parent == current_selection_owner {
                    let title = get_window_title(event.window, &*self.connection, &self.atoms)?
                        .unwrap_or_default();
                    let is_embdded =
                        get_xembed_info(event.window, self.connection.as_ref(), &self.atoms)?
                            .map_or(false, |xembed_info| xembed_info.is_mapped());
                    self.icons
                        .contains(&event.window)
                        .then(|| TrayEvent::IconAdded(event.window, title, is_embdded))
                } else {
                    self.quit_dock(event.window)
                        .then(|| TrayEvent::IconRemoved(event.window))
                }
            }
            (
                DestroyNotify(event),
                Ownership::Pending(old_selection_owner, new_selection_owner),
            ) if event.window == old_selection_owner => {
                log::info!(
                    "destroyed previous selection owner (old_selection_owner: {}, new_selection_owner: {})",
                    old_selection_owner,
                    new_selection_owner
                );
                self.broadcast_manager_message(new_selection_owner)?;
                None
            }
            (DestroyNotify(event), Ownership::Managed(_)) => self
                .quit_dock(event.window)
                .then(|| TrayEvent::IconRemoved(event.window)),
            _ => None,
        };

        Ok(event)
    }

    fn begin_message(&mut self, balloon_message: BalloonMessage) {
        log::info!(
            "begin balloon message (icon: {}, id: {})",
            balloon_message.icon(),
            balloon_message.icon()
        );
        self.balloon_messages.push(balloon_message);
    }

    fn broadcast_manager_message(
        &mut self,
        new_selection_owner: xproto::Window,
    ) -> anyhow::Result<()> {
        log::info!("broadcast MANAGER message");

        let screen = &self.connection.setup().roots[self.screen_num];
        let event = xproto::ClientMessageEvent::new(
            32,
            screen.root,
            self.atoms.MANAGER,
            [
                x11rb::CURRENT_TIME,
                self.system_tray_selection_atom,
                new_selection_owner,
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
            .context("broadcast MANAGER message")?;

        self.connection.flush()?;

        self.ownership = Ownership::Managed(new_selection_owner);

        Ok(())
    }

    fn cancel_message(&mut self, id: u32) {
        log::info!("cancel balloon message (id: {})", id);
        self.balloon_messages
            .retain(|balloon_message| balloon_message.id() != id);
    }

    fn clear_embeddings(&mut self) -> anyhow::Result<()> {
        log::info!("clear embeddings");

        for icon in mem::take(&mut self.icons) {
            release_embedding(icon, &*self.connection, self.screen_num)?;
        }

        self.ownership = Ownership::Unmanaged;

        Ok(())
    }

    fn quit_dock(&mut self, icon: xproto::Window) -> bool {
        self.balloon_messages
            .retain(|balloon_message| balloon_message.icon() != icon);

        if let Some(i) = self.icons.iter().position(|i| *i == icon) {
            self.icons.remove(i);
            true
        } else {
            false
        }
    }

    fn receive_message(
        &mut self,
        icon: xproto::Window,
        data: &xproto::ClientMessageData,
    ) -> Option<BalloonMessage> {
        log::info!("receive balloon message (icon: {})", icon);
        if let Some((i, balloon_message)) = self
            .balloon_messages
            .iter_mut()
            .enumerate()
            .find(|(_, balloon_message)| balloon_message.icon() == icon)
        {
            balloon_message.write_message(&data.as_data8());
            if balloon_message.remaining_len() == 0 {
                Some(self.balloon_messages.swap_remove(i))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn request_dock(
        &mut self,
        icon: xproto::Window,
        selection_owner: xproto::Window,
    ) -> anyhow::Result<()> {
        log::info!("request dock (icon: {})", icon);

        if self.icons.contains(&icon) {
            log::warn!("duplicated icon (icon: {})", icon);
        } else {
            if let Some(xembed_info) = get_xembed_info(icon, &*self.connection, &self.atoms)? {
                begin_embedding(
                    icon,
                    selection_owner,
                    xembed_info,
                    &*self.connection,
                    &self.atoms,
                )?;
                self.icons.push(icon);
            }
        }

        Ok(())
    }

    fn wait_for_destroy_selection_owner(
        &mut self,
        current_selection_owner: xproto::Window,
        new_selection_owner: xproto::Window,
    ) -> anyhow::Result<()> {
        log::info!("wait until the current selection selection_owner will be destroyed");

        let values = xproto::ChangeWindowAttributesAux::new()
            .event_mask(Some(xproto::EventMask::STRUCTURE_NOTIFY.into()));

        self.connection
            .change_window_attributes(current_selection_owner, &values)?
            .check()?;

        self.ownership = Ownership::Pending(current_selection_owner, new_selection_owner);

        Ok(())
    }
}

#[derive(Debug)]
pub enum TrayEvent {
    IconAdded(xproto::Window, String, bool),
    IconRemoved(xproto::Window),
    VisibilityChanged(xproto::Window, bool),
    TitleChanged(xproto::Window, String),
    MessageReceived(BalloonMessage),
    SelectionCleared,
}

#[derive(Debug, Clone)]
pub struct BalloonMessage {
    icon: xproto::Window,
    id: u32,
    length: usize,
    timeout_millis: u32,
    buffer: Vec<u8>,
}

impl BalloonMessage {
    pub fn new(icon: xproto::Window, id: u32, length: usize, timeout_millis: u32) -> Self {
        Self {
            icon,
            id,
            length,
            timeout_millis,
            buffer: Vec::with_capacity(length),
        }
    }

    pub fn icon(&self) -> xproto::Window {
        self.icon
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn timeout_millis(&self) -> u32 {
        self.timeout_millis
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Ownership {
    Unmanaged,
    Pending(xproto::Window, xproto::Window),
    Managed(xproto::Window),
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
#[repr(u32)]
enum XEmbedMessage {
    EmbeddedNotify = 0,
    WindowActivate = 1,
    WindowDeactivate = 2,
    RequestFocus = 3,
    FocusIn = 4,
    FocusOut = 5,
    FocusNext = 6,
    FocusPrev = 7,
    GrabKey = 8,
    UngrabKey = 9,
    ModalityOn = 10,
    ModalityOff = 11,
    RegisterAccelerator = 12,
    UnregisterAccelerator = 13,
    ActivateAccelerator = 14,
}

impl From<XEmbedMessage> for u32 {
    fn from(value: XEmbedMessage) -> Self {
        value as u32
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct XEmbedInfo {
    pub version: u32,
    pub flags: u32,
}

impl XEmbedInfo {
    pub fn is_mapped(&self) -> bool {
        self.flags & XEMBED_MAPPED != 0
    }
}

fn intern_system_tray_selection_atom(
    connection: &impl Connection,
    screen_num: usize,
) -> anyhow::Result<xproto::Atom> {
    let atom = connection
        .intern_atom(
            false,
            &format!("_NET_SYSTEM_TRAY_S{}", screen_num).as_bytes(),
        )?
        .reply()
        .context("intern _NET_SYSTEM_TRAY_S{N}")?
        .atom;
    Ok(atom)
}

fn begin_embedding<C: Connection>(
    icon: xproto::Window,
    selection_owner: xproto::Window,
    xembed_info: XEmbedInfo,
    connection: &C,
    atoms: &Atoms,
) -> anyhow::Result<()> {
    log::info!("begin embedding for icon (icon: {})", icon);

    {
        let values = xproto::ChangeWindowAttributesAux::new().event_mask(Some(
            (xproto::EventMask::PROPERTY_CHANGE | xproto::EventMask::STRUCTURE_NOTIFY).into(),
        ));
        connection
            .change_window_attributes(icon, &values)?
            .check()
            .context("set icon event mask")?;
    }

    connection
        .change_save_set(xproto::SetMode::INSERT, icon)?
        .check()
        .context("change icon save set")?;

    connection
        .reparent_window(icon, selection_owner, 0, 0)?
        .check()
        .context("reparent icon window")?;

    let event = xproto::ClientMessageEvent::new(
        32,
        icon,
        atoms._XEMBED,
        [
            x11rb::CURRENT_TIME,
            XEmbedMessage::EmbeddedNotify.into(),
            0, // detail
            selection_owner,
            xembed_info.version,
        ],
    );

    connection
        .send_event(false, icon, xproto::EventMask::STRUCTURE_NOTIFY, event)?
        .check()
        .context("send _XEMBED message")?;

    connection.flush().context("flush")?;

    Ok(())
}

fn release_embedding<C: Connection>(
    icon: xproto::Window,
    connection: &C,
    screen_num: usize,
) -> anyhow::Result<()> {
    log::info!("release embedding for icon (icon: {})", icon);

    let screen = &connection.setup().roots[screen_num];

    {
        let values = xproto::ChangeWindowAttributesAux::new()
            .event_mask(Some(xproto::EventMask::NO_EVENT.into()));
        connection
            .change_window_attributes(icon, &values)?
            .check()
            .context("restore icon event mask")?;
    }

    connection.unmap_window(icon)?.check()?;

    connection
        .reparent_window(icon, screen.root, 0, 0)?
        .check()
        .context("restore icon parent")?;

    connection.flush().context("flush")?;

    Ok(())
}

fn get_window_title(
    window: xproto::Window,
    connection: &impl Connection,
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
        )?
        .reply()
        .context("get _NET_WM_NAME property")?;
    if let Some(title) = reply
        .value8()
        .and_then(|bytes| String::from_utf8(bytes.collect()).ok())
        .filter(|title| !title.is_empty())
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
        )?
        .reply()
        .context("get WM_NAME property")?;
    if let Some(title) = reply
        .value8()
        .and_then(|bytes| String::from_utf8(bytes.collect()).ok())
        .filter(|title| !title.is_empty())
    {
        return Ok(Some(title));
    }

    Ok(None)
}

fn get_xembed_info(
    window: xproto::Window,
    connection: &impl Connection,
    atoms: &Atoms,
) -> anyhow::Result<Option<XEmbedInfo>> {
    let reply = connection
        .get_property(
            false,
            window,
            atoms._XEMBED_INFO,
            xproto::AtomEnum::ANY,
            0,
            2,
        )?
        .reply()
        .context("get _XEMBED_INFO property")?;
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
