use nix;
use nix::sys::epoll;
use nix::sys::signalfd;
use std::mem;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use x11::xlib;

#[derive(Debug)]
pub enum Event {
    X11Event(X11Event),
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

pub fn run_event_loop<F>(
    display: *mut xlib::Display,
    signal_fd: &mut signalfd::SignalFd,
    mut callback: F,
) -> nix::Result<()>
where
    F: FnMut(Event) -> ControlFlow,
{
    let epoll_fd = epoll::epoll_create()?;
    let x11_rawfd = unsafe { xlib::XConnectionNumber(display) as RawFd };
    let signal_rawfd = signal_fd.as_raw_fd();

    {
        let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, x11_rawfd as u64);
        epoll::epoll_ctl(
            epoll_fd,
            epoll::EpollOp::EpollCtlAdd,
            x11_rawfd,
            Some(&mut event),
        )?;
    }

    {
        let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, signal_rawfd as u64);
        epoll::epoll_ctl(
            epoll_fd,
            epoll::EpollOp::EpollCtlAdd,
            signal_rawfd,
            Some(&mut event),
        )?;
    }

    let mut epoll_events = vec![epoll::EpollEvent::empty(); 2];
    let mut x11_event: xlib::XEvent = unsafe { mem::MaybeUninit::uninit().assume_init() };

    'outer: loop {
        let available_fds = epoll::epoll_wait(epoll_fd, &mut epoll_events, -1).unwrap_or(0);

        for epoll_event in &epoll_events[0..available_fds] {
            if epoll_event.data() == x11_rawfd as _ {
                let pending_events = unsafe { xlib::XPending(display) };
                for _ in 0..pending_events {
                    unsafe {
                        xlib::XNextEvent(display, &mut x11_event);
                    }

                    if matches!(
                        callback(Event::X11Event(x11_event.into())),
                        ControlFlow::Break
                    ) {
                        break 'outer;
                    }
                }
            } else if epoll_event.data() == signal_rawfd as _ {
                if let Ok(Some(signal)) = signal_fd.read_signal() {
                    if matches!(callback(Event::Signal(signal)), ControlFlow::Break) {
                        break 'outer;
                    }
                }
            }
        }
    }

    Ok(())
}
