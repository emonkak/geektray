extern crate x11;

use std::env;
use std::io::Write;
use std::mem;
use std::os::raw::*;
use std::ptr;
use x11::xlib;

const XEMBED_MAPPED: c_ulong = 1 << 0;

#[allow(dead_code)]
enum SystemTrayOpcode {
    RequestDock = 0,
    BeginMessage = 1,
    CancelMessage = 2,
}

fn main() {
    let display = unsafe { xlib::XOpenDisplay(ptr::null()) };
    if display.is_null() {
        panic!(
            "No display found at {}",
            env::var("DISPLAY").unwrap_or_default()
        );
    }

    let selection_atom = unsafe {
        let screen_number = xlib::XDefaultScreen(display);
        xlib::XInternAtom(
            display,
            format!("_NET_SYSTEM_TRAY_S{}\0", screen_number).as_ptr() as *const c_char,
            xlib::False,
        )
    };
    let manager_atom =
        unsafe { xlib::XInternAtom(display, "MANAGER\0".as_ptr() as *const c_char, xlib::False) };

    let window = unsafe {
        let screen = xlib::XDefaultScreenOfDisplay(display);
        let screen_number = xlib::XScreenNumberOfScreen(screen);
        let root = xlib::XRootWindowOfScreen(screen);

        let mut attributes: xlib::XSetWindowAttributes = mem::MaybeUninit::uninit().assume_init();
        attributes.backing_store = xlib::WhenMapped;
        attributes.background_pixel = xlib::XWhitePixel(display, screen_number);
        attributes.event_mask = xlib::ButtonPressMask
            | xlib::ButtonReleaseMask
            | xlib::ExposureMask
            | xlib::StructureNotifyMask;
        attributes.override_redirect = xlib::True;

        let window = xlib::XCreateWindow(
            display,
            root,
            0,
            0,
            24,
            24,
            0,
            xlib::CopyFromParent,
            xlib::InputOutput as u32,
            xlib::CopyFromParent as *mut xlib::Visual,
            xlib::CWBackingStore | xlib::CWBackPixel | xlib::CWEventMask | xlib::CWOverrideRedirect,
            &mut attributes,
        );

        xlib::XStoreName(
            display,
            window,
            "Tray Icon Test\0".as_ptr() as *const c_char,
        );

        set_xembed_info(display, window);

        window
    };

    let mut system_tray = unsafe {
        let selection_window = xlib::XGetSelectionOwner(display, selection_atom);
        if selection_window != 0 {
            request_dock(display, window, selection_window);
            Some(window)
        } else {
            xlib::XMapWindow(display, window);
            None
        }
    };

    let mut event: xlib::XEvent = unsafe { mem::MaybeUninit::uninit().assume_init() };
    let mut notification_id = 0;

    unsafe {
        let root = xlib::XDefaultRootWindow(display);
        // Required for watching MANAGER client message
        xlib::XSelectInput(display, root, xlib::StructureNotifyMask);
        xlib::XFlush(display);
    }

    loop {
        unsafe {
            xlib::XNextEvent(display, &mut event);
        }

        match event.get_type() {
            xlib::Expose => unsafe {
                let event = xlib::XExposeEvent::from(event);
                if event.window == window && event.count == 0 {
                    xlib::XClearWindow(display, window);
                }
            },
            xlib::ButtonRelease => unsafe {
                notification_id += 1;
                xlib::XStoreName(
                    display,
                    window,
                    format!("Tray Icon Test #{:?}\0", notification_id).as_ptr() as *const c_char,
                );
                if let Some(selection_window) = system_tray {
                    send_balloon_messages(
                        display,
                        window,
                        selection_window,
                        3000,
                        &format!("Test Message #{:?}", notification_id),
                        notification_id,
                    );
                }
            },
            xlib::ClientMessage => unsafe {
                let event = xlib::XClientMessageEvent::from(event);
                if event.message_type == manager_atom {
                    if event.data.get_long(1) as xlib::Atom == selection_atom {
                        let selection_window = event.data.get_long(2) as xlib::Window;
                        system_tray = Some(selection_window);
                        request_dock(display, window, selection_window);
                    }
                }
            },
            _ => {}
        }
    }
}

