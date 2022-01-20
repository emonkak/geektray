use keytray_shell::geometrics::Size;
use keytray_shell::graphics::Color;
use std::collections::hash_map;
use std::collections::HashMap;
use std::mem;
use std::rc::Rc;
use std::str;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::errors::{ReplyError, ReplyOrIdError};
use x11rb::protocol;
use x11rb::protocol::composite;
use x11rb::protocol::composite::ConnectionExt as _;
use x11rb::protocol::damage;
use x11rb::protocol::damage::ConnectionExt as _;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::wrapper::ConnectionExt as _;

use crate::xembed::{XEmbedInfo, XEmbedMessage};

const SYSTEM_TRAY_REQUEST_DOCK: u32 = 0;
const SYSTEM_TRAY_BEGIN_MESSAGE: u32 = 1;
const SYSTEM_TRAY_CANCEL_MESSAGE: u32 = 2;

#[derive(Debug)]
pub struct TrayManager<C: Connection> {
    connection: Rc<C>,
    screen_num: usize,
    container_window: xproto::Window,
    atoms: Atoms,
    icon_size: Size,
    status: TrayStatus,
    system_tray_selection_atom: xproto::Atom,
    embedded_icons: HashMap<xproto::Window, TrayIcon>,
    balloon_messages: HashMap<xproto::Window, BalloonMessage>,
}

