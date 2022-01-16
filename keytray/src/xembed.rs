const XEMBED_MAPPED: u32 = 1 << 0;

#[allow(dead_code)]
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

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct XEmbedInfo {
    pub version: u32,
    pub flags: u32,
}

impl XEmbedInfo {
    pub fn is_mapped(&self) -> bool {
        self.flags & XEMBED_MAPPED != 0
    }
}
