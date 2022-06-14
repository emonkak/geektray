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
use crate::geometrics::{PhysicalPoint, PhysicalRect, PhysicalSize, Point, Size};
use crate::graphics::{RenderContext, RenderError};

pub struct Window<Widget> {
    widget: Widget,
    connection: Rc<XCBConnection>,
    screen_num: usize,
    window: xproto::Window,
    position: PhysicalPoint,
    size: PhysicalSize,
    layout: Layout,
    is_mapped: bool,
    should_redraw: bool,
    should_layout: bool,
    render_context: Option<RenderContext>,
    delayed_effects: HashMap<TimerId, Effect>,
}

impl<Widget: self::Widget> Window<Widget> {
    pub fn new(
        widget: Widget,
        connection: Rc<XCBConnection>,
        screen_num: usize,
        depth: u8,
        visual_id: xproto::Visualid,
        colormap: xproto::Colormap,
        initial_size: Size,
    ) -> Result<Self, ReplyOrIdError> {
        let layout = widget.layout(initial_size);
        let size = layout.size.snap();
        let position = widget.arrange_window(connection.as_ref(), screen_num, size);

        let window = {
            let window = connection.generate_id()?;
            let screen = &connection.setup().roots[screen_num];

            let event_mask = xproto::EventMask::BUTTON_PRESS
                | xproto::EventMask::BUTTON_RELEASE
                | xproto::EventMask::ENTER_WINDOW
                | xproto::EventMask::EXPOSURE
                | xproto::EventMask::FOCUS_CHANGE
                | xproto::EventMask::KEY_PRESS
                | xproto::EventMask::KEY_RELEASE
                | xproto::EventMask::LEAVE_WINDOW
                | xproto::EventMask::PROPERTY_CHANGE
                | xproto::EventMask::STRUCTURE_NOTIFY;

            let values = xproto::CreateWindowAux::new()
                .event_mask(event_mask)
                .colormap(colormap)
                .border_pixel(screen.black_pixel)
                .background_pixel(screen.black_pixel);

            connection
                .create_window(
                    depth,
                    window,
                    screen.root,
                    position.x as i16,
                    position.y as i16,
                    size.width as u16,
                    size.height as u16,
                    0, // border_width
                    xproto::WindowClass::INPUT_OUTPUT,
                    visual_id,
                    &values,
                )?
                .check()?;

            window
        };

        widget.layout_window(
            connection.as_ref(),
            screen_num,
            window,
            position,
            size,
            size,
        )?;

        Ok(Self {
            widget,
            connection,
            screen_num,
            window,
            position,
            size,
            layout,
            is_mapped: false,
            should_redraw: false,
            should_layout: false,
            render_context: None,
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

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn widget_mut(&mut self) -> &mut Widget {
        &mut self.widget
    }

    pub fn show(&self) -> Result<(), ReplyError> {
        {
            let position =
                self.widget
                    .arrange_window(self.connection.as_ref(), self.screen_num, self.size);
            let values = xproto::ConfigureWindowAux::new()
                .x(position.x)
                .y(position.y)
                .stack_mode(xproto::StackMode::ABOVE);
            self.connection
                .configure_window(self.window, &values)?
                .check()?;
        }
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

    pub fn request_redraw(&mut self) {
        self.should_redraw = true;
    }

    pub fn apply_effect(
        &mut self,
        effect: Effect,
        context: &mut EventLoopContext,
    ) -> Result<bool, ReplyError> {
        let mut pending_effects = VecDeque::new();
        let mut current = effect;
        let mut result = false;

        loop {
            match current {
                Effect::None => {}
                Effect::Batch(effects) => {
                    pending_effects.extend(effects);
                }
                Effect::Delay(effect, timeout) => {
                    let timer_id = context.request_timeout(timeout)?;
                    self.delayed_effects.insert(timer_id, *effect);
                }
                Effect::Action(action) => {
                    result = true;
                    current = action(self.connection.as_ref(), self.screen_num, self.window)?;
                    continue;
                }
                Effect::RequestRedraw => {
                    result = true;
                    self.should_redraw = true;
                }
                Effect::RequestLayout => {
                    result = true;
                    self.should_layout = true;
                }
            }
            if let Some(next) = pending_effects.pop_front() {
                current = next;
            } else {
                break;
            }
        }

        Ok(result)
    }

    pub fn process_event(
        &mut self,
        event: &Event,
        context: &mut EventLoopContext,
        control_flow: &mut ControlFlow,
    ) -> Result<(), RenderError> {
        match event {
            Event::X11Event(event) => self.on_x11_event(event, context, control_flow),
            Event::Timer(timer) => {
                if let Some(effect) = self.delayed_effects.remove(&timer.id) {
                    self.apply_effect(effect, context)?;
                }
                Ok(())
            }
            Event::Signal(_) => Ok(()),
            Event::NextTick => {
                if self.should_layout {
                    self.recalculate_layout()?;
                }
                if self.should_redraw && self.is_mapped {
                    self.redraw()?;
                }
                Ok(())
            }
        }
    }

    fn on_x11_event(
        &mut self,
        event: &protocol::Event,
        context: &mut EventLoopContext,
        control_flow: &mut ControlFlow,
    ) -> Result<(), RenderError> {
        use protocol::Event::*;

        if get_window_from_event(&event) == Some(self.window) {
            let effect = self.widget.on_event(event, Point::ZERO, &self.layout);
            self.apply_effect(effect, context)?;
        }

        match event {
            Expose(event) => {
                if event.window == self.window && event.count == 0 {
                    self.should_redraw = true;
                }
            }
            ConfigureNotify(event) => {
                if event.window == event.event && event.window == self.window {
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
                        self.should_layout = true;
                        self.render_context = None;
                    }
                }
            }
            DestroyNotify(event) => {
                if event.window == event.event && event.window == self.window {
                    *control_flow = ControlFlow::Break;
                }
            }
            MapNotify(event) => {
                if event.window == event.event && event.window == self.window {
                    self.is_mapped = true;
                }
            }
            UnmapNotify(event) => {
                if event.window == event.event && event.window == self.window {
                    self.is_mapped = false;
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn redraw(&mut self) -> Result<(), RenderError> {
        log::debug!("Redraw window");

        if self.render_context.is_none() {
            self.render_context = Some(RenderContext::new(
                self.connection.clone(),
                self.screen_num,
                self.window,
                self.size,
            )?);
        }

        let render_context = self.render_context.as_mut().unwrap();

        let render_op = self
            .widget
            .render(Point::ZERO, &self.layout, 0, render_context);

        render_context.commit(render_op)?;

        self.connection.flush()?;

        self.should_redraw = false;

        Ok(())
    }

    fn recalculate_layout(&mut self) -> Result<(), ReplyError> {
        let new_layout = self.widget.layout(self.size.unsnap());

        if new_layout != self.layout {
            let size = new_layout.size.snap();

            self.layout = new_layout;

            if self.size != size {
                log::debug!(
                    "Window resized from {}x{} to {}x{}",
                    self.size.width,
                    self.size.height,
                    size.width,
                    size.height
                );
                self.widget.layout_window(
                    self.connection.as_ref(),
                    self.screen_num,
                    self.window,
                    self.position,
                    self.size,
                    size,
                )?;
            } else {
                self.should_redraw = true;
            }
        }

        self.should_layout = false;

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
