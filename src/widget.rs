use crate::effect::Effect;
use crate::event_loop::X11Event;
use crate::geometrics::{Point, Size};
use crate::render_context::RenderContext;

pub trait Widget {
    fn render(&self, position: Point, layout: &LayoutResult, context: &mut RenderContext);

    fn layout(&self, container_size: Size) -> LayoutResult;

    fn on_event(&mut self, _event: &X11Event, _position: Point, _layout: &LayoutResult) -> Effect {
        Effect::None
    }
}

#[derive(Clone, Debug, Default)]
pub struct LayoutResult {
    pub size: Size,
    pub children: Vec<(Point, LayoutResult)>,
}
