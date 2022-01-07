use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt;

#[allow(non_snake_case)]
#[derive(Debug)]
pub struct Atoms {
    pub MANAGER: xproto::Atom,
    pub UTF8_STRING: xproto::Atom,
    pub WM_DELETE_WINDOW: xproto::Atom,
    pub WM_PROTOCOLS: xproto::Atom,
    pub _NET_SYSTEM_TRAY_S: xproto::Atom,
    pub _NET_SYSTEM_TRAY_MESSAGE_DATA: xproto::Atom,
    pub _NET_SYSTEM_TRAY_OPCODE: xproto::Atom,
    pub _NET_SYSTEM_TRAY_ORIENTATION: xproto::Atom,
    pub _NET_SYSTEM_TRAY_VISUAL: xproto::Atom,
    pub _NET_WM_NAME: xproto::Atom,
    pub _NET_WM_PID: xproto::Atom,
    pub _NET_WM_PING: xproto::Atom,
    pub _NET_WM_STATE: xproto::Atom,
    pub _NET_WM_STATE_STICKY: xproto::Atom,
    pub _NET_WM_SYNC_REQUEST: xproto::Atom,
    pub _NET_WM_WINDOW_TYPE: xproto::Atom,
    pub _NET_WM_WINDOW_TYPE_DIALOG: xproto::Atom,
    pub _XEMBED: xproto::Atom,
    pub _XEMBED_INFO: xproto::Atom,
}

impl Atoms {
    pub fn new<Connection: self::Connection>(
        connection: &Connection,
        screen_num: usize,
    ) -> Result<Self, ReplyError> {
        Ok(Self {
            MANAGER: new_atom(connection, "MANAGER")?,
            UTF8_STRING: new_atom(connection, "UTF8_STRING")?,
            WM_DELETE_WINDOW: new_atom(connection, "WM_DELETE_WINDOW")?,
            WM_PROTOCOLS: new_atom(connection, "WM_PROTOCOLS")?,
            _NET_SYSTEM_TRAY_S: new_atom(connection, &format!("_NET_SYSTEM_TRAY_S{}", screen_num))?,
            _NET_SYSTEM_TRAY_MESSAGE_DATA: new_atom(connection, "_NET_SYSTEM_TRAY_MESSAGE_DATA")?,
            _NET_SYSTEM_TRAY_OPCODE: new_atom(connection, "_NET_SYSTEM_TRAY_OPCODE")?,
            _NET_SYSTEM_TRAY_ORIENTATION: new_atom(connection, "_NET_SYSTEM_TRAY_ORIENTATION")?,
            _NET_SYSTEM_TRAY_VISUAL: new_atom(connection, "_NET_SYSTEM_TRAY_VISUAL")?,
            _NET_WM_NAME: new_atom(connection, "_NET_WM_NAME")?,
            _NET_WM_PID: new_atom(connection, "_NET_WM_PID")?,
            _NET_WM_STATE_STICKY: new_atom(connection, "_NET_WM_STATE_STICKY")?,
            _NET_WM_PING: new_atom(connection, "_NET_WM_PING")?,
            _NET_WM_STATE: new_atom(connection, "_NET_WM_STATE")?,
            _NET_WM_SYNC_REQUEST: new_atom(connection, "_NET_WM_SYNC_REQUEST")?,
            _NET_WM_WINDOW_TYPE: new_atom(connection, "_NET_WM_WINDOW_TYPE")?,
            _NET_WM_WINDOW_TYPE_DIALOG: new_atom(connection, "_NET_WM_WINDOW_TYPE_DIALOG")?,
            _XEMBED: new_atom(connection, "_XEMBED")?,
            _XEMBED_INFO: new_atom(connection, "_XEMBED_INFO")?,
        })
    }
}

#[inline]
fn new_atom<Connection: self::Connection>(
    connection: &Connection,
    name: &str,
) -> Result<xproto::Atom, ReplyError> {
    let reply = connection.intern_atom(false, name.as_bytes())?.reply()?;
    Ok(reply.atom)
}
