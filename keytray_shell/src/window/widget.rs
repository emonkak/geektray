use x11rb::protocol;

use super::effect::Effect;
use super::layout::Layout;
use crate::graphics::{PhysicalPoint, PhysicalSize, Point, RenderContext, Size};

pub trait Widget {
    fn render(&self, position: Point, layout: &Layout, index: usize, context: &mut RenderContext);

    fn layout(&self, container_size: Size) -> Layout;

    fn on_change_layout(
        &mut self,
        _position: PhysicalPoint,
        _old_size: PhysicalSize,
        _new_size: PhysicalSize,
    ) -> Effect {
        Effect::RequestRedraw
    }

    fn on_event(&mut self, _event: &protocol::Event, _position: Point, _layout: &Layout) -> Effect {
        Effect::Success
    }
}
