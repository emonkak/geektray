use std::mem;
use std::ops::{Add, Div, Rem};
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
    let client_message_event = xlib::XClientMessageEvent {
        type_: xlib::ClientMessage,
        serial: 0,
        send_event: xlib::True,
        display,
        window,
        message_type,
        format: 32,
        data,
    };

    xlib::XSendEvent(
        display,
        destination,
        xlib::False,
        xlib::StructureNotifyMask,
        &mut client_message_event.into(),
    ) == xlib::True
}

#[inline]
pub unsafe fn send_button_event(
    display: *mut xlib::Display,
    window: xlib::Window,
    event_type: c_int,
    button: c_uint,
    button_mask: c_uint,
    x: c_int,
    y: c_int,
    x_root: c_int,
    y_root: c_int,
) -> bool {
    let screen = xlib::XDefaultScreen(display);
    let root = xlib::XRootWindow(display, screen);

    let event = xlib::XButtonEvent {
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
        x_root,
        y_root,
        state: button_mask,
        button,
        same_screen: xlib::True,
    };

    xlib::XSendEvent(
        display,
        window,
        xlib::True,
        xlib::NoEventMask,
        &mut event.into(),
    ) == xlib::True
}

#[inline]
pub unsafe fn emit_click_event(
    display: *mut xlib::Display,
    window: xlib::Window,
    button: c_uint,
    button_mask: c_uint,
    x: c_int,
    y: c_int,
) -> bool {
    let screen = xlib::XDefaultScreenOfDisplay(display);
    let root = xlib::XRootWindowOfScreen(screen);
    let (cursor_x, cursor_y) = get_pointer_position(display, root);

    let mut x_root = 0;
    let mut y_root = 0;
    let mut _subwindow = 0;

    xlib::XTranslateCoordinates(
        display,
        window,
        root,
        x,
        y,
        &mut x_root,
        &mut y_root,
        &mut _subwindow,
    );

    xlib::XWarpPointer(display, 0, root, 0, 0, 0, 0, x_root, y_root);

    let result = send_button_event(
        display,
        window,
        xlib::ButtonPress,
        button,
        button_mask,
        x,
        y,
        x_root,
        y_root,
    );
    if !result {
        return false;
    }

    let result = send_button_event(
        display,
        window,
        xlib::ButtonRelease,
        button,
        button_mask,
        x,
        y,
        x_root,
        y_root,
    );
    if !result {
        return false;
    }

    xlib::XWarpPointer(display, 0, root, 0, 0, 0, 0, cursor_x, cursor_y);
    xlib::XFlush(display);

    true
}

#[inline]
pub unsafe fn get_window_title(
    display: *mut xlib::Display,
    window: xlib::Window,
) -> Option<String> {
    get_window_variable_property::<u8>(display, window, xlib::XA_WM_NAME, xlib::XA_STRING, 256)
        .or_else(|| {
            get_window_variable_property::<u8>(
                display,
                window,
                xlib::XA_WM_CLASS,
                xlib::XA_STRING,
                256,
            )
        })
        .and_then(|mut bytes| {
            if let Some(null_position) = bytes.iter().position(|c| *c == 0) {
                bytes.resize(null_position, 0);
            }
            String::from_utf8(bytes).ok()
        })
}

#[inline]
pub unsafe fn get_window_fixed_property<T: Sized, const N: usize>(
    display: *mut xlib::Display,
    window: xlib::Window,
    property_atom: xlib::Atom,
) -> Option<Box<[T; N]>> {
    let mut actual_type: xlib::Atom = 0;
    let mut actual_format: i32 = 0;
    let mut nitems: u64 = 0;
    let mut bytes_after: u64 = 0;
    let mut prop: *mut u8 = ptr::null_mut();

    let expected_format = match mem::size_of::<T>() {
        8 | 4 => 32,
        2 => 16,
        1 => 8,
        _ => 0,
    };

    let result = xlib::XGetWindowProperty(
        display,
        window,
        property_atom,
        0,
        ceiling_div(expected_format * N, 4) as i64,
        xlib::False,
        xlib::AnyPropertyType as u64,
        &mut actual_type,
        &mut actual_format,
        &mut nitems,
        &mut bytes_after,
        &mut prop,
    );

    if result != xlib::Success.into()
        || actual_format != expected_format as c_int
        || nitems != N as c_ulong
        || bytes_after != 0
        || prop.is_null()
    {
        return None;
    }

    Some(Box::from_raw(prop.cast()))
}

#[inline]
pub unsafe fn get_window_variable_property<T: Sized>(
    display: *mut xlib::Display,
    window: xlib::Window,
    property_atom: xlib::Atom,
    property_type: xlib::Atom,
    property_capacity: u64,
) -> Option<Vec<T>> {
    let mut actual_type: xlib::Atom = 0;
    let mut actual_format: i32 = 0;
    let mut nitems: u64 = 0;
    let mut bytes_after: u64 = 0;
    let mut prop: *mut u8 = ptr::null_mut();

    let expected_format = match mem::size_of::<T>() {
        8 | 4 => 32,
        2 => 16,
        1 => 8,
        _ => 0,
    };

    let result = xlib::XGetWindowProperty(
        display,
        window,
        property_atom,
        0,
        ceiling_div(property_capacity, 4) as c_long,
        xlib::False,
        property_type,
        &mut actual_type,
        &mut actual_format,
        &mut nitems,
        &mut bytes_after,
        &mut prop,
    );

    if result != xlib::Success.into()
        || actual_type != property_type
        || actual_format != expected_format
        || nitems == 0
        || prop.is_null()
    {
        return None;
    }

    let mut data = Vec::from_raw_parts(
        prop.cast(),
        nitems as usize,
        (nitems + bytes_after) as usize,
    );
    let mut offset = nitems;

    while bytes_after > 0 {
        let result = xlib::XGetWindowProperty(
            display,
            window,
            property_atom,
            ceiling_div(offset, 4) as i64,
            ceiling_div(bytes_after, 4) as i64,
            xlib::False,
            property_type,
            &mut actual_type,
            &mut actual_format,
            &mut nitems,
            &mut bytes_after,
            &mut prop,
        );

        if result != xlib::Success.into()
            || actual_type != property_type
            || actual_format != expected_format
            || prop.is_null()
        {
            return None;
        }

        let additional_data = Vec::from_raw_parts(prop.cast(), nitems as usize, nitems as usize);
        data.extend(additional_data);

        offset += nitems;
    }

    Some(data)
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
fn ceiling_div<T>(n: T, divisor: T) -> T
where
    T: Copy + Add<Output = T> + Div<Output = T> + Rem<Output = T>,
{
    (n + n % divisor) / divisor
}
