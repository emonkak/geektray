use std::collections::VecDeque;
use std::rc::Rc;
use x11rb::connection::Connection;
use x11rb::errors::{ConnectionError, ReplyError};
use x11rb::properties;
use x11rb::protocol;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

use crate::atoms::Atoms;
use crate::config::UiConfig;
use crate::event_loop::ControlFlow;
use crate::geometrics::{PhysicalPoint, PhysicalSize, Point, Size};
use crate::render_context::RenderContext;
use crate::widget::{Effect, Layout, Widget};

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
    pub fn new(
        widget: Widget,
        connection: Rc<XCBConnection>,
        screen_num: usize,
        atoms: &Atoms,
        config: &UiConfig,
    ) -> Result<Self, ReplyError> {
        let layout = widget.layout(Size {
            width: config.window_width,
            height: 0.0,
        });
        let size = layout.size.snap();
        let position = get_window_position(&connection, screen_num, size);

        let window = {
            let window_id = connection.generate_id().unwrap();
            let screen = &connection.setup().roots[screen_num];

            let event_mask = xproto::EventMask::EXPOSURE
                | xproto::EventMask::KEY_PRESS
                | xproto::EventMask::KEY_RELEASE
                | xproto::EventMask::BUTTON_PRESS
                | xproto::EventMask::BUTTON_RELEASE
                | xproto::EventMask::ENTER_WINDOW
                | xproto::EventMask::LEAVE_WINDOW
                | xproto::EventMask::PROPERTY_CHANGE
                | xproto::EventMask::STRUCTURE_NOTIFY;

            let values = xproto::CreateWindowAux::new()
                .event_mask(event_mask)
                .backing_store(xproto::BackingStore::WHEN_MAPPED);

            connection.create_window(
                screen.root_depth,
                window_id,
                screen.root,
                position.x as i16,
                position.y as i16,
                size.width as u16,
                size.height as u16,
                0, // border_width
                xproto::WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                &values,
            )?;

            window_id
        };

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                atoms.WM_PROTOCOLS,
                xproto::AtomEnum::ATOM,
                &[
                    atoms._NET_WM_PING,
                    atoms._NET_WM_SYNC_REQUEST,
                    atoms.WM_DELETE_WINDOW,
                ],
            )?
            .check()?;

        {
            connection
                .change_property8(
                    xproto::PropMode::REPLACE,
                    window,
                    atoms.WM_NAME,
                    xproto::AtomEnum::STRING,
                    config.window_name.as_bytes(),
                )?
                .check()?;
            connection
                .change_property8(
                    xproto::PropMode::REPLACE,
                    window,
                    atoms._NET_WM_NAME,
                    atoms.UTF8_STRING,
                    config.window_name.as_bytes(),
                )?
                .check()?;
        }

        {
            let class_string = format!(
                "{}\0{}",
                config.window_class.as_ref(),
                config.window_class.as_ref()
            );
            connection
                .change_property8(
                    xproto::PropMode::REPLACE,
                    window,
                    atoms.WM_CLASS,
                    xproto::AtomEnum::STRING,
                    class_string.as_bytes(),
                )?
                .check()?;
        }

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                atoms._NET_WM_WINDOW_TYPE,
                xproto::AtomEnum::ATOM,
                &[atoms._NET_WM_WINDOW_TYPE_DIALOG],
            )?
            .check()?;

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                atoms._NET_WM_STATE,
                xproto::AtomEnum::ATOM,
                &[atoms._NET_WM_STATE_STICKY],
            )?
            .check()?;

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                atoms._NET_SYSTEM_TRAY_ORIENTATION,
                xproto::AtomEnum::CARDINAL,
                &[1], // _NET_SYSTEM_TRAY_ORIENTATION_VERT
            )?
            .check()?;

        {
            let screen = &connection.setup().roots[screen_num];
            connection
                .change_property32(
                    xproto::PropMode::REPLACE,
                    window,
                    atoms._NET_SYSTEM_TRAY_VISUAL,
                    xproto::AtomEnum::VISUALID,
                    &[screen.root_visual],
                )?
                .check()?;
        }

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

    pub fn window(&self) -> xproto::Window {
        self.window
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn widget_mut(&mut self) -> &mut Widget {
        &mut self.widget
    }

    pub fn show(&self) -> Result<(), ConnectionError> {
        self.connection.map_window(self.window)?;
        self.connection.flush()?;
        Ok(())
    }

    pub fn move_at_center(&self) -> Result<(), ConnectionError> {
        let position = get_window_position(&self.connection, self.screen_num, self.size);
        let mut values = xproto::ConfigureWindowAux::new();
        values.x = Some(position.x as i32);
        values.y = Some(position.y as i32);
        self.connection.configure_window(self.window, &values)?;
        Ok(())
    }

    pub fn hide(&self) -> Result<(), ConnectionError> {
        self.connection.unmap_window(self.window)?;
        self.connection.flush()?;
        Ok(())
    }

    pub fn toggle(&self) -> Result<(), ConnectionError> {
        if self.is_mapped {
            self.hide()
        } else {
            self.show()
        }
    }

    pub fn request_redraw(&self) -> Result<(), ConnectionError> {
        self.connection.clear_area(true, self.window, 0, 0, 0, 0)?;
        self.connection.flush()?;
        Ok(())
    }

    pub fn recalculate_layout(&mut self) -> Result<(), ReplyError> {
        self.layout = self.widget.layout(self.size.unsnap());
        let size = self.layout.size.snap();

        if self.size != size {
            let mut size_hints = properties::WmSizeHints::new();
            size_hints.min_size = Some((0, size.height as i32));
            size_hints.max_size = Some((0, size.height as i32));

            size_hints
                .set_normal_hints(self.connection.as_ref(), self.window)?
                .check()?;

            let mut values = xproto::ConfigureWindowAux::new();
            values.x = Some(self.position.x);
            values.y = Some(
                self.position.y - (((size.height as i32 - self.size.height as i32) / 2) as i32),
            );
            values.height = Some(size.height);
            values.width = Some(size.width);

            self.connection.configure_window(self.window, &values)?;
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
        let mut result = false;

        loop {
            match current {
                Effect::None => {}
                Effect::Batch(effects) => {
                    pending_effects.extend(effects);
                }
                Effect::Action(action) => {
                    action(&self.connection, self.screen_num, self.window)?;
                    result = true;
                }
                Effect::RequestRedraw => {
                    redraw_requested = true;
                    result = true;
                }
                Effect::RequestLayout => {
                    layout_requested = true;
                    result = true;
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
    ) -> Result<(), ReplyError> {
        use protocol::Event::*;

        let effect = self.widget.on_event(event, Point::ZERO, &self.layout);
        self.apply_effect(effect)?;

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

    fn redraw(&mut self) -> Result<(), ReplyError> {
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

fn get_window_position(
    connection: &XCBConnection,
    screen_num: usize,
    size: PhysicalSize,
) -> PhysicalPoint {
    let screen = &connection.setup().roots[screen_num];
    PhysicalPoint {
        x: (screen.width_in_pixels as f64 / 2.0 - size.width as f64 / 2.0) as i32,
        y: (screen.height_in_pixels as f64 / 2.0 - size.height as f64 / 2.0) as i32,
    }
}
