use nix;
use nix::sys::epoll;
use nix::sys::signalfd;
use std::mem;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use x11::xlib;

pub enum Event {
    X11(xlib::XEvent),
    Signal(signalfd::siginfo),
}

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

                    if matches!(callback(Event::X11(x11_event)), ControlFlow::Break) {
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
