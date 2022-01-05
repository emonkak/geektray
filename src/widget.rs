use std::ops::Add;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto;
use x11rb::protocol;
use x11rb::xcb_ffi::XCBConnection;

use crate::geometrics::{Point, Size};
use crate::render_context::RenderContext;

pub trait Widget {
    fn render(&self, position: Point, layout: &Layout, index: usize, context: &mut RenderContext);

    fn layout(&self, container_size: Size) -> Layout;

    fn on_event(&mut self, _event: &protocol::Event, _position: Point, _layout: &Layout) -> Effect {
        Effect::None
    }
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub size: Size,
    pub children: Vec<(Point, Layout)>,
}

#[must_use]
pub enum Effect {
    None,
    Batch(Vec<Effect>),
    Action(Box<dyn FnOnce(&XCBConnection, usize, xproto::Window) -> Result<(), ReplyError>>),
    RequestRedraw,
    RequestLayout,
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
                let mut xs = vec![x];
                xs.extend(ys);
                Self::Batch(xs)
            }
            (x, y) => Self::Batch(vec![x, y]),
        }
    }
}
