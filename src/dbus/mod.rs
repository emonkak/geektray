mod connection;
mod message;
mod types;

pub mod reader;
pub mod writer;

pub use connection::Connection;
pub use message::Message;
pub use types::{Argument, DictEntry, Signature, SignatureParseError, Variant};

use std::ffi::CStr;
use std::os::raw::*;
use std::str;

unsafe fn c_str_to_slice<'a>(c: *const c_char) -> Option<&'a str> {
    if c.is_null() {
        None
    } else {
        str::from_utf8(CStr::from_ptr(c).to_bytes()).ok()
    }
}