unsafe fn request_dock(
    display: *mut xlib::Display,
    window: xlib::Window,
    selection_window: xlib::Window,
) {
    let opcode_atom = xlib::XInternAtom(
        display,
        "_NET_SYSTEM_TRAY_OPCODE\0".as_ptr() as *const c_char,
        xlib::False,
    );

    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, xlib::CurrentTime as c_long);
    data.set_long(1, SystemTrayOpcode::RequestDock as c_long);
    data.set_long(2, window as c_long);

    let event = xlib::XClientMessageEvent {
        type_: xlib::ClientMessage,
        serial: 0,
        send_event: xlib::True,
        display,
        window,
        message_type: opcode_atom,
        format: 32,
        data,
    };

    xlib::XSendEvent(
        display,
        selection_window,
        xlib::False,
        0xffffff,
        &mut event.into(),
    );

    xlib::XFlush(display);
}

unsafe fn set_xembed_info(display: *mut xlib::Display, window: xlib::Window) {
    let xembed_info_atom = xlib::XInternAtom(
        display,
        "_XEMBED_INFO\0".as_ptr() as *const c_char,
        xlib::False,
    );

    let xembed_info: [c_ulong; 2] = [0, XEMBED_MAPPED];

    xlib::XChangeProperty(
        display,
        window,
        xembed_info_atom,
        xembed_info_atom,
        32,
        xlib::PropModeReplace,
        xembed_info.as_ptr().cast(),
        2,
    );
}

unsafe fn send_balloon_messages(
    display: *mut xlib::Display,
    window: xlib::Window,
    selection_window: xlib::Window,
    timeout_millis: c_long,
    body: &str,
    id: c_long,
) {
    begin_message(
        display,
        window,
        selection_window,
        timeout_millis,
        body.len() as c_long,
        id,
    );

    send_message_data(display, window, selection_window, body);

    xlib::XFlush(display);
}

unsafe fn begin_message(
    display: *mut xlib::Display,
    window: xlib::Window,
    selection_window: xlib::Window,
    timeout_millis: c_long,
    length: c_long,
    id: c_long,
) {
    let opcode_atom = xlib::XInternAtom(
        display,
        "_NET_SYSTEM_TRAY_OPCODE\0".as_ptr() as *const c_char,
        xlib::False,
    );

    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, xlib::CurrentTime as c_long);
    data.set_long(1, SystemTrayOpcode::BeginMessage as c_long);
    data.set_long(2, timeout_millis);
    data.set_long(3, length);
    data.set_long(4, id);

    let event = xlib::XClientMessageEvent {
        type_: xlib::ClientMessage,
        serial: 0,
        send_event: xlib::True,
        display,
        window,
        message_type: opcode_atom,
        format: 32,
        data: data.into(),
    };

    xlib::XSendEvent(
        display,
        selection_window,
        xlib::False,
        0xffffff,
        &mut event.into(),
    );

    xlib::XFlush(display);
}

unsafe fn send_message_data(
    display: *mut xlib::Display,
    window: xlib::Window,
    selection_window: xlib::Window,
    body: &str,
) {
    let message_data_atom = xlib::XInternAtom(
        display,
        "_NET_SYSTEM_TRAY_MESSAGE_DATA\0".as_ptr() as *const c_char,
        xlib::False,
    );

    for chunk in body.as_bytes().chunks(20) {
        let mut data = xlib::ClientMessageData::new();
        (data.as_mut() as &mut [u8]).write(chunk).unwrap();

        let event = xlib::XClientMessageEvent {
            type_: xlib::ClientMessage,
            serial: 0,
            send_event: xlib::True,
            display,
            window,
            message_type: message_data_atom,
            format: 8,
            data,
        };

        xlib::XSendEvent(
            display,
            selection_window,
            xlib::False,
            0xffffff,
            &mut event.into(),
        );
    }
}
