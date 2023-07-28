use anyhow::Context as _;
use std::rc::Rc;
use std::str;
use x11rb::connection::Connection;
use x11rb::protocol;
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::wrapper::ConnectionExt as _;

use crate::atoms::Atoms;
use crate::color::Color;

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
    selection_status: SelectionStatus,
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
            selection_status: SelectionStatus::Unmanaged,
            icons: Vec::new(),
            balloon_messages: Vec::new(),
        })
    }

    pub fn acquire_tray_selection(
        &mut self,
        embedder: xproto::Window,
        orientation: SystemTrayOrientation,
        colors: SystemTrayColors,
    ) -> anyhow::Result<()> {
        let new_manager = self.create_manager_window(embedder, orientation, colors)?;

        self.connection
            .set_selection_owner(
                new_manager,
                self.system_tray_selection_atom,
                x11rb::CURRENT_TIME,
            )?
            .check()
            .context("acquire tray selection")?;

        self.update_selection_status(new_manager, embedder)
    }

    pub fn release_tray_selection(&mut self) -> anyhow::Result<()> {
        match self.selection_status {
            SelectionStatus::Managed { manager, .. } => {
                log::info!("release tray selection (manager: {})", manager);

                self.connection
                    .set_selection_owner(
                        x11rb::NONE,
                        self.system_tray_selection_atom,
                        x11rb::CURRENT_TIME,
                    )?
                    .check()
                    .context("reset tray selection")?;

                self.connection
                    .destroy_window(manager)?
                    .check()
                    .context("destory manager window")?;

                self.clear_embeddings()?;
            }
            _ => {}
        }

        self.selection_status = SelectionStatus::Unmanaged;

        Ok(())
    }

    pub fn translate_event(
        &mut self,
        event: &protocol::Event,
    ) -> anyhow::Result<Option<TrayEvent>> {
        use x11rb::protocol::Event::*;

        let event = match (event, self.selection_status) {
            (ClientMessage(event), SelectionStatus::Managed { embedder, .. })
                if event.type_ == self.atoms._NET_SYSTEM_TRAY_OPCODE =>
            {
                let data = event.data.as_data32();
                let opcode = data[1];
                if opcode == SYSTEM_TRAY_REQUEST_DOCK {
                    let icon = data[2];
                    self.request_dock(icon, embedder)?;
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
            (ClientMessage(event), SelectionStatus::Managed { .. })
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
            (SelectionClear(event), SelectionStatus::Managed { manager, .. })
                if event.selection == self.system_tray_selection_atom && event.owner == manager =>
            {
                self.clear_embeddings()?;
                Some(TrayEvent::SelectionCleared)
            }
            (PropertyNotify(event), SelectionStatus::Managed { .. })
                if event.atom == self.atoms._XEMBED_INFO && self.icons.contains(&event.window) =>
            {
                log::info!("change xembed info (icon: {})", event.window);
                let is_embdded = get_xembed_info(&*self.connection, &self.atoms, event.window)?
                    .map_or(false, |xembed_info| xembed_info.is_mapped());
                Some(TrayEvent::VisibilityChanged(event.window, is_embdded))
            }
            (PropertyNotify(event), SelectionStatus::Managed { .. })
                if event.atom == self.atoms._NET_WM_NAME && self.icons.contains(&event.window) =>
            {
                log::info!("change window title (icon: {})", event.window);
                let title = get_window_title(&*self.connection, &self.atoms, event.window)?
                    .unwrap_or_default();
                Some(TrayEvent::TitleChanged(event.window, title))
            }
            (ReparentNotify(event), SelectionStatus::Managed { embedder, .. })
                if event.event == event.window =>
            {
                if event.parent == embedder {
                    if self.icons.contains(&event.window) {
                        let title = get_window_title(&*self.connection, &self.atoms, event.window)?
                            .unwrap_or_default();
                        let is_embdded =
                            get_xembed_info(&*self.connection, &self.atoms, event.window)?
                                .map_or(false, |xembed_info| xembed_info.is_mapped());
                        Some(TrayEvent::IconAdded(event.window, title, is_embdded))
                    } else {
                        None
                    }
                } else {
                    self.quit_dock(event.window)
                        .then(|| TrayEvent::IconRemoved(event.window))
                }
            }
            (
                DestroyNotify(event),
                SelectionStatus::Pending {
                    old_manager,
                    new_manager,
                    embedder,
                },
            ) if event.window == old_manager => {
                log::info!(
                    "destroyed previous selection owner (old_manager: {}, new_manager: {})",
                    old_manager,
                    new_manager
                );
                self.update_selection_status(new_manager, embedder)?;
                None
            }
            (DestroyNotify(event), SelectionStatus::Managed { .. }) => self
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

    fn broadcast_manager_message(&mut self, new_manager: xproto::Window) -> anyhow::Result<()> {
        log::info!("broadcast MANAGER message");

        let screen = &self.connection.setup().roots[self.screen_num];
        let event = xproto::ClientMessageEvent::new(
            32,
            screen.root,
            self.atoms.MANAGER,
            [
                x11rb::CURRENT_TIME,
                self.system_tray_selection_atom,
                new_manager,
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

        Ok(())
    }

    fn cancel_message(&mut self, id: u32) {
        log::info!("cancel balloon message (id: {})", id);
        self.balloon_messages
            .retain(|balloon_message| balloon_message.id() != id);
    }

    fn clear_embeddings(&mut self) -> anyhow::Result<()> {
        log::info!("clear embeddings");

        self.balloon_messages.clear();

        for icon in self.icons.drain(..) {
            release_embedding(&*self.connection, self.screen_num, icon)?;
        }

        self.connection
            .flush()
            .context("flush after release embeddings")?;

        self.selection_status = SelectionStatus::Unmanaged;

        Ok(())
    }

    fn create_manager_window(
        &self,
        embedder: xproto::Window,
        orientation: SystemTrayOrientation,
        colors: SystemTrayColors,
    ) -> anyhow::Result<xproto::Window> {
        let window = self.connection.generate_id()?;
        let screen = &self.connection.setup().roots[self.screen_num];
        let values = xproto::CreateWindowAux::new().event_mask(xproto::EventMask::PROPERTY_CHANGE);

        self.connection
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
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

        let embedder_attributes = self
            .connection
            .get_window_attributes(embedder)?
            .reply()
            .context("get embedder attributes")?;

        self.connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                self.atoms._NET_SYSTEM_TRAY_VISUAL,
                xproto::AtomEnum::VISUALID,
                &[embedder_attributes.visual],
            )?
            .check()?;

        self.connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                self.atoms._NET_SYSTEM_TRAY_ORIENTATION,
                xproto::AtomEnum::CARDINAL,
                &[orientation.0],
            )?
            .check()?;

        self.connection
            .change_property(
                xproto::PropMode::REPLACE,
                window,
                self.atoms._NET_SYSTEM_TRAY_COLORS,
                xproto::AtomEnum::CARDINAL,
                32,
                12,
                colors.as_bytes(),
            )?
            .check()?;

        Ok(window)
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
        embedder: xproto::Window,
    ) -> anyhow::Result<()> {
        log::info!("request dock (icon: {})", icon);

        if self.icons.contains(&icon) {
            log::warn!("duplicated icon (icon: {})", icon);
        } else {
            if let Some(xembed_info) = get_xembed_info(&*self.connection, &self.atoms, icon)? {
                begin_embedding(&*self.connection, &self.atoms, icon, embedder, xembed_info)?;
                self.connection
                    .flush()
                    .context("flush after begin embedding")?;
                self.icons.push(icon);
            }
        }

        Ok(())
    }

    fn update_selection_status(
        &mut self,
        new_manager: xproto::Window,
        embedder: xproto::Window,
    ) -> anyhow::Result<()> {
        log::info!("set selection owner (manager: {})", new_manager);

        let current_manager = self
            .connection
            .get_selection_owner(self.system_tray_selection_atom)?
            .reply()
            .context("get selection owner")?
            .owner;

        if current_manager == new_manager {
            self.broadcast_manager_message(new_manager)?;
            self.selection_status = SelectionStatus::Managed {
                manager: new_manager,
                embedder,
            };
        } else if current_manager != x11rb::NONE {
            self.wait_for_destroy_selection_owner(current_manager)?;
            self.selection_status = SelectionStatus::Pending {
                old_manager: current_manager,
                new_manager,
                embedder,
            };
        } else {
            self.selection_status = SelectionStatus::Unmanaged;
        }

        Ok(())
    }

    fn wait_for_destroy_selection_owner(
        &mut self,
        current_manager: xproto::Window,
    ) -> anyhow::Result<()> {
        log::info!("wait until the current selection selection_owner will be destroyed");

        let values = xproto::ChangeWindowAttributesAux::new()
            .event_mask(Some(xproto::EventMask::STRUCTURE_NOTIFY.into()));

        self.connection
            .change_window_attributes(current_manager, &values)?
            .check()?;

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
    timeout_millis: u32,
    buffer: Vec<u8>,
}

impl BalloonMessage {
    pub fn new(icon: xproto::Window, id: u32, length: usize, timeout_millis: u32) -> Self {
        Self {
            icon,
            id,
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
        self.buffer.capacity().saturating_sub(self.buffer.len())
    }

    fn write_message(&mut self, bytes: &[u8]) {
        let incoming_len = self.remaining_len().min(20);
        if incoming_len > 0 {
            self.buffer.extend_from_slice(&bytes[..incoming_len]);
        }
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
    pub const fn new(normal: Color, success: Color, warning: Color, error: Color) -> Self {
        let normal_components = normal.to_u16_components();
        let success_components = success.to_u16_components();
        let warning_components = warning.to_u16_components();
        let error_components = error.to_u16_components();
        Self {
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

    pub const fn single(color: Color) -> Self {
        Self::new(color, color, color, color)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectionStatus {
    Unmanaged,
    Pending {
        old_manager: xproto::Window,
        new_manager: xproto::Window,
        embedder: xproto::Window,
    },
    Managed {
        manager: xproto::Window,
        embedder: xproto::Window,
    },
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

fn begin_embedding(
    connection: &impl Connection,
    atoms: &Atoms,
    icon: xproto::Window,
    embedder: xproto::Window,
    xembed_info: XEmbedInfo,
) -> anyhow::Result<()> {
    log::info!("begin embedding for icon (icon: {})", icon);

    let values = xproto::ChangeWindowAttributesAux::new().event_mask(Some(
        (xproto::EventMask::PROPERTY_CHANGE | xproto::EventMask::STRUCTURE_NOTIFY).into(),
    ));

    connection
        .change_window_attributes(icon, &values)?
        .check()
        .context("set icon event mask")?;

    connection
        .change_save_set(xproto::SetMode::INSERT, icon)?
        .check()
        .context("change icon save set")?;

    connection
        .reparent_window(icon, embedder, 0, 0)?
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
            embedder,
            xembed_info.version,
        ],
    );

    connection
        .send_event(false, icon, xproto::EventMask::STRUCTURE_NOTIFY, event)?
        .check()
        .context("send _XEMBED message")?;

    Ok(())
}

fn get_window_title(
    connection: &impl Connection,
    atoms: &Atoms,
    window: xproto::Window,
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
    connection: &impl Connection,
    atoms: &Atoms,
    window: xproto::Window,
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

fn release_embedding(
    connection: &impl Connection,
    screen_num: usize,
    icon: xproto::Window,
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

    Ok(())
}
