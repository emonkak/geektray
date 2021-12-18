use x11::xlib;

use crate::utils;

#[allow(non_snake_case)]
#[derive(Debug)]
pub struct Atoms {
    pub NET_SYSTEM_TRAY_MESSAGE_DATA: xlib::Atom,
    pub NET_SYSTEM_TRAY_OPCODE: xlib::Atom,
    pub NET_WM_NAME: xlib::Atom,
    pub NET_WM_PID: xlib::Atom,
    pub WM_DELETE_WINDOW: xlib::Atom,
    pub WM_PROTOCOLS: xlib::Atom,
    pub XEMBED: xlib::Atom,
    pub XEMBED_INFO: xlib::Atom,
}

impl Atoms {
    pub fn new(display: *mut xlib::Display) -> Self {
        unsafe {
            Self {
                NET_SYSTEM_TRAY_MESSAGE_DATA: utils::new_atom(
                    display,
                    "_NET_SYSTEM_TRAY_MESSAGE_DATA\0",
                ),
                NET_SYSTEM_TRAY_OPCODE: utils::new_atom(display, "_NET_SYSTEM_TRAY_OPCODE\0"),
                NET_WM_NAME: utils::new_atom(display, "_NET_WM_NAME\0"),
                NET_WM_PID: utils::new_atom(display, "_NET_WM_PID\0"),
                WM_DELETE_WINDOW: utils::new_atom(display, "WM_DELETE_WINDOW\0"),
                WM_PROTOCOLS: utils::new_atom(display, "WM_PROTOCOLS\0"),
                XEMBED: utils::new_atom(display, "_XEMBED\0"),
                XEMBED_INFO: utils::new_atom(display, "_XEMBED_INFO\0"),
            }
        }
    }
}
