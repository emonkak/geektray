use std::collections::VecDeque;
use std::rc::Rc;
use x11rb::connection::Connection;
use x11rb::errors::{ReplyError, ReplyOrIdError};
use x11rb::protocol;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

use super::effect::Effect;
use super::layout::Layout;
use super::widget::Widget;
use crate::event::ControlFlow;
use crate::graphics::{PhysicalPoint, PhysicalSize, Point, RenderContext, Size};

#[derive(Debug)]
pub struct Window<Widget> {
    widget: Widget,
    connection: Rc<XCBConnection>,
    screen_num: usize,
    window: xproto::Window,
    position: PhysicalPoint,
    size: PhysicalSize,
    layout: Layout,
    is_mapped: bool,
}

impl<Widget: self::Widget> Window<Widget> {
    pub fn new<GetPosition>(
        widget: Widget,
        connection: Rc<XCBConnection>,
        screen_num: usize,
        initial_size: Size,
        get_position: GetPosition,
    ) -> Result<Self, ReplyOrIdError>
    where
        GetPosition: FnOnce(&XCBConnection, usize, PhysicalSize) -> PhysicalPoint,
    {
        let layout = widget.layout(initial_size);
        let size = layout.size.snap();
        let position = get_position(connection.as_ref(), screen_num, size);

        let window = {
            let window = connection.generate_id()?;
            let screen = &connection.setup().roots[screen_num];

            let event_mask = xproto::EventMask::BUTTON_PRESS
                | xproto::EventMask::BUTTON_RELEASE
                | xproto::EventMask::ENTER_WINDOW
                | xproto::EventMask::EXPOSURE
                | xproto::EventMask::KEY_PRESS
                | xproto::EventMask::KEY_RELEASE
                | xproto::EventMask::LEAVE_WINDOW
                | xproto::EventMask::PROPERTY_CHANGE
                | xproto::EventMask::STRUCTURE_NOTIFY;

            let values = xproto::CreateWindowAux::new().event_mask(event_mask);

            connection
                .create_window(
                    screen.root_depth,
                    window,
                    screen.root,
                    position.x as i16,
                    position.y as i16,
                    size.width as u16,
                    size.height as u16,
                    0, // border_width
                    xproto::WindowClass::INPUT_OUTPUT,
                    x11rb::COPY_FROM_PARENT,
                    &values,
                )?
                .check()?;

            window
        };

        Ok(Self {
            widget,
            connection,
            screen_num,
            window,
            position,
            size,
            layout,
            is_mapped: false,
        })
    }

    pub fn id(&self) -> xproto::Window {
        self.window
    }

    pub fn size(&self) -> PhysicalSize {
        self.size
    }

