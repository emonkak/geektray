use std::ops::Add;
use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{Point, Size};
use crate::render_context::RenderContext;

pub trait Widget<Message> {
    fn render(&mut self, position: Point, layout: &LayoutResult, context: &mut RenderContext);

    fn layout(&mut self, container_size: Size) -> LayoutResult;

    fn on_event(&mut self, _event: &X11Event, _position: Point, _layout: &LayoutResult) -> Effect {
        Effect::None
    }

    fn on_message(&mut self, _message: Message) -> Effect {
        Effect::None
    }
}

#[derive(Clone, Debug, Default)]
pub struct LayoutResult {
    pub size: Size,
    pub children: Vec<(Point, LayoutResult)>,
}

pub enum Effect {
    None,
    Batch(Vec<Effect>),
    Action(Box<dyn FnOnce(*mut xlib::Display, xlib::Window)>),
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
