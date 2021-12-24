use std::os::raw::*;
use x11::xlib;

pub extern "C" fn ignore_error(
    _display: *mut xlib::Display,
    _error: *mut xlib::XErrorEvent,
) -> c_int {
    0
}

pub extern "C" fn print_error(display: *mut xlib::Display, error: *mut xlib::XErrorEvent) -> c_int {
    unsafe {
        let error_message = x11_get_error_message(display, (*error).error_code as i32);
        let request_message = x11_get_request_description(display, (*error).request_code as i32);
        println!(
            "X11 Error: {} (request: {}, resource: {})",
            error_message,
            request_message,
            (*error).resourceid
        );
    }
    0
}

fn x11_get_error_message(display: *mut xlib::Display, error_code: i32) -> String {
    let mut message = vec![0 as u8; 256];

    unsafe {
        xlib::XGetErrorText(
            display,
            error_code,
            message.as_mut_ptr() as *mut i8,
            message.len() as i32,
        );
    }

    if let Some(null_position) = message.iter().position(|c| *c == 0) {
        message.resize(null_position as usize, 0);
    }

    String::from_utf8(message).ok().unwrap_or_default()
}

fn x11_get_request_description(display: *mut xlib::Display, request_code: i32) -> String {
    let mut message = vec![0 as u8; 256];
    let request_type = format!("{}\0", request_code.to_string());

    unsafe {
        xlib::XGetErrorDatabaseText(
            display,
            "XRequest\0".as_ptr() as *const c_char,
            request_type.as_ptr() as *const c_char,
            "Unknown\0".as_ptr() as *const c_char,
            message.as_mut_ptr() as *mut i8,
            message.len() as i32,
        );
    }

    if let Some(null_position) = message.iter().position(|c| *c == 0) {
        message.resize(null_position as usize, 0);
    }

    String::from_utf8(message).ok().unwrap_or_default()
}