    pub fn is_mapped(&self) -> bool {
        self.is_mapped
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn widget_mut(&mut self) -> &mut Widget {
        &mut self.widget
    }

    pub fn show(&self) -> Result<(), ReplyError> {
        self.connection.map_window(self.window)?.check()?;
        self.connection.flush()?;
        Ok(())
    }

    pub fn hide(&self) -> Result<(), ReplyError> {
        self.connection.unmap_window(self.window)?.check()?;
        self.connection.flush()?;
        Ok(())
    }

    pub fn move_position(&self, position: PhysicalPoint) -> Result<(), ReplyError> {
        let values = xproto::ConfigureWindowAux::new()
            .x(position.x as i32)
            .y(position.y as i32);
        self.connection
            .configure_window(self.window, &values)?
            .check()
    }

    pub fn request_redraw(&self) -> Result<(), ReplyError> {
        self.connection
            .clear_area(true, self.window, 0, 0, 0, 0)?
            .check()?;
        self.connection.flush()?;
        Ok(())
    }

    pub fn recalculate_layout(&mut self) -> Result<(), ReplyError> {
        self.layout = self.widget.layout(self.size.unsnap());
        let size = self.layout.size.snap();

        if self.size != size {
            let effect = self.widget.on_change_layout(self.position, self.size, size);
            self.apply_effect(effect)?;
        } else {
            self.request_redraw()?;
        }

        self.connection.flush()?;

        Ok(())
    }

    pub fn apply_effect(&mut self, effect: Effect) -> Result<bool, ReplyError> {
        let mut pending_effects = VecDeque::new();
        let mut current = effect;

        let mut redraw_requested = false;
        let mut layout_requested = false;
        let mut result = true;

        loop {
            match current {
                Effect::Success => {}
                Effect::Failure => {
                    result = false;
                }
                Effect::Batch(effects) => {
                    pending_effects.extend(effects);
                }
                Effect::Action(action) => {
                    current = action(self.connection.as_ref(), self.screen_num, self.window)?;
                    continue;
                }
                Effect::RequestRedraw => {
                    redraw_requested = true;
                }
                Effect::RequestLayout => {
                    layout_requested = true;
                }
            }
            if let Some(next) = pending_effects.pop_front() {
                current = next;
            } else {
                break;
            }
        }

        if layout_requested {
            self.recalculate_layout()?;
        } else if redraw_requested {
            self.request_redraw()?;
        }

        Ok(result)
    }

    pub fn on_event(
        &mut self,
        event: &protocol::Event,
        control_flow: &mut ControlFlow,
    ) -> Result<(), ReplyOrIdError> {
        use protocol::Event::*;

        if get_window_from_event(&event) == Some(self.window) {
            let effect = self.widget.on_event(event, Point::ZERO, &self.layout);
            self.apply_effect(effect)?;
        }

        match event {
            Expose(event) if event.window == self.window && event.count == 0 => {
                self.redraw()?;
            }
            ConfigureNotify(event) if event.window == self.window => {
                self.position = PhysicalPoint {
                    x: event.x as i32,
                    y: event.y as i32,
                };
                let size = PhysicalSize {
                    width: event.width as u32,
                    height: event.height as u32,
                };
                if self.size != size {
                    self.size = size;
                    self.recalculate_layout()?;
                }
            }
            DestroyNotify(event) if event.window == self.window => {
                *control_flow = ControlFlow::Break;
            }
            MapNotify(event) if event.window == self.window => {
                self.is_mapped = true;
            }
            UnmapNotify(event) if event.window == self.window => {
                self.is_mapped = false;
            }
            _ => {}
        }

        Ok(())
    }

    fn redraw(&mut self) -> Result<(), ReplyOrIdError> {
        let mut context = RenderContext::new(
            self.connection.clone(),
            self.screen_num,
            self.window,
            self.size,
        )?;

        self.widget
            .render(Point::ZERO, &self.layout, 0, &mut context);

        context.commit()?;

        Ok(())
    }
}

impl<Widget> Drop for Window<Widget> {
    fn drop(&mut self) {
        self.connection.destroy_window(self.window).ok();
    }
}

fn get_window_from_event(event: &protocol::Event) -> Option<xproto::Window> {
    use protocol::Event::*;

    match event {
        ButtonPress(event) => Some(event.event),
        ButtonRelease(event) => Some(event.event),
        CirculateNotify(event) => Some(event.event),
        CirculateRequest(event) => Some(event.event),
        ClientMessage(event) => Some(event.window),
        ColormapNotify(event) => Some(event.window),
        ConfigureNotify(event) => Some(event.event),
        ConfigureRequest(event) => Some(event.window),
        CreateNotify(event) => Some(event.window),
        DestroyNotify(event) => Some(event.event),
        EnterNotify(event) => Some(event.event),
        Expose(event) => Some(event.window),
        FocusIn(event) => Some(event.event),
        FocusOut(event) => Some(event.event),
        GravityNotify(event) => Some(event.event),
        KeyPress(event) => Some(event.event),
        KeyRelease(event) => Some(event.event),
        LeaveNotify(event) => Some(event.event),
        MapNotify(event) => Some(event.event),
        MapRequest(event) => Some(event.window),
        MotionNotify(event) => Some(event.event),
        PropertyNotify(event) => Some(event.window),
        ReparentNotify(event) => Some(event.event),
        ResizeRequest(event) => Some(event.window),
        SelectionClear(event) => Some(event.owner),
        SelectionNotify(event) => Some(event.requestor),
        SelectionRequest(event) => Some(event.owner),
        UnmapNotify(event) => Some(event.event),
        VisibilityNotify(event) => Some(event.window),
        _ => None,
    }
}
