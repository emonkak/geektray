use libdbus_sys as dbus;
use nix;
use nix::errno;
use nix::sys::epoll;
use nix::sys::signal;
use nix::sys::signalfd;
use nix::unistd;
use std::ffi::CStr;
use std::fmt;
use std::mem;
use std::mem::ManuallyDrop;
use std::os::raw::*;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::ptr;
use std::str;
use x11::xlib;

const EVENT_X11: u64 = 1;
const EVNET_SIGNAL: u64 = 2;
const EVENT_DBUS: u64 = 3;

const DBUS_NAME: &'static str = "io.github.emonkak.keytray\0";

#[derive(Debug)]
pub struct EventLoop {
    display: *mut xlib::Display,
    epoll_fd: RawFd,
    signal_fd: ManuallyDrop<signalfd::SignalFd>,
    dbus_connection: *mut dbus::DBusConnection,
}

impl EventLoop {
    pub fn new(display: *mut xlib::Display) -> Result<Self, Error> {
        let epoll_fd = epoll::epoll_create().map_err(Error::NixError)?;
        let signal_fd = prepare_signal_fd().map_err(Error::NixError)?;
        let dbus_connection =
            unsafe { prepare_dbus_connection(epoll_fd) }.map_err(Error::DBusError)?;

        {
            let raw_fd = unsafe { xlib::XConnectionNumber(display) as RawFd };
            let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, EVENT_X11);
            epoll::epoll_ctl(
                epoll_fd,
                epoll::EpollOp::EpollCtlAdd,
                raw_fd,
                Some(&mut event),
            )
            .map_err(Error::NixError)?;
        }

        {
            let raw_fd = signal_fd.as_raw_fd();
            let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, EVNET_SIGNAL);
            epoll::epoll_ctl(
                epoll_fd,
                epoll::EpollOp::EpollCtlAdd,
                raw_fd,
                Some(&mut event),
            )
            .map_err(Error::NixError)?;
        }

        Ok(Self {
            display,
            epoll_fd,
            signal_fd: ManuallyDrop::new(signal_fd),
            dbus_connection,
        })
    }

    pub fn run<F>(&mut self, mut callback: F)
    where
        F: FnMut(Event, &mut EventLoopContext) -> ControlFlow,
    {
        let mut epoll_events = vec![epoll::EpollEvent::empty(); 3];
        let mut x11_event: xlib::XEvent = unsafe { mem::MaybeUninit::uninit().assume_init() };

        let mut context = EventLoopContext {
            dbus_connection: self.dbus_connection,
        };

        'outer: loop {
            let available_fds =
                epoll::epoll_wait(self.epoll_fd, &mut epoll_events, -1).unwrap_or(0);

            for epoll_event in &epoll_events[0..available_fds] {
                if epoll_event.data() == EVENT_X11 {
                    let pending_events = unsafe { xlib::XPending(self.display) };
                    for _ in 0..pending_events {
                        unsafe {
                            xlib::XNextEvent(self.display, &mut x11_event);
                        }

                        if matches!(
                            callback(Event::X11Event(x11_event.into()), &mut context),
                            ControlFlow::Break
                        ) {
                            break 'outer;
                        }
                    }
                } else if epoll_event.data() == EVNET_SIGNAL {
                    if let Ok(Some(signal)) = self.signal_fd.read_signal() {
                        if matches!(
                            callback(Event::Signal(signal), &mut context),
                            ControlFlow::Break
                        ) {
                            break 'outer;
                        }
                    }
                } else if epoll_event.data() == EVENT_DBUS {
                    unsafe {
                        if dbus::dbus_connection_read_write(self.dbus_connection, 0) != 0 {
                            while let Some(message) =
                                DBusMessage::from_connection(self.dbus_connection)
                            {
                                if matches!(
                                    callback(Event::DBusMessage(message), &mut context),
                                    ControlFlow::Break
                                ) {
                                    break 'outer;
                                }
                            }
                        }
                    }
                } else {
                    unreachable!();
                }
            }
        }
    }
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        unistd::close(self.epoll_fd).unwrap();
        unsafe {
            ManuallyDrop::drop(&mut self.signal_fd);
            dbus::dbus_connection_close(self.dbus_connection);
        }
    }
}

pub struct EventLoopContext {
    dbus_connection: *mut dbus::DBusConnection,
}

impl EventLoopContext {
    pub fn send_dbus_message(&self, message: &DBusMessage) -> bool {
        unsafe {
            let result =
                dbus::dbus_connection_send(self.dbus_connection, message.message, ptr::null_mut());
            dbus::dbus_connection_flush(self.dbus_connection);
            result != 0
        }
    }
}

#[derive(Debug)]
pub enum Error {
    NixError(nix::Error),
    DBusError(DBusError),
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

pub struct DBusMessage {
    message: *mut dbus::DBusMessage,
}

impl DBusMessage {
    pub fn from_connection(connection: *mut dbus::DBusConnection) -> Option<Self> {
        let message = unsafe { dbus::dbus_connection_pop_message(connection) };
        if !message.is_null() {
            Some(Self { message })
        } else {
            None
        }
    }

