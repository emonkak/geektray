use nix::sys::epoll;
use nix::sys::time::TimeValLike;
use nix::time::ClockId;
use nix;
use std::cell::RefCell;
use std::collections::LinkedList;
use std::os::unix::io::RawFd;

pub use self::CallbackResult::*;

const IDLE_TIMEOUT_MILLI_SECONDS: i64 = 50;

pub struct TaskScheduler {
    epoll_fd: RawFd,
    num_watched_fds: usize,
    idle_callbacks: RefCell<LinkedList<Box<dyn FnOnce(IdleDeadline)>>>,
}

impl TaskScheduler {
    pub fn new() -> nix::Result<Self> {
        Ok(Self {
            epoll_fd: epoll::epoll_create()?,
            num_watched_fds: 0,
            idle_callbacks: RefCell::new(LinkedList::new()),
        })
    }

    pub fn watch(&mut self, fd: RawFd) -> nix::Result<()> {
        let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, fd as u64);
        epoll::epoll_ctl(self.epoll_fd, epoll::EpollOp::EpollCtlAdd, fd, Some(&mut event))?;
        self.num_watched_fds += 1;
        Ok(())
    }

    pub fn request_idle_callback<F: FnOnce(IdleDeadline) + 'static>(&self, callback: F) {
        self.idle_callbacks.borrow_mut().push_back(Box::new(callback));
    }

    pub fn wait<F: FnMut(RawFd) -> CallbackResult<T>, T>(&self, timeout: isize, mut callback: F) -> T {
        let mut epoll_events = vec!(epoll::EpollEvent::empty(); self.num_watched_fds);

        loop {
            let mut num_fds = 0;

            while !self.idle_callbacks.borrow().is_empty() {
                num_fds = epoll::epoll_wait(self.epoll_fd, &mut epoll_events, 0)
                    .unwrap_or(0);
                if num_fds > 0 {
                    break;
                }

                let idle_callback = self.idle_callbacks
                    .borrow_mut()
                    .pop_front()
                    .unwrap();
                let deadline = IdleDeadline::new(IDLE_TIMEOUT_MILLI_SECONDS);
                idle_callback(deadline);
            }

            if num_fds == 0 {
                num_fds = epoll::epoll_wait(self.epoll_fd, &mut epoll_events, timeout)
                    .unwrap_or(0);
            }

            for epoll_event in epoll_events.iter().take(num_fds) {
                if let CallbackResult::Return(result) = callback(epoll_event.data() as RawFd) {
                    return result;
                }
            }
        }
    }
}

pub enum CallbackResult<T> {
    Continue,
    Return(T),
}

pub struct IdleDeadline {
    deadline: i64,
}

impl IdleDeadline {
    fn new(timeout: i64) -> Self {
        Self {
            deadline: now_ticks() + timeout,
        }
    }

    pub fn time_remaining(&self) -> i64 {
        self.deadline - now_ticks()
    }

    pub fn did_timeout(&self) -> bool {
        self.time_remaining() <= 0
    }
}

fn now_ticks() -> i64 {
    ClockId::CLOCK_REALTIME
        .now()
        .map_or(0, |now| now.num_milliseconds())
}
