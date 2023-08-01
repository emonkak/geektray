use anyhow::Context as _;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, ConnectionExt};

use crate::atoms::Atoms;

const XEMBED_MAPPED: u32 = 1 << 0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XEmbedInfo {
    version: u32,
    flags: u32,
}

impl XEmbedInfo {
    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn is_mapped(&self) -> bool {
        self.flags & XEMBED_MAPPED != 0
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum XEmbedMessage {
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

pub fn get_xembed_info(
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