impl<C: Connection> TrayManager<C> {
    pub fn new(
        connection: Rc<C>,
        screen_num: usize,
        orientation: SystemTrayOrientation,
        colors: SystemTrayColors,
        icon_size: Size,
    ) -> Result<Self, ReplyOrIdError> {
        let container_window = connection.generate_id()?;
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

            let visual = screen
                .allowed_depths
                .iter()
                .filter(|depth| depth.depth == 32)
                .flat_map(|depth| depth.visuals.iter())
                .find(|visual| visual.class == xproto::VisualClass::TRUE_COLOR)
                .unwrap();

            let colormap = connection.generate_id()?;
            connection
                .create_colormap(
                    xproto::ColormapAlloc::NONE,
                    colormap,
                    screen.root,
                    visual.visual_id,
                )?
                .check()?;

            let values = xproto::CreateWindowAux::new()
                .event_mask(xproto::EventMask::PROPERTY_CHANGE)
                .colormap(colormap)
                .border_pixel(screen.black_pixel)
                .background_pixel(screen.black_pixel)
                .override_redirect(1);

            connection
                .create_window(
                    32,
                    container_window,
                    screen.root,
                    0,
                    0,
                    icon_size.width as u16,
                    icon_size.height as u16,
                    0,
                    xproto::WindowClass::INPUT_OUTPUT,
                    visual.visual_id,
                    &values,
                )?
                .check()?;

            connection
                .change_property32(
                    xproto::PropMode::REPLACE,
                    container_window,
                    atoms._NET_SYSTEM_TRAY_ORIENTATION,
                    xproto::AtomEnum::CARDINAL,
                    &[orientation.0],
                )?
                .check()?;

            connection
                .change_property32(
                    xproto::PropMode::REPLACE,
                    container_window,
                    atoms._NET_SYSTEM_TRAY_VISUAL,
                    xproto::AtomEnum::VISUALID,
                    &[visual.visual_id],
                )?
                .check()?;

            connection
                .change_property(
                    xproto::PropMode::REPLACE,
                    container_window,
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
            container_window,
            atoms,
            icon_size,
            status: TrayStatus::Unmanaged,
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

        self.connection.map_window(self.container_window)?.check()?;

        self.connection
            .set_selection_owner(
                self.container_window,
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
        event: &protocol::Event,
    ) -> Result<Option<TrayEvent>, ReplyOrIdError> {
        use protocol::Event::*;

        let response = match event {
            DamageNotify(event) => self
                .embedded_icons
                .get(&event.drawable)
                .map(|icon| TrayEvent::TrayIconUpdated(icon.clone())),
            ClientMessage(event) if event.type_ == self.atoms._NET_SYSTEM_TRAY_OPCODE => {
                let data = event.data.as_data32();
                let opcode = data[1];
                if opcode == SYSTEM_TRAY_REQUEST_DOCK {
                    let icon_window = data[2];
                    self.register_tray_icon(icon_window)?;
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
                    && event.owner == self.container_window =>
            {
                for (_, icon) in mem::take(&mut self.embedded_icons) {
                    if icon.xembed_info.is_mapped() {
                        icon.release_embedding(self.connection.as_ref(), self.screen_num)?;
                    }
                }
                self.status = TrayStatus::Unmanaged;
                Some(TrayEvent::SelectionCleared)
            }
            PropertyNotify(event) if event.atom == self.atoms._XEMBED_INFO => {
                self.register_tray_icon(event.window)?;
                None
            }
            PropertyNotify(event)
                if event.atom == u32::from(xproto::AtomEnum::WM_NAME)
                    || event.atom == self.atoms._NET_WM_NAME =>
            {
                if let Some(icon) = self.embedded_icons.get_mut(&event.window) {
                    let title =
                        get_window_title(self.connection.as_ref(), event.window, &self.atoms)?
                            .unwrap_or_default();
                    icon.title = title;
                    Some(TrayEvent::TrayIconUpdated(icon.clone()))
                } else {
                    None
                }
            }
            // Ignore from SUBSTRUCTURE_NOTIFY.
            ReparentNotify(event) if event.event == event.window => {
                if event.parent == self.container_window {
                    self.embedded_icons
                        .get(&event.window)
                        .map(|icon| TrayEvent::TrayIconAdded(icon.clone()))
                } else {
                    self.unregister_tray_icon(event.window)?
                        .map(|icon| TrayEvent::TrayIconRemoved(icon))
                }
            }
            DestroyNotify(event) => match self.status {
                TrayStatus::Pending(window) if event.window == window => {
                    self.broadcast_manager_message()?;
                    None
                }
                _ => self
                    .unregister_tray_icon(event.window)?
                    .map(|icon| TrayEvent::TrayIconRemoved(icon)),
            },
            _ => None,
        };

        Ok(response)
    }

    fn release_tray_selection(&mut self) -> Result<(), ReplyError> {
        if matches!(self.status, TrayStatus::Managed) {
            for (_, icon) in mem::take(&mut self.embedded_icons) {
                if icon.xembed_info.is_mapped() {
                    icon.release_embedding(self.connection.as_ref(), self.screen_num)?;
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

    fn register_tray_icon(&mut self, icon_window: xproto::Window) -> Result<(), ReplyOrIdError> {
        if let Some(xembed_info) = self.get_xembed_info(icon_window)? {
            match self.embedded_icons.entry(icon_window) {
                hash_map::Entry::Occupied(mut entry) => {
                    let old_xembed_info =
                        mem::replace(&mut entry.get_mut().xembed_info, xembed_info);
                    match (old_xembed_info.is_mapped(), xembed_info.is_mapped()) {
                        (false, true) => {
                            entry.get().begin_embedding(self.connection.as_ref())?;
                        }
                        (true, false) => {
                            entry
                                .get()
                                .release_embedding(self.connection.as_ref(), self.screen_num)?;
                        }
                        _ => {}
                    }
                }
                hash_map::Entry::Vacant(entry) => {
                    let damage = self.connection.generate_id()?;
                    self.connection
                        .damage_create(damage, icon_window, damage::ReportLevel::NON_EMPTY)?
                        .check()?;
                    let title =
                        get_window_title(self.connection.as_ref(), icon_window, &self.atoms)?
                            .unwrap_or_default();
                    let icon = TrayIcon {
                        window: icon_window,
                        container_window: self.container_window,
                        damage,
                        title,
                        size: self.icon_size,
                        xembed_info,
                    };
                    icon.send_embedded_notify(self.connection.as_ref(), &self.atoms)?;
                    if xembed_info.is_mapped() {
                        icon.begin_embedding(self.connection.as_ref())?;
                    } else {
                        icon.wait_for_embedding(self.connection.as_ref())?;
                    };
                    entry.insert(icon);
                }
            }
        }

        Ok(())
    }

    fn unregister_tray_icon(
        &mut self,
        icon_window: xproto::Window,
    ) -> Result<Option<TrayIcon>, ReplyError> {
        self.balloon_messages.remove(&icon_window);
        if let Some(icon) = self.embedded_icons.remove(&icon_window) {
            self.connection.damage_destroy(icon.damage)?.check()?;
            Ok(Some(icon))
        } else {
            Ok(None)
        }
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
                self.container_window,
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

    fn get_xembed_info(&self, target: xproto::Window) -> Result<Option<XEmbedInfo>, ReplyError> {
        let reply = self
            .connection
            .get_property(
                false,
                target,
                self.atoms._XEMBED_INFO,
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
}

impl<C: Connection> Drop for TrayManager<C> {
    fn drop(&mut self) {
        self.release_tray_selection().ok();
        self.connection.destroy_window(self.container_window).ok();
    }
}

#[derive(Debug, Clone)]
pub struct TrayIcon {
    window: xproto::Window,
    container_window: xproto::Window,
    damage: damage::Damage,
    title: String,
    xembed_info: XEmbedInfo,
    size: Size,
}

impl TrayIcon {
    pub fn window(&self) -> xproto::Window {
        self.window
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn click<C: Connection>(
        &self,
        connection: &C,
        screen_num: usize,
        button: xproto::ButtonIndex,
        button_mask: xproto::ButtonMask,
    ) -> Result<(), ReplyError> {
        let screen = &connection.setup().roots[screen_num];
        let pointer = connection.query_pointer(screen.root)?.reply()?;

        let saved_position = connection
            .translate_coordinates(screen.root, self.container_window, 0, 0)?
            .reply()?;

        {
            let values = xproto::ConfigureWindowAux::new()
                .x(pointer.root_x as i32)
                .y(pointer.root_y as i32)
                .stack_mode(xproto::StackMode::ABOVE);
            connection
                .configure_window(self.container_window, &values)?
                .check()?;

            let values = xproto::ConfigureWindowAux::new().stack_mode(xproto::StackMode::ABOVE);
            connection.configure_window(self.window, &values)?.check()?;
        }

        send_button_event(
            connection,
            screen_num,
            self.window,
            button,
            button_mask,
            true,
            0,
            0,
            pointer.root_x,
            pointer.root_y,
        )?;

        send_button_event(
            connection,
            screen_num,
            self.window,
            button,
            button_mask,
            false,
            0,
            0,
            pointer.root_x,
            pointer.root_y,
        )?;

        {
            let values = xproto::ConfigureWindowAux::new()
                .x(saved_position.dst_x as i32)
                .y(saved_position.dst_y as i32)
                .stack_mode(xproto::StackMode::BELOW);
            connection.configure_window(self.container_window, &values)?.check()?;
        }

        connection.flush()?;

        Ok(())
    }

    fn send_embedded_notify<C: Connection>(
        &self,
        connection: &C,
        atoms: &Atoms,
    ) -> Result<(), ReplyError> {
        let event = xproto::ClientMessageEvent::new(
            32,
            self.window,
            atoms._XEMBED,
            [
                x11rb::CURRENT_TIME,
                XEmbedMessage::EmbeddedNotify.into(),
                self.container_window,
                self.xembed_info.version,
                0,
            ],
        );

        connection
            .send_event(
                false,
                self.window,
                xproto::EventMask::STRUCTURE_NOTIFY,
                event,
            )?
            .check()
    }

    fn begin_embedding<C: Connection>(&self, connection: &C) -> Result<(), ReplyOrIdError> {
        {
            let values = xproto::ChangeWindowAttributesAux::new().event_mask(Some(
                (xproto::EventMask::PROPERTY_CHANGE | xproto::EventMask::STRUCTURE_NOTIFY).into(),
            ));
            connection
                .change_window_attributes(self.window, &values)?
                .check()?;
        }

        connection
            .composite_redirect_window(self.window, composite::Redirect::MANUAL)?
            .check()?;

        connection
            .change_save_set(xproto::SetMode::INSERT, self.window)?
            .check()?;

        connection
            .reparent_window(self.window, self.container_window, 0, 0)?
            .check()?;

        {
            let values = xproto::ConfigureWindowAux::new()
                .width(self.size.width as u32)
                .height(self.size.height as u32)
                .stack_mode(xproto::StackMode::ABOVE);
            connection.configure_window(self.window, &values)?.check()?;
        }

        connection.map_window(self.window)?.check()?;

        connection.flush()?;

        Ok(())
    }

    fn wait_for_embedding<C: Connection>(&self, connection: &C) -> Result<(), ReplyError> {
        {
            let values = xproto::ChangeWindowAttributesAux::new()
                .event_mask(xproto::EventMask::PROPERTY_CHANGE);
            connection
                .change_window_attributes(self.window, &values)?
                .check()?;
        }

        connection.flush()?;

        Ok(())
    }

    fn release_embedding<C: Connection>(
        &self,
        connection: &C,
        screen_num: usize,
    ) -> Result<(), ReplyError> {
        let screen = &connection.setup().roots[screen_num];

        {
            let values = xproto::ChangeWindowAttributesAux::new()
                .event_mask(Some(xproto::EventMask::NO_EVENT.into()));
            connection
                .change_window_attributes(self.window, &values)?
                .check()?;
        }

        connection
            .reparent_window(self.window, screen.root, 0, 0)?
            .check()?;

        connection.unmap_window(self.window)?.check()?;

        connection.flush()?;

        Ok(())
    }
}

fn send_button_event<Connection: self::Connection>(
    connection: &Connection,
    screen_num: usize,
    window: xproto::Window,
    button: xproto::ButtonIndex,
    button_mask: xproto::ButtonMask,
    is_pressed: bool,
    x: i16,
    y: i16,
    root_x: i16,
    root_y: i16,
) -> Result<(), ReplyError> {
    let screen = &connection.setup().roots[screen_num];

    let event = xproto::ButtonPressEvent {
        response_type: if is_pressed {
            xproto::BUTTON_PRESS_EVENT
        } else {
            xproto::BUTTON_RELEASE_EVENT
        },
        detail: button.into(),
        sequence: 0,
        time: x11rb::CURRENT_TIME,
        root: screen.root,
        event: window,
        child: x11rb::NONE,
        event_x: x,
        event_y: y,
        root_x,
        root_y,
        state: button_mask.into(),
        same_screen: true,
    };

    connection
        .send_event(true, window, xproto::EventMask::NO_EVENT, event)?
        .check()?;

    Ok(())
}

#[derive(Debug)]
pub enum TrayEvent {
    TrayIconAdded(TrayIcon),
    TrayIconUpdated(TrayIcon),
    TrayIconRemoved(TrayIcon),
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

fn get_window_title<C: Connection>(
    connection: &C,
    window: xproto::Window,
    atoms: &Atoms,
) -> Result<Option<String>, ReplyError> {
    let reply = connection
        .get_property(
            false,
            window,
            atoms._NET_WM_NAME,
            atoms.UTF8_STRING,
            0,
            256 / 4,
        )?
        .reply()?;
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
        .reply()?;
    if let Some(title) = reply
        .value8()
        .and_then(|bytes| String::from_utf8(bytes.collect()).ok())
        .filter(|title| !title.is_empty())
    {
        return Ok(Some(title));
    }

    Ok(None)
}

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        MANAGER,
        UTF8_STRING,
        _NET_SYSTEM_TRAY_COLORS,
        _NET_SYSTEM_TRAY_MESSAGE_DATA,
        _NET_SYSTEM_TRAY_OPCODE,
        _NET_SYSTEM_TRAY_ORIENTATION,
        _NET_SYSTEM_TRAY_VISUAL,
        _NET_WM_NAME,
        _XEMBED,
        _XEMBED_INFO,
    }
}
