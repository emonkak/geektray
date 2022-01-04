use crate::effect::Effect;
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
