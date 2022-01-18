use std::collections::{HashMap, VecDeque};
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
use crate::event::{ControlFlow, Event, EventLoopContext, TimerId};
use crate::graphics::{PhysicalPoint, PhysicalRect, PhysicalSize, Point, RenderContext, Size};

pub struct Window<Widget> {
    widget: Widget,
    connection: Rc<XCBConnection>,
    screen_num: usize,
    override_redirect: bool,
    window: xproto::Window,
    position: PhysicalPoint,
    size: PhysicalSize,
    layout: Layout,
    is_mapped: bool,
    delayed_effects: HashMap<TimerId, Effect>,
}

impl<Widget: self::Widget> Window<Widget> {
    pub fn new<GetPosition>(
        widget: Widget,
        connection: Rc<XCBConnection>,
        screen_num: usize,
        initial_size: Size,
        override_redirect: bool,
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

            let values = xproto::CreateWindowAux::new()
                .event_mask(event_mask)
                .override_redirect(if override_redirect { 1 } else { 0 });

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
            override_redirect,
            window,
            position,
            size,
            layout,
            is_mapped: false,
            delayed_effects: HashMap::new(),
        })
    }

    pub fn id(&self) -> xproto::Window {
        self.window
    }

    pub fn position(&self) -> PhysicalPoint {
        self.position
    }

    pub fn size(&self) -> PhysicalSize {
        self.size
    }

    pub fn bounds(&self) -> PhysicalRect {
        PhysicalRect::new(self.position, self.size)
    }

    pub fn is_mapped(&self) -> bool {
        self.is_mapped
    }

    pub fn override_redirect(&self) -> bool {
        self.override_redirect
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn widget_mut(&mut self) -> &mut Widget {
        &mut self.widget
    }

    pub fn show(&self) -> Result<(), ReplyError> {
        let values = xproto::ConfigureWindowAux::new().stack_mode(xproto::StackMode::ABOVE);
        self.connection
            .configure_window(self.window, &values)?
            .check()?;
        self.connection.map_window(self.window)?.check()?;
        self.connection.flush()?;
        Ok(())
    }

    pub fn raise(&self) -> Result<(), ReplyError> {
        let values = xproto::ConfigureWindowAux::new().stack_mode(xproto::StackMode::ABOVE);
        self.connection
            .configure_window(self.window, &values)?
            .check()?;
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

    pub fn recalculate_layout(&mut self, context: &mut EventLoopContext) -> Result<(), ReplyError> {
        self.layout = self.widget.layout(self.size.unsnap());
        let size = self.layout.size.snap();

        if self.size != size {
            let effect = self.widget.on_change_layout(self.position, self.size, size);
            self.apply_effect(effect, context)?;
        } else {
            self.request_redraw()?;
        }

        self.connection.flush()?;

        Ok(())
    }

    pub fn apply_effect(
        &mut self,
        effect: Effect,
        context: &mut EventLoopContext,
    ) -> Result<bool, ReplyError> {
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
                Effect::Delay(effect, timeout) => {
                    let timer_id = context.request_timeout(timeout)?;
                    self.delayed_effects.insert(timer_id, *effect);
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
            self.recalculate_layout(context)?;
        } else if redraw_requested {
            self.request_redraw()?;
        }

        Ok(result)
    }

    pub fn on_event(
        &mut self,
        event: &Event,
        context: &mut EventLoopContext,
        control_flow: &mut ControlFlow,
    ) -> Result<(), ReplyOrIdError> {
        match event {
            Event::X11Event(event) => self.on_x11_event(event, context, control_flow),
            Event::Timer(timer) => {
                if let Some(effect) = self.delayed_effects.remove(&timer.id) {
                    self.apply_effect(effect, context)?;
                }
                Ok(())
            }
            Event::Signal(_) => Ok(()),
        }
    }

    fn on_x11_event(
        &mut self,
        event: &protocol::Event,
        context: &mut EventLoopContext,
        control_flow: &mut ControlFlow,
    ) -> Result<(), ReplyOrIdError> {
        use protocol::Event::*;

        if get_window_from_event(&event) == Some(self.window) {
            let effect = self.widget.on_event(event, Point::ZERO, &self.layout);
            self.apply_effect(effect, context)?;
        }

        match event {
            Expose(event) if event.window == self.window && event.count == 0 => {
                self.redraw(context)?;
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
                    self.recalculate_layout(context)?;
                }
            }
            DestroyNotify(event) if event.window == self.window => {
                *control_flow = ControlFlow::Break;
            }
            MapNotify(event) if event.window == self.window => {
                self.is_mapped = true;
                if self.override_redirect {
                    self.grab_keyboard()?;
                }
            }
            UnmapNotify(event) if event.window == self.window => {
                self.is_mapped = false;
                if self.override_redirect {
                    self.ungrab_keyboard()?;
                }
            }
            MapNotify(event) => {
                if self.override_redirect
                    && event.window != event.event // only from SUBSTRUCTURE_NOTIFY
                    && event.window != self.window
                    && !event.override_redirect
                {
                    // It maybe hidden under other windows, so lift the window.
                    self.raise()?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn redraw(&mut self, context: &mut EventLoopContext) -> Result<(), ReplyOrIdError> {
        let mut render_context = RenderContext::new(
            self.connection.clone(),
            self.screen_num,
            self.window,
            self.size,
        )?;

        self.widget
            .render(Point::ZERO, &self.layout, 0, &mut render_context);

        let effect = render_context.commit()?;

        self.apply_effect(effect, context)?;

        self.connection.flush()?;

        Ok(())
    }

    fn grab_keyboard(&self) -> Result<(), ReplyError> {
        let screen = &self.connection.setup().roots[self.screen_num];
        self.connection
            .grab_keyboard(
                true,
                screen.root,
                x11rb::CURRENT_TIME,
                xproto::GrabMode::ASYNC,
                xproto::GrabMode::ASYNC,
            )?
            .discard_reply_and_errors();
        self.connection.flush()?;
        Ok(())
    }

    fn ungrab_keyboard(&self) -> Result<(), ReplyError> {
        self.connection
            .ungrab_keyboard(x11rb::CURRENT_TIME)?
            .check()?;
        self.connection.flush()?;
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
        CirculateNotify(event) => Some(event.window),
        CirculateRequest(event) => Some(event.window),
        ClientMessage(event) => Some(event.window),
        ColormapNotify(event) => Some(event.window),
        ConfigureNotify(event) => Some(event.window),
        ConfigureRequest(event) => Some(event.window),
        CreateNotify(event) => Some(event.window),
        DestroyNotify(event) => Some(event.window),
        EnterNotify(event) => Some(event.event),
        Expose(event) => Some(event.window),
        FocusIn(event) => Some(event.event),
        FocusOut(event) => Some(event.event),
        GravityNotify(event) => Some(event.window),
        KeyPress(event) => Some(event.event),
        KeyRelease(event) => Some(event.event),
        LeaveNotify(event) => Some(event.event),
        MapNotify(event) => Some(event.window),
        MapRequest(event) => Some(event.window),
        MotionNotify(event) => Some(event.event),
        PropertyNotify(event) => Some(event.window),
        ReparentNotify(event) => Some(event.parent),
        ResizeRequest(event) => Some(event.window),
        SelectionClear(event) => Some(event.owner),
        SelectionNotify(event) => Some(event.requestor),
        SelectionRequest(event) => Some(event.owner),
        UnmapNotify(event) => Some(event.window),
        VisibilityNotify(event) => Some(event.window),
        _ => None,
    }
}