    pub fn new_method_return(&self) -> Self {
        let message = unsafe { dbus::dbus_message_new_method_return(self.message) };
        assert!(!message.is_null());
        Self { message }
    }

    pub fn message_type(&self) -> dbus::DBusMessageType {
        match unsafe { dbus::dbus_message_get_type(self.message) } {
            1 => dbus::DBusMessageType::MethodCall,
            2 => dbus::DBusMessageType::MethodReturn,
            3 => dbus::DBusMessageType::Error,
            4 => dbus::DBusMessageType::Signal,
            x => unreachable!("Invalid message type: {}", x),
        }
    }

    pub fn reply_serial(&self) -> u32 {
        unsafe { dbus::dbus_message_get_reply_serial(self.message) }
    }

    pub fn serial(&self) -> u32 {
        unsafe { dbus::dbus_message_get_serial(self.message) }
    }

    pub fn path(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus::dbus_message_get_path(self.message)) }
    }

    pub fn interface(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus::dbus_message_get_interface(self.message)) }
    }

    pub fn destination(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus::dbus_message_get_destination(self.message)) }
    }

    pub fn member(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus::dbus_message_get_member(self.message)) }
    }

    pub fn sender(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus::dbus_message_get_sender(self.message)) }
    }

    pub fn no_reply(&self) -> bool {
        unsafe { dbus::dbus_message_get_no_reply(self.message) != 0 }
    }

    pub fn auto_start(&self) -> bool {
        unsafe { dbus::dbus_message_get_auto_start(self.message) != 0 }
    }
}

impl fmt::Debug for DBusMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        f.debug_struct("DBusMessage")
            .field("message_type", &self.message_type())
            .field("reply_serial", &self.reply_serial())
            .field("serial", &self.serial())
            .field("path", &self.path())
            .field("interface", &self.interface())
            .field("destination", &self.destination())
            .field("member", &self.member())
            .field("sender", &self.sender())
            .field("no_reply", &self.no_reply())
            .field("auto_start", &self.auto_start())
            .finish()
    }
}

impl Drop for DBusMessage {
    fn drop(&mut self) {
        unsafe {
            dbus::dbus_message_unref(self.message);
        }
    }
}

pub struct DBusError {
    error: dbus::DBusError,
}

impl DBusError {
    pub fn init() -> Self {
        unsafe {
            let mut error = mem::MaybeUninit::uninit();
            dbus::dbus_error_init(error.as_mut_ptr());
            Self {
                error: error.assume_init(),
            }
        }
    }

    pub fn name(&self) -> Option<&str> {
        unsafe { c_str_to_slice(self.error.name) }
    }

    pub fn message(&self) -> Option<&str> {
        unsafe { c_str_to_slice(self.error.name) }
    }

    pub fn is_set(&self) -> bool {
        !self.error.name.is_null()
    }

    pub fn as_mut_ptr(&mut self) -> *mut dbus::DBusError {
        &mut self.error
    }
}

impl fmt::Debug for DBusError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        f.debug_struct("DBusError")
            .field("name", &self.name())
            .field("message", &self.message())
            .finish()
    }
}

impl Drop for DBusError {
    fn drop(&mut self) {
        unsafe {
            dbus::dbus_error_free(&mut self.error);
        }
    }
}

#[derive(Debug)]
pub enum ControlFlow {
    Continue,
    Break,
}

fn prepare_signal_fd() -> nix::Result<signalfd::SignalFd> {
    let mut mask = signalfd::SigSet::empty();
    mask.add(signal::Signal::SIGINT);
    mask.thread_block().unwrap();
    signalfd::SignalFd::new(&mask)
}

unsafe fn prepare_dbus_connection(epoll_fd: RawFd) -> Result<*mut dbus::DBusConnection, DBusError> {
    let mut error = DBusError::init();

    let connection = dbus::dbus_bus_get_private(dbus::DBusBusType::Session, error.as_mut_ptr());
    if error.is_set() {
        return Err(error);
    }

    dbus::dbus_bus_request_name(
        connection,
        DBUS_NAME.as_ptr() as *const c_char,
        dbus::DBUS_NAME_FLAG_REPLACE_EXISTING as c_uint,
        error.as_mut_ptr(),
    );
    if error.is_set() {
        return Err(error);
    }

    dbus::dbus_connection_set_watch_functions(
        connection,
        Some(handle_dbus_add_watch),
        Some(handle_dbus_remove_watch),
        None,
        epoll_fd as *mut c_void,
        None,
    );

    Ok(connection)
}

extern "C" fn handle_dbus_add_watch(watch: *mut dbus::DBusWatch, user_data: *mut c_void) -> u32 {
    let epoll_fd = user_data as RawFd;
    let raw_fd = unsafe { dbus::dbus_watch_get_unix_fd(watch) as RawFd };

    let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, EVENT_DBUS);
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

unsafe fn c_str_to_slice<'a>(c: *const c_char) -> Option<&'a str> {
    if c.is_null() {
        None
    } else {
        str::from_utf8(CStr::from_ptr(c).to_bytes()).ok()
    }
}
