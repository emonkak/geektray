extern crate x11;

use std::ffi::CStr;
use std::os::raw::*;
use std::str;
use x11::xlib;

fn main() {
    for keysym in 0x0000..=0xffff {
        if let Some(keysym_str) = keysym_to_str(keysym) {
            println!("0x{:<04x}\t{}", keysym, keysym_str);
        }
    }
}

fn keysym_to_str(keysym: c_uint) -> Option<&'static str> {
    unsafe {
        let c = xlib::XKeysymToString(keysym as c_ulong);
        if c.is_null() {
            None
        } else {
            str::from_utf8(CStr::from_ptr(c).to_bytes()).ok()
        }
    }
}
