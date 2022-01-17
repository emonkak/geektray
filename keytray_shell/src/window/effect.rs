use std::ops::Add;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto;
use x11rb::xcb_ffi::XCBConnection;

#[must_use]
pub enum Effect {
    Success,
    Failure,
    Batch(Vec<Effect>),
    Action(Box<dyn FnOnce(&XCBConnection, usize, xproto::Window) -> Result<Effect, ReplyError>>),
    RequestRedraw,
    RequestLayout,
}

impl Add for Effect {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::Success, y) => y,
            (x, Self::Success) => x,
            (Self::Failure, _) => Self::Failure,
            (_, Self::Failure) => Self::Failure,
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
