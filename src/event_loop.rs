use nix;
use nix::sys::epoll;
use nix::sys::signal;
use nix::sys::signalfd;
use nix::unistd;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;
use x11rb::connection::Connection;
use x11rb::protocol;

use crate::ui::ControlFlow;

const EVENT_KIND_X11: u64 = 1;
const EVENT_KIND_SIGNAL: u64 = 2;

#[derive(Debug)]
pub struct EventLoop<Connection> {
    connection: Rc<Connection>,
    epoll_fd: RawFd,
    signal_fd: signalfd::SignalFd,
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
        })
    }

    pub fn run<F>(&mut self, mut callback: F) -> anyhow::Result<()>
    where
        F: FnMut(Event, &mut EventLoop<Connection>, &mut ControlFlow) -> anyhow::Result<()>,
    {
        let mut epoll_events = vec![epoll::EpollEvent::empty(); 3];
        let mut control_flow = ControlFlow::Continue;

        'outer: loop {
            let available_fds =
                epoll::epoll_wait(self.epoll_fd, &mut epoll_events, -1).unwrap_or(0);

            for epoll_event in &epoll_events[0..available_fds] {
                if epoll_event.data() == EVENT_KIND_X11 {
                    while let Some(event) = self.connection.poll_for_event()? {
                        callback(Event::X11Event(event), self, &mut control_flow)?;

                        if matches!(control_flow, ControlFlow::Break) {
                            break 'outer;
                        }
                    }
                } else if epoll_event.data() == EVENT_KIND_SIGNAL {
                    if let Some(signal) = self.signal_fd.read_signal()? {
                        callback(Event::Signal(signal), self, &mut control_flow)?;

                        if matches!(control_flow, ControlFlow::Break) {
                            break 'outer;
                        }
                    }
                } else {
                    unreachable!();
                }
            }
        }

        Ok(())
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
    Signal(signalfd::siginfo),
}
