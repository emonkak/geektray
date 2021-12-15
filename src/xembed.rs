const XEMBED_MAPPED: u64 = 1 << 0;

#[derive(Debug)]
#[allow(dead_code)]
pub enum XEmbedMessage {
    EmbeddedNotify        = 0,
    WindowActivate        = 1,
    WindowDeactivate      = 2,
    RequestFocus          = 3,
    FocusIn               = 4,
    FocusOut              = 5,
    FocusNext             = 6,
    FocusPrev             = 7,
    GrabKey               = 8,
    UngrabKey             = 9,
    ModalityOn            = 10,
    ModalityOff           = 11,
    RegisterAccelerator   = 12,
    UnregisterAccelerator = 13,
    ActivateAccelerator   = 14,
}

#[repr(C)]
#[derive(Debug)]
pub struct XEmbedInfo {
    pub version: u64,
    pub flags: u64,
}

impl XEmbedInfo {
    pub fn is_mapped(&self) -> bool {
        self.flags & XEMBED_MAPPED != 0
    }
}
