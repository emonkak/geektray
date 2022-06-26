use std::ops::Add;
use std::time::Duration;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto;
use x11rb::xcb_ffi::XCBConnection;

#[must_use]
pub enum Effect {
    None,
    Batch(Vec<Effect>),
    Delay(Box<Effect>, Duration),
    Action(Box<dyn FnOnce(&XCBConnection, usize, xproto::Window) -> Result<Effect, ReplyError>>),
    RequestRedraw,
    RequestLayout,
}

impl Effect {
    pub fn action<F>(action: F) -> Self
    where
        F: 'static + FnOnce(&XCBConnection, usize, xproto::Window) -> Result<Effect, ReplyError>,
    {
        Self::Action(Box::new(action))
    }

    pub fn delay(self, timeout: Duration) -> Self {
        Self::Delay(Box::new(self), timeout)
    }
}

impl Default for Effect {
    fn default() -> Self {
        Self::None
    }
}

impl Add for Effect {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, y) => y,
            (x, Self::None) => x,
            (Self::Batch(mut xs), Self::Batch(ys)) => {
                xs.extend(ys);
                Self::Batch(xs)
            }
            (Self::Batch(mut xs), y) => {
                xs.push(y);
                Self::Batch(xs)
            }
            (x, Self::Batch(ys)) => {
                let mut xs = Vec::with_capacity(ys.len() + 1);
                xs.push(x);
                xs.extend(ys);
                Self::Batch(xs)
            }
            (x, y) => Self::Batch(vec![x, y]),
        }
    }
}
