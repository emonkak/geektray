use libdbus_sys as dbus;
use nix;
use nix::errno;
use nix::sys::epoll;
use nix::sys::signal;
use nix::sys::signalfd;
use nix::unistd;
use std::error::Error;
use std::ffi::CStr;
use std::mem;
use std::os::raw::*;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::time::Duration;
use x11::xlib;

use crate::dbus::{DBusArguments, DBusConnection, DBusMessage, DBusVariant};

const EVENT_KIND_X11: u64 = 1;
const EVENT_KIND_SIGNAL: u64 = 2;
const EVENT_KIND_DBUS: u64 = 3;

const DBUS_INTERFACE_NAME: &'static [u8] = b"io.github.emonkak.keytray\0";

#[derive(Debug)]
pub struct EventLoop {
    display: *mut xlib::Display,
    epoll_fd: RawFd,
    signal_fd: signalfd::SignalFd,
    dbus_connection: DBusConnection,
}

impl EventLoop {
    pub fn new(display: *mut xlib::Display) -> Result<Self, Box<dyn Error>> {
        let epoll_fd = epoll::epoll_create()?;
        let signal_fd = {
            let mut mask = signalfd::SigSet::empty();
            mask.add(signal::Signal::SIGINT);
            mask.thread_block()?;
            signalfd::SignalFd::new(&mask)
        }?;
        let dbus_connection =
            DBusConnection::new(CStr::from_bytes_with_nul(DBUS_INTERFACE_NAME).unwrap())?;

        dbus_connection.set_watch_functions(
            Some(handle_dbus_add_watch),
            Some(handle_dbus_remove_watch),
            None,
            epoll_fd as *mut c_void,
            None,
        );

        {
            let raw_fd = unsafe { xlib::XConnectionNumber(display) as RawFd };
            let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, EVENT_KIND_X11);
            epoll::epoll_ctl(
                epoll_fd,
                epoll::EpollOp::EpollCtlAdd,
                raw_fd,
                Some(&mut event),
            )?;
        }

        {
            let raw_fd = signal_fd.as_raw_fd();
            let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, EVENT_KIND_SIGNAL);
            epoll::epoll_ctl(
                epoll_fd,
                epoll::EpollOp::EpollCtlAdd,
                raw_fd,
                Some(&mut event),
            )?;
        }

        Ok(Self {
            display,
            epoll_fd,
            signal_fd,
            dbus_connection,
        })
    }

    pub fn run<F>(&mut self, mut callback: F)
    where
        F: FnMut(Event, &mut EventLoop) -> ControlFlow,
    {
        let mut epoll_events = vec![epoll::EpollEvent::empty(); 3];
        let mut x11_event: xlib::XEvent = unsafe { mem::MaybeUninit::uninit().assume_init() };

        'outer: loop {
            let available_fds =
                epoll::epoll_wait(self.epoll_fd, &mut epoll_events, -1).unwrap_or(0);

            for epoll_event in &epoll_events[0..available_fds] {
                if epoll_event.data() == EVENT_KIND_X11 {
                    let pending_events = unsafe { xlib::XPending(self.display) };
                    for _ in 0..pending_events {
                        unsafe {
                            xlib::XNextEvent(self.display, &mut x11_event);
                        }

                        if matches!(
                            callback(Event::X11Event(x11_event.into()), self),
                            ControlFlow::Break
                        ) {
                            break 'outer;
                        }
                    }
                } else if epoll_event.data() == EVENT_KIND_SIGNAL {
                    if let Ok(Some(signal)) = self.signal_fd.read_signal() {
                        if matches!(callback(Event::Signal(signal), self), ControlFlow::Break) {
                            break 'outer;
                        }
                    }
                } else if epoll_event.data() == EVENT_KIND_DBUS {
                    if self.dbus_connection.read_write(0) {
                        while let Some(message) = self.dbus_connection.pop_message() {
                            if matches!(
                                callback(Event::DBusMessage(message), self),
                                ControlFlow::Break
                            ) {
                                break 'outer;
                            }
                        }
                    }
                } else {
                    unreachable!();
                }
            }
        }
    }

    pub fn send_dbus_message(&self, message: &DBusMessage) -> bool {
        let result = self.dbus_connection.send(message, None);
        self.dbus_connection.flush();
        result
    }

    pub fn send_notification(
        &self,
        summary: &CStr,
        body: &CStr,
        id: u32,
        timeout: Option<Duration>,
    ) -> bool {
        let message = DBusMessage::new_method_call(
            CStr::from_bytes_with_nul(b"org.freedesktop.Notifications\0").unwrap(),
            CStr::from_bytes_with_nul(b"/org/freedesktop/Notifications\0").unwrap(),
            CStr::from_bytes_with_nul(b"org.freedesktop.Notifications\0").unwrap(),
            CStr::from_bytes_with_nul(b"Notify\0").unwrap(),
        );

        let mut args = DBusArguments::new();
        args.add_argument(CStr::from_bytes_with_nul(DBUS_INTERFACE_NAME).unwrap()); // STRING app_name
        args.add_argument(id); // UINT32 replaces_id
        args.add_argument(CStr::from_bytes_with_nul(b"\0").unwrap()); // STRING app_icon
        args.add_argument(summary); // STRING summary
        args.add_argument(body); // STRING body
        args.add_argument(vec![] as Vec<&CStr>); // as actions
        args.add_argument(vec![] as Vec<(&CStr, DBusVariant)>); // a{sv} hints
        args.add_argument(timeout.map_or(-1, |duration| duration.as_millis() as i32)); // INT32 expire_timeout

        message.add_arguments(args);

        let result = self.dbus_connection.send(&message, None);
        self.dbus_connection.flush();
        result
    }
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        unistd::close(self.epoll_fd).ok();
    }
}

