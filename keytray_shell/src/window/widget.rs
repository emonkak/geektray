use x11rb::protocol;

use super::effect::Effect;
use super::layout::Layout;
use crate::geometrics::{PhysicalPoint, PhysicalSize, Point, Size};
use crate::graphics::{RenderContext, RenderOp};

pub trait Widget {
    fn render(
        &self,
        position: Point,
        layout: &Layout,
        index: usize,
        context: &mut RenderContext,
    ) -> RenderOp;

    fn layout(&self, container_size: Size) -> Layout;

    fn on_resize_window(
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
