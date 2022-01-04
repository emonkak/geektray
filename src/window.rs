use std::collections::VecDeque;
use std::mem;
use std::ops::Add;
use std::os::raw::*;
use x11::xlib;

use crate::atoms::Atoms;
use crate::config::UiConfig;
use crate::event_loop::{ControlFlow, X11Event};
use crate::geometrics::{PhysicalPoint, PhysicalSize, Point, Size};
use crate::render_context::RenderContext;
use crate::text::TextRenderer;
use crate::utils;
use crate::widget::{Layout, Widget};

#[derive(Debug)]
pub struct Window<Widget> {
    widget: Widget,
    display: *mut xlib::Display,
    window: xlib::Window,
    position: PhysicalPoint,
    size: PhysicalSize,
    layout: Layout,
    text_renderer: TextRenderer,
    is_mapped: bool,
}

impl<Widget: self::Widget> Window<Widget> {
    pub fn new(
        widget: Widget,
        display: *mut xlib::Display,
        atoms: &Atoms,
        config: &UiConfig,
    ) -> Result<Self, String> {
        let layout = widget.layout(Size {
            width: config.window_width,
            height: 0.0,
        });

        let size = layout.size.snap();
        let position = unsafe { get_window_position(display, size) };

        let window = unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(display);
            let root = xlib::XRootWindowOfScreen(screen);

            let mut attributes: xlib::XSetWindowAttributes =
                mem::MaybeUninit::uninit().assume_init();
            attributes.backing_store = xlib::WhenMapped;
            attributes.bit_gravity = xlib::CenterGravity;
            attributes.event_mask = xlib::KeyPressMask
                | xlib::ButtonPressMask
                | xlib::ButtonReleaseMask
                | xlib::EnterWindowMask
                | xlib::ExposureMask
                | xlib::FocusChangeMask
                | xlib::LeaveWindowMask
                | xlib::KeyReleaseMask
                | xlib::PropertyChangeMask
                | xlib::StructureNotifyMask;

            xlib::XCreateWindow(
                display,
                root,
                position.x as i32,
                position.y as i32,
                size.width,
                size.height,
                0,
                xlib::CopyFromParent,
                xlib::InputOutput as u32,
                xlib::CopyFromParent as *mut xlib::Visual,
                xlib::CWBackingStore | xlib::CWBitGravity | xlib::CWEventMask,
                &mut attributes,
            )
        };

        unsafe {
            let mut protocol_atoms = [
                atoms.NET_WM_PING,
                atoms.NET_WM_SYNC_REQUEST,
                atoms.WM_DELETE_WINDOW,
            ];
            xlib::XSetWMProtocols(
                display,
                window,
                protocol_atoms.as_mut_ptr(),
                protocol_atoms.len() as i32,
            );
        }

        unsafe {
            let name_string = format!("{}\0", config.window_name.as_ref());
            let class_string = format!(
                "{}\0{}\0",
                config.window_class.as_ref(),
                config.window_class.as_ref()
            );

            let mut class_hint = mem::MaybeUninit::<xlib::XClassHint>::uninit().assume_init();
            class_hint.res_name = name_string.as_ptr() as *mut c_char;
            class_hint.res_class = class_string.as_ptr() as *mut c_char;

            xlib::XSetClassHint(display, window, &mut class_hint);
        }

        unsafe {
            utils::set_window_property(
                display,
                window,
                atoms.NET_WM_WINDOW_TYPE,
                xlib::XA_ATOM,
                &[atoms.NET_WM_WINDOW_TYPE_DIALOG],
            );

            utils::set_window_property(
                display,
                window,
                atoms.NET_WM_STATE,
                xlib::XA_ATOM,
                &[atoms.NET_WM_STATE_STICKY],
            );

            utils::set_window_property(
                display,
                window,
                atoms.NET_SYSTEM_TRAY_ORIENTATION,
                xlib::XA_CARDINAL,
                &[1], // _NET_SYSTEM_TRAY_ORIENTATION_VERT
            );
        }

        unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(display);
            let visual = xlib::XDefaultVisualOfScreen(screen);
            let visual_id = xlib::XVisualIDFromVisual(visual);
            utils::set_window_property(
                display,
                window,
                atoms.NET_SYSTEM_TRAY_VISUAL,
                xlib::XA_VISUALID,
                &[visual_id],
            );
        }

        Ok(Self {
            widget,
            display,
            window,
            position,
            size,
            layout,
            is_mapped: false,
            text_renderer: TextRenderer::new(),
        })
    }

    pub fn window(&self) -> xlib::Window {
        self.window
    }

    pub fn widget(&self) -> &Widget {
        &self.widget
    }

    pub fn widget_mut(&mut self) -> &mut Widget {
        &mut self.widget
    }

    pub fn show(&self) {
        unsafe {
            xlib::XMapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }

    pub fn move_at_center(&self) {
        unsafe {
            let position = get_window_position(self.display, self.size);
            xlib::XMoveWindow(self.display, self.window, position.x, position.y);
        }
    }

    pub fn hide(&self) {
        unsafe {
            xlib::XUnmapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }

    pub fn toggle(&self) {
        if self.is_mapped {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn request_redraw(&self) {
        unsafe {
            xlib::XClearArea(self.display, self.window, 0, 0, 0, 0, xlib::True);
            xlib::XFlush(self.display);
        }
    }

    pub fn recalculate_layout(&mut self) {
        self.layout = self.widget.layout(self.size.unsnap());
        let size = self.layout.size.snap();

        unsafe {
            if self.size != size {
                let mut size_hints = mem::MaybeUninit::<xlib::XSizeHints>::zeroed().assume_init();
                size_hints.flags = xlib::PMinSize | xlib::PMaxSize;
                size_hints.min_height = size.height as c_int;
                size_hints.max_height = size.height as c_int;

                xlib::XSetWMSizeHints(
                    self.display,
                    self.window,
                    &mut size_hints,
                    xlib::XA_WM_NORMAL_HINTS,
                );

                let x = self.position.x;
                let y =
                    self.position.y - (((size.height as i32 - self.size.height as i32) / 2) as i32);

                xlib::XMoveResizeWindow(self.display, self.window, x, y, size.width, size.height);
            } else {
                self.request_redraw();
            }

            xlib::XFlush(self.display);
        }
    }

    pub fn apply_effect(&mut self, effect: WindowEffcet) -> bool {
        let mut pending_effects = VecDeque::new();
        let mut current = effect;

        let mut redraw_requested = false;
        let mut layout_requested = false;
        let mut result = false;

        loop {
            match current {
                WindowEffcet::None => {}
                WindowEffcet::Batch(effects) => {
                    pending_effects.extend(effects);
                }
                WindowEffcet::Action(action) => {
                    action(self.display, self.window);
                    result = true;
                }
                WindowEffcet::RequestRedraw => {
                    redraw_requested = true;
                    result = true;
                }
                WindowEffcet::RequestLayout => {
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
            self.recalculate_layout();
        } else if redraw_requested {
            self.request_redraw();
        }

        result
    }

    pub fn on_event(&mut self, event: &X11Event, control_flow: &mut ControlFlow) {
        let effect = self.widget.on_event(event, Point::ZERO, &self.layout);
        self.apply_effect(effect);

        match event {
            X11Event::Expose(event) if event.count == 0 => {
                self.redraw();
            }
            X11Event::ConfigureNotify(event) => {
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
                    self.recalculate_layout();
                }
            }
            X11Event::DestroyNotify(_event) => {
                *control_flow = ControlFlow::Break;
            }
            X11Event::MapNotify(_event) => {
                self.is_mapped = true;
            }
            X11Event::UnmapNotify(_event) => {
                self.is_mapped = false;
            }
            _ => {}
        }
    }

    fn redraw(&mut self) {
        let mut context = RenderContext::new(
            self.display,
            self.window,
            self.size,
            &mut self.text_renderer,
        );

        self.widget.render(Point::ZERO, &self.layout, &mut context);

        context.commit();
    }
}

impl<Widget> Drop for Window<Widget> {
    fn drop(&mut self) {
        self.text_renderer.clear_caches(self.display);

        unsafe {
            xlib::XDestroyWindow(self.display, self.window);
        }
    }
}

unsafe fn get_window_position(display: *mut xlib::Display, size: PhysicalSize) -> PhysicalPoint {
    let screen_number = xlib::XDefaultScreen(display);
    let display_width = xlib::XDisplayWidth(display, screen_number);
    let display_height = xlib::XDisplayHeight(display, screen_number);
    PhysicalPoint {
        x: (display_width as f32 / 2.0 - size.width as f32 / 2.0) as i32,
        y: (display_height as f32 / 2.0 - size.height as f32 / 2.0) as i32,
    }
}

#[must_use]
pub enum WindowEffcet {
    None,
    Batch(Vec<WindowEffcet>),
    Action(Box<dyn FnOnce(*mut xlib::Display, xlib::Window)>),
    RequestRedraw,
    RequestLayout,
}

impl Add for WindowEffcet {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, y) => y,
            (x, Self::None) => x,
            (Self::Batch(mut xs), Self::Batch(ys)) => {
                xs.extend(ys);
                Self::Batch(xs)
            }
            (Self::Batch(mut xs), y) => {
                xs.push(y);
                Self::Batch(xs)
            }
            (x, Self::Batch(ys)) => {
                let mut xs = vec![x];
                xs.extend(ys);
                Self::Batch(xs)
            }
            (x, y) => Self::Batch(vec![x, y]),
        }
    }
}
