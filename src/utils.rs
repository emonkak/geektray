use std::fs;
use std::io;
use std::mem;
use std::os::raw::*;
use std::ptr;
use x11::xlib;

#[inline]
pub unsafe fn new_atom(display: *mut xlib::Display, null_terminated_name: &str) -> xlib::Atom {
    assert!(null_terminated_name
        .chars()
        .last()
        .map_or(false, |c| c == '\0'));
    xlib::XInternAtom(
        display,
        null_terminated_name.as_ptr() as *const c_char,
        xlib::False,
    )
}

#[inline]
pub unsafe fn send_client_message(
    display: *mut xlib::Display,
    destination: xlib::Window,
    window: xlib::Window,
    message_type: xlib::Atom,
    data: xlib::ClientMessageData,
) -> bool {
    let mut client_message_event = xlib::XEvent::from(xlib::XClientMessageEvent {
        type_: xlib::ClientMessage,
        serial: 0,
        send_event: xlib::True,
        display,
        window,
        message_type,
        format: 32,
        data,
    });

    xlib::XSendEvent(
        display,
        destination,
        xlib::False,
        0xffffff,
        &mut client_message_event,
    ) == xlib::True
}

#[inline]
pub unsafe fn get_window_property<T: Sized, const N: usize>(
    display: *mut xlib::Display,
    window: xlib::Window,
    property_atom: xlib::Atom,
) -> Option<Box<[T; N]>> {
    let mut actual_type: xlib::Atom = 0;
    let mut actual_format: i32 = 0;
    let mut nitems: u64 = 0;
    let mut bytes_after: u64 = 0;
    let mut prop: *mut u8 = ptr::null_mut();

    let result = xlib::XGetWindowProperty(
        display,
        window,
        property_atom,
        0,
        N as c_long,
        xlib::False,
        xlib::AnyPropertyType as u64,
        &mut actual_type,
        &mut actual_format,
        &mut nitems,
        &mut bytes_after,
        &mut prop,
    );

    let expected_format = match mem::size_of::<T>() {
        8 | 4 => 32,
        2 => 16,
        1 => 8,
        _ => 0,
    };

    if result != xlib::Success.into()
        || actual_format != expected_format
        || nitems != N as c_ulong
        || prop.is_null()
    {
        return None;
    }

    Some(Box::from_raw(prop.cast()))
}

#[inline]
pub fn get_process_name(pid: u32) -> io::Result<String> {
    let path = format!("/proc/{}/cmdline", pid);
    let bytes = fs::read(path)?;
    let null_position = bytes.iter().position(|byte| *byte == 0);
    let name = String::from_utf8_lossy(&bytes[0..null_position.unwrap_or(0)]).into_owned();
    Ok(name)
}

#[inline]
pub unsafe fn get_pointer_position(
    display: *mut xlib::Display,
    window: xlib::Window,
) -> (c_int, c_int) {
    let mut root = 0;
    let mut child = 0;
    let mut root_x = 0;
    let mut root_y = 0;
    let mut x = 0;
    let mut y = 0;
    let mut state = 0;

    xlib::XQueryPointer(
        display,
        window,
        &mut root,
        &mut child,
        &mut root_x,
        &mut root_y,
        &mut x,
        &mut y,
        &mut state,
    );

    (x, y)
}

#[inline]
pub unsafe fn emit_button_event(
    display: *mut xlib::Display,
    window: xlib::Window,
    event_type: c_int,
    button: c_uint,
    button_mask: c_uint,
    x: c_int,
    y: c_int,
) -> bool {
    let screen = xlib::XDefaultScreen(display);
    let root = xlib::XRootWindow(display, screen);

    let mut event = xlib::XEvent::from(xlib::XButtonEvent {
        type_: event_type,
        serial: 0,
        send_event: xlib::True,
        display,
        window,
        root,
        subwindow: 0,
        time: xlib::CurrentTime,
        x,
        y,
        x_root: 0,
        y_root: 0,
        state: button_mask,
        button,
        same_screen: xlib::True,
    });

    xlib::XSendEvent(
        display,
        xlib::PointerWindow as xlib::Window,
        xlib::True,
        xlib::NoEventMask,
        &mut event,
    ) == xlib::True
}
