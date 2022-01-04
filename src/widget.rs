use crate::event_loop::X11Event;
use crate::geometrics::{Point, Size};
use crate::render_context::RenderContext;
use crate::window::WindowEffcet;

pub trait Widget {
    fn render(&self, position: Point, layout: &Layout, context: &mut RenderContext);

    fn layout(&self, container_size: Size) -> Layout;

    fn on_event(&mut self, _event: &X11Event, _position: Point, _layout: &Layout) -> WindowEffcet {
        WindowEffcet::None
    }
}

#[derive(Clone, Debug, Default)]
pub struct Layout {
    pub size: Size,
    pub children: Vec<(Point, Layout)>,
}
