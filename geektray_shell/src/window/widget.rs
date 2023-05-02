use x11rb::errors::ReplyError;
use x11rb::protocol;
use x11rb::protocol::xproto;
use x11rb::xcb_ffi::XCBConnection;

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

    fn arrange_window(
        &self,
        _connection: &XCBConnection,
        _screen_num: usize,
        _size: PhysicalSize,
    ) -> PhysicalPoint {
        PhysicalPoint { x: 0, y: 0 }
    }

    fn layout_window(
        &self,
        _connection: &XCBConnection,
        _screen_num: usize,
        _window: xproto::Window,
        _position: PhysicalPoint,
        _old_size: PhysicalSize,
        _new_size: PhysicalSize,
    ) -> Result<(), ReplyError> {
        Ok(())
    }

    fn on_event(&mut self, _event: &protocol::Event, _position: Point, _layout: &Layout) -> Effect {
        Effect::None
    }
}