#[derive(Debug)]
pub enum Event {
    X11Event(X11Event),
    DBusMessage(DBusMessage),
    Signal(signalfd::siginfo),
}

#[derive(Debug)]
pub enum X11Event {
    // Keyboard events
    KeyPress(xlib::XKeyPressedEvent),
    KeyRelease(xlib::XKeyReleasedEvent),

    // Pointer events
    ButtonPress(xlib::XButtonPressedEvent),
    ButtonRelease(xlib::XButtonReleasedEvent),
    MotionNotify(xlib::XPointerMovedEvent),

    // Window crossing events
    EnterNotify(xlib::XEnterWindowEvent),
    LeaveNotify(xlib::XLeaveWindowEvent),

    // Input focus events
    FocusIn(xlib::XFocusInEvent),
    FocusOut(xlib::XFocusOutEvent),

    // Expose events
    Expose(xlib::XExposeEvent),
    GraphicsExpose(xlib::XGraphicsExposeEvent),
    NoExpose(xlib::XNoExposeEvent),

    // Structure control events
    CirculateRequest(xlib::XCirculateRequestEvent),
    ConfigureRequest(xlib::XConfigureRequestEvent),
    MapRequest(xlib::XMapRequestEvent),
    ResizeRequest(xlib::XResizeRequestEvent),

    // Window state notification events
    CirculateNotify(xlib::XCirculateEvent),
    ConfigureNotify(xlib::XConfigureEvent),
    CreateNotify(xlib::XCreateWindowEvent),
    DestroyNotify(xlib::XDestroyWindowEvent),
    GravityNotify(xlib::XGravityEvent),
    MapNotify(xlib::XMapEvent),
    MappingNotify(xlib::XMappingEvent),
    ReparentNotify(xlib::XReparentEvent),
    UnmapNotify(xlib::XUnmapEvent),
    VisibilityNotify(xlib::XVisibilityEvent),

    // Colormap state notification event
    ColormapNotify(xlib::XColormapEvent),

    // Client communication events
    ClientMessage(xlib::XClientMessageEvent),
    PropertyNotify(xlib::XPropertyEvent),
    SelectionClear(xlib::XSelectionClearEvent),
    SelectionNotify(xlib::XSelectionEvent),
    SelectionRequest(xlib::XSelectionRequestEvent),

    // Other event
    UnknownEvent(xlib::XAnyEvent),
}

impl X11Event {
    pub fn window(&self) -> xlib::Window {
        use X11Event::*;

        match self {
            KeyPress(event) => event.window,
            KeyRelease(event) => event.window,
            ButtonPress(event) => event.window,
            ButtonRelease(event) => event.window,
            MotionNotify(event) => event.window,
            EnterNotify(event) => event.window,
            LeaveNotify(event) => event.window,
            FocusIn(event) => event.window,
            FocusOut(event) => event.window,
            Expose(event) => event.window,
            GraphicsExpose(event) => event.drawable,
            NoExpose(event) => event.drawable,
            CirculateRequest(event) => event.window,
            ConfigureRequest(event) => event.window,
            MapRequest(event) => event.window,
            ResizeRequest(event) => event.window,
            CirculateNotify(event) => event.window,
            ConfigureNotify(event) => event.window,
            CreateNotify(event) => event.window,
            DestroyNotify(event) => event.window,
            GravityNotify(event) => event.window,
            MapNotify(event) => event.window,
            MappingNotify(event) => event.event, // BUGS: missing property name
            ReparentNotify(event) => event.window,
            UnmapNotify(event) => event.window,
            VisibilityNotify(event) => event.window,
            ColormapNotify(event) => event.window,
            ClientMessage(event) => event.window,
            PropertyNotify(event) => event.window,
            SelectionClear(event) => event.window,
            SelectionNotify(event) => event.requestor,
            SelectionRequest(event) => event.requestor,
            UnknownEvent(event) => event.window,
        }
    }
}

