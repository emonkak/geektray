use libdbus_sys as dbus;
use nix;
use nix::errno;
use nix::sys::epoll;
use nix::sys::signal;
use nix::sys::signalfd;
use nix::unistd;
use std::ffi::CStr;
use std::os::raw::*;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol;

use crate::dbus::{DBusArguments, DBusConnection, DBusMessage, DBusVariant};

const EVENT_KIND_X11: u64 = 1;
const EVENT_KIND_SIGNAL: u64 = 2;
const EVENT_KIND_DBUS: u64 = 3;

const DBUS_INTERFACE_NAME: &'static [u8] = b"io.github.emonkak.keytray\0";

#[derive(Debug)]
pub struct EventLoop<Connection> {
    connection: Rc<Connection>,
    epoll_fd: RawFd,
    signal_fd: signalfd::SignalFd,
    dbus_connection: DBusConnection,
}

impl<Connection: self::Connection + AsRawFd> EventLoop<Connection> {
    pub fn new(connection: Rc<Connection>) -> anyhow::Result<Self> {
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
            let raw_fd = connection.as_raw_fd();
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
            connection,
            epoll_fd,
            signal_fd,
            dbus_connection,
        })
    }

    pub fn run<F>(&mut self, mut callback: F) -> anyhow::Result<()>
    where
        F: FnMut(Event, &mut ControlFlow, &mut EventLoop<Connection>) -> anyhow::Result<()>,
    {
        let mut epoll_events = vec![epoll::EpollEvent::empty(); 3];
        let mut control_flow = ControlFlow::Continue;

        'outer: loop {
            let available_fds =
                epoll::epoll_wait(self.epoll_fd, &mut epoll_events, -1).unwrap_or(0);

            for epoll_event in &epoll_events[0..available_fds] {
                if epoll_event.data() == EVENT_KIND_X11 {
                    while let Some(event) = self.connection.poll_for_event()? {
                        callback(Event::X11Event(event), &mut control_flow, self)?;

                        if matches!(control_flow, ControlFlow::Break) {
                            break 'outer;
                        }
                    }
                } else if epoll_event.data() == EVENT_KIND_SIGNAL {
                    if let Some(signal) = self.signal_fd.read_signal()? {
                        callback(Event::Signal(signal), &mut control_flow, self)?;

                        if matches!(control_flow, ControlFlow::Break) {
                            break 'outer;
                        }
                    }
                } else if epoll_event.data() == EVENT_KIND_DBUS {
                    if self.dbus_connection.read_write(0) {
                        while let Some(message) = self.dbus_connection.pop_message() {
                            callback(Event::DBusMessage(message), &mut control_flow, self)?;

                            if matches!(control_flow, ControlFlow::Break) {
                                break 'outer;
                            }
                        }
                    }
                } else {
                    unreachable!();
                }
            }
        }

        Ok(())
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

impl<Connection> Drop for EventLoop<Connection> {
    fn drop(&mut self) {
        unistd::close(self.epoll_fd).ok();
    }
}

#[derive(Debug)]
pub enum Event {
    X11Event(protocol::Event),
    DBusMessage(DBusMessage),
    Signal(signalfd::siginfo),
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
