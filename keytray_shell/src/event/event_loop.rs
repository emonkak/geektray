use nix::sys::epoll;
use nix::sys::signal;
use nix::sys::signalfd;
use nix::sys::timerfd;
use nix::unistd;
use nix;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol;

const EVENT_KIND_X11: u64 = 1;
const EVENT_KIND_TIMER: u64 = 2;
const EVENT_KIND_SIGNAL: u64 = 3;

#[derive(Debug)]
pub struct EventLoop<C> {
    connection: Rc<C>,
    epoll_fd: RawFd,
    signal_fd: signalfd::SignalFd,
    timer_fd: Rc<timerfd::TimerFd>,
}

impl<C: Connection + AsRawFd> EventLoop<C> {
    pub fn new(connection: Rc<C>) -> io::Result<Self> {
        let epoll_fd = epoll::epoll_create()?;

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

        let signal_fd = {
            let mut mask = signalfd::SigSet::empty();
            mask.add(signal::Signal::SIGINT);
            mask.thread_block()?;
            signalfd::SignalFd::new(&mask)
        }?;

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

        let timer_fd = timerfd::TimerFd::new(
            timerfd::ClockId::CLOCK_MONOTONIC,
            timerfd::TimerFlags::empty(),
        )?;

        {
            let raw_fd = timer_fd.as_raw_fd();
            let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, EVENT_KIND_TIMER);
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
            timer_fd: Rc::new(timer_fd),
        })
    }

    pub fn run<F>(&mut self, mut callback: F) -> anyhow::Result<()>
    where
        F: FnMut(Event, &mut EventLoopContext, &mut ControlFlow) -> anyhow::Result<()>,
    {
        let mut epoll_events = vec![epoll::EpollEvent::empty(); 3];
        let mut control_flow = ControlFlow::Continue;

        let mut context = EventLoopContext::new(self);

        'outer: loop {
            let available_fds =
                epoll::epoll_wait(self.epoll_fd, &mut epoll_events, -1).unwrap_or(0);

            for epoll_event in &epoll_events[0..available_fds] {
                if epoll_event.data() == EVENT_KIND_X11 {
                    while let Some(event) = self.connection.poll_for_event()? {
                        callback(Event::X11Event(event), &mut context, &mut control_flow)?;

                        if matches!(control_flow, ControlFlow::Break) {
                            break 'outer;
                        }
                    }
                } else if epoll_event.data() == EVENT_KIND_TIMER {
                    self.timer_fd.wait()?;
                    for timer in context.dequeue_timers()? {
                        callback(Event::Timer(timer), &mut context, &mut control_flow)?;

                        if matches!(control_flow, ControlFlow::Break) {
                            break 'outer;
                        }
                    }
                } else if epoll_event.data() == EVENT_KIND_SIGNAL {
                    if let Some(signal) = self.signal_fd.read_signal()? {
                        callback(Event::Signal(signal), &mut context, &mut control_flow)?;

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

pub struct EventLoopContext {
    timer_fd: Rc<timerfd::TimerFd>,
    timer_counter: TimerId,
    timer_queue: BinaryHeap<Reverse<Timer>>,
}

impl EventLoopContext {
    fn new<C>(event_loop: &EventLoop<C>) -> Self {
        Self {
            timer_fd: event_loop.timer_fd.clone(),
            timer_counter: 0,
            timer_queue: BinaryHeap::new(),
        }
    }

    pub fn request_timeout(&mut self, timeout: Duration) -> io::Result<TimerId> {
        let now = Instant::now();
        let deadline = now.checked_add(timeout).unwrap_or(now);
        match self.timer_queue.peek() {
            None => {
                self.timer_fd.set(
                    timerfd::Expiration::OneShot(timeout.into()),
                    timerfd::TimerSetTimeFlags::empty(),
                )?;
            }
            Some(timer) if deadline < timer.0.deadline => {
                self.timer_fd.set(
                    timerfd::Expiration::OneShot(timeout.into()),
                    timerfd::TimerSetTimeFlags::empty(),
                )?;
            }
            _ => {
            }
        }
        let id = self.next_timer_id();
        let timer = Timer { deadline, id };
        self.timer_queue.push(Reverse(timer));
        Ok(id)
    }

    fn dequeue_timers(&mut self) -> io::Result<Vec<Timer>> {
        let now = Instant::now();
        let mut timers = Vec::new();

        while let Some(timer) = self.timer_queue.peek() {
            if timer.0.deadline <= now {
                timers.push(self.timer_queue.pop().unwrap().0)
            } else {
                let timeout = timer.0.deadline.duration_since(now);
                self.timer_fd.set(
                    timerfd::Expiration::OneShot(timeout.into()),
                    timerfd::TimerSetTimeFlags::empty(),
                )?;
                break;
            }
        }

        Ok(timers)
    }

    fn next_timer_id(&mut self) -> TimerId {
        self.timer_counter += 1;
        self.timer_counter
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Timer {
    pub deadline: Instant,
    pub id: TimerId,
}

pub type TimerId = usize;

#[derive(Debug)]
pub enum Event {
    X11Event(protocol::Event),
    Signal(signalfd::siginfo),
    Timer(Timer),
}

#[derive(Debug)]
pub enum ControlFlow {
    Continue,
    Break,
}