impl From<xlib::XEvent> for X11Event {
    fn from(event: xlib::XEvent) -> Self {
        use X11Event::*;

        match event.get_type() {
            xlib::KeyPress => KeyPress(xlib::XKeyPressedEvent::from(event)),
            xlib::KeyRelease => KeyRelease(xlib::XKeyReleasedEvent::from(event)),
            xlib::ButtonPress => ButtonPress(xlib::XButtonPressedEvent::from(event)),
            xlib::ButtonRelease => ButtonRelease(xlib::XButtonReleasedEvent::from(event)),
            xlib::MotionNotify => MotionNotify(xlib::XPointerMovedEvent::from(event)),
            xlib::EnterNotify => EnterNotify(xlib::XEnterWindowEvent::from(event)),
            xlib::LeaveNotify => LeaveNotify(xlib::XLeaveWindowEvent::from(event)),
            xlib::FocusIn => FocusIn(xlib::XFocusInEvent::from(event)),
            xlib::FocusOut => FocusOut(xlib::XFocusOutEvent::from(event)),
            xlib::Expose => Expose(xlib::XExposeEvent::from(event)),
            xlib::GraphicsExpose => GraphicsExpose(xlib::XGraphicsExposeEvent::from(event)),
            xlib::NoExpose => NoExpose(xlib::XNoExposeEvent::from(event)),
            xlib::CirculateRequest => CirculateRequest(xlib::XCirculateRequestEvent::from(event)),
            xlib::ConfigureRequest => ConfigureRequest(xlib::XConfigureRequestEvent::from(event)),
            xlib::MapRequest => MapRequest(xlib::XMapRequestEvent::from(event)),
            xlib::ResizeRequest => ResizeRequest(xlib::XResizeRequestEvent::from(event)),
            xlib::CirculateNotify => CirculateNotify(xlib::XCirculateEvent::from(event)),
            xlib::ConfigureNotify => ConfigureNotify(xlib::XConfigureEvent::from(event)),
            xlib::CreateNotify => CreateNotify(xlib::XCreateWindowEvent::from(event)),
            xlib::DestroyNotify => DestroyNotify(xlib::XDestroyWindowEvent::from(event)),
            xlib::GravityNotify => GravityNotify(xlib::XGravityEvent::from(event)),
            xlib::MapNotify => MapNotify(xlib::XMapEvent::from(event)),
            xlib::MappingNotify => MappingNotify(xlib::XMappingEvent::from(event)),
            xlib::ReparentNotify => ReparentNotify(xlib::XReparentEvent::from(event)),
            xlib::UnmapNotify => UnmapNotify(xlib::XUnmapEvent::from(event)),
            xlib::VisibilityNotify => VisibilityNotify(xlib::XVisibilityEvent::from(event)),
            xlib::ColormapNotify => ColormapNotify(xlib::XColormapEvent::from(event)),
            xlib::ClientMessage => ClientMessage(xlib::XClientMessageEvent::from(event)),
            xlib::PropertyNotify => PropertyNotify(xlib::XPropertyEvent::from(event)),
            xlib::SelectionClear => SelectionClear(xlib::XSelectionClearEvent::from(event)),
            xlib::SelectionNotify => SelectionNotify(xlib::XSelectionEvent::from(event)),
            xlib::SelectionRequest => SelectionRequest(xlib::XSelectionRequestEvent::from(event)),
            _ => UnknownEvent(xlib::XAnyEvent::from(event)),
        }
    }
}

#[derive(Debug)]
pub enum ControlFlow {
    Continue,
    Break,
}

extern "C" fn handle_dbus_add_watch(watch: *mut dbus::DBusWatch, user_data: *mut c_void) -> u32 {
    let epoll_fd = user_data as RawFd;
    let raw_fd = unsafe { dbus::dbus_watch_get_unix_fd(watch) as RawFd };

    let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, EVENT_KIND_DBUS);
    let result = epoll::epoll_ctl(
        epoll_fd,
        epoll::EpollOp::EpollCtlAdd,
        raw_fd,
        Some(&mut event),
    );

    match result {
        Ok(()) => 1,
        Err(errno::Errno::EEXIST) => 1,
        _ => 0,
    }
}

extern "C" fn handle_dbus_remove_watch(watch: *mut dbus::DBusWatch, user_data: *mut c_void) {
    let epoll_fd = user_data as RawFd;
    let raw_fd = unsafe { dbus::dbus_watch_get_unix_fd(watch) as RawFd };

    epoll::epoll_ctl(epoll_fd, epoll::EpollOp::EpollCtlDel, raw_fd, None).ok();
}
