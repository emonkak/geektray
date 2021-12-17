use std::any::Any;
use x11::xlib;

#[derive(Debug)]
struct WidgetPod {
    widget: Box<dyn Any>,
    window: Option<xlib::Window>,
    bounds: Rectangle,
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct WidgetId(u32);

#[derive(Debug)]
struct WidgetStorage {
    widgets: HashMap<WidgetId, Widget>;
}

trait Widget {
    fn render(&self, display: *mut xlib::Display, window: xlib::Window, context: &mut RenderContext);

    fn layout(&self, window_size: Size, children: Vec<WidgetId>, widget_storage: &mut WidgetStorage);
}
