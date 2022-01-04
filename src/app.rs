use libdbus_sys as dbus;
use std::collections::VecDeque;
use std::env;
use std::error::Error;
use std::ffi::CString;
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use std::str::FromStr;
use x11::xlib;

use crate::atoms::Atoms;
use crate::command::Command;
use crate::config::Config;
use crate::effect::Effect;
use crate::error_handler;
use crate::event_loop::{ControlFlow, Event, EventLoop, X11Event};
use crate::geometrics::{PhysicalPoint, PhysicalSize, Point, Size};
use crate::key_mapping::{KeyInterpreter, Keysym, Modifiers};
use crate::render_context::RenderContext;
use crate::styles::Styles;
use crate::text::TextRenderer;
use crate::tray::Tray;
use crate::tray_manager::{TrayEvent, TrayManager};
use crate::utils;
use crate::widget::{LayoutResult, Widget};

#[derive(Debug)]
pub struct App {
    display: *mut xlib::Display,
    atoms: Rc<Atoms>,
    tray: Tray,
    window: xlib::Window,
    window_position: PhysicalPoint,
    window_size: PhysicalSize,
    window_mapped: bool,
    layout: LayoutResult,
    key_interpreter: KeyInterpreter,
    text_renderer: TextRenderer,
    old_error_handler:
        Option<unsafe extern "C" fn(*mut xlib::Display, *mut xlib::XErrorEvent) -> c_int>,
}

impl App {
    pub fn new(config: Config) -> Result<Self, String> {
        let old_error_handler = unsafe {
            xlib::XSetErrorHandler(if config.print_x11_errors {
                Some(error_handler::print_error)
            } else {
                Some(error_handler::ignore_error)
            })
        };

        let display = unsafe { xlib::XOpenDisplay(ptr::null()) };
        if display.is_null() {
            return Err(format!(
                "No display found at {}",
                env::var("DISPLAY").unwrap_or_default()
            ));
        }

        let atoms = Rc::new(Atoms::new(display));
        let styles = Rc::new(Styles::new(display, &config)?);
        let mut tray = Tray::new(styles);

        let layout = tray.layout(Size {
            width: config.ui.window_width,
            height: 0.0,
        });
        let window_size = layout.size.snap();
        let window_position = unsafe { get_centered_position_on_display(display, window_size) };
        let window =
            unsafe { create_window(display, window_position, window_size, &config, &atoms) };

        Ok(Self {
            display,
            atoms,
            tray,
            window,
            window_position,
            window_size,
            window_mapped: false,
            layout,
            key_interpreter: KeyInterpreter::new(config.keys),
            text_renderer: TextRenderer::new(),
            old_error_handler,
        })
    }

    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let mut tray_manager = TrayManager::new(self.display, self.window, self.atoms.clone());

        tray_manager.acquire_tray_selection();

        unsafe {
            xlib::XMapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }

        let mut event_loop = EventLoop::new(self.display)?;

        event_loop.run(|event, control_flow, context| match event {
            Event::X11Event(event) => {
                tray_manager.process_event(&event, |event| {
                    self.on_tray_event(event, context, control_flow);
                });
                if event.window() == self.window {
                    let effect = self.tray.on_event(&event, Point::default(), &self.layout);
                    self.apply_effect(effect);
                }
                self.on_x11_event(event, context, control_flow);
            }
            Event::DBusMessage(message) => {
                use dbus::DBusMessageType::*;

                match (
                    message.message_type(),
                    message.path().unwrap_or_default(),
                    message.member().unwrap_or_default(),
                ) {
                    (MethodCall, "/", command_str) => {
                        if let Ok(command) = Command::from_str(command_str) {
                            self.run_command(command);
                        }
                        context.send_dbus_message(&message.new_method_return());
                    }
                    _ => {}
                }
            }
            Event::Signal(_) => {
                *control_flow = ControlFlow::Break;
            }
        });

        tray_manager.release_tray_selection();

        Ok(())
    }

    fn on_tray_event(
        &mut self,
        event: TrayEvent,
        context: &mut EventLoop,
        control_flow: &mut ControlFlow,
    ) {
        match event {
            TrayEvent::BalloonMessageReceived(window, balloon_message) => {
                let summary = unsafe {
                    utils::get_window_title(self.display, window, self.atoms.NET_WM_NAME)
                }
                .and_then(|title| CString::new(title).ok())
                .unwrap_or_default();
                context.send_notification(
                    summary.as_c_str(),
                    balloon_message.as_c_str(),
                    balloon_message.id() as u32,
                    Some(balloon_message.timeout()),
                );
            }
            TrayEvent::TrayIconAdded(window) => {
                let title = unsafe {
                    utils::get_window_title(self.display, window, self.atoms.NET_WM_NAME)
                        .unwrap_or_default()
                };
                let effect = self.tray.add_tray_item(window, title);
                self.apply_effect(effect);
            }
            TrayEvent::TrayIconRemoved(window) => {
                let effect = self.tray.remove_tray_item(window);
                self.apply_effect(effect);
            }
            TrayEvent::SelectionCleared => {
                *control_flow = ControlFlow::Break;
            }
        }
    }

    fn on_x11_event(
        &mut self,
        event: X11Event,
        _context: &mut EventLoop,
        control_flow: &mut ControlFlow,
    ) {
        match event {
            X11Event::KeyRelease(event) => {
                if let Some(keysym) =
                    Keysym::new(self.display, event.keycode as xlib::KeyCode, event.state)
                {
                    let modifiers = Modifiers::new(event.state);
                    let commands = self.key_interpreter.eval(keysym, modifiers);
                    for command in commands {
                        if !self.run_command(command) {
                            break;
                        }
                    }
                }
            }
            X11Event::Expose(event) if event.window == self.window && event.count == 0 => {
                self.redraw();
            }
            X11Event::ConfigureNotify(event) if event.window == self.window => {
                self.window_position = PhysicalPoint {
                    x: event.x as i32,
                    y: event.y as i32,
                };
                let window_size = PhysicalSize {
                    width: event.width as u32,
                    height: event.height as u32,
                };
                if self.window_size != window_size {
                    self.window_size = window_size;
                    self.recaclulate_layout();
                }
            }
            X11Event::DestroyNotify(event) if event.window == self.window => {
                *control_flow = ControlFlow::Break;
            }
            X11Event::MapNotify(event) if event.window == self.window => {
                self.window_mapped = true;
            }
            X11Event::UnmapNotify(event) if event.window == self.window => {
                self.window_mapped = false;
            }
            X11Event::ClientMessage(event)
                if event.message_type == self.atoms.WM_PROTOCOLS && event.format == 32 =>
            {
                let protocol = event.data.get_long(0) as xlib::Atom;
                if protocol == self.atoms.NET_WM_PING {
                    unsafe {
                        let root = xlib::XDefaultRootWindow(self.display);
                        let mut reply_event = event;
                        reply_event.window = root;
                        xlib::XSendEvent(
                            self.display,
                            root,
                            xlib::False,
                            xlib::SubstructureNotifyMask | xlib::SubstructureRedirectMask,
                            &mut reply_event.into(),
                        );
                    }
                } else if protocol == self.atoms.NET_WM_SYNC_REQUEST {
                    self.request_redraw();
                } else if protocol == self.atoms.WM_DELETE_WINDOW {
                    self.hide_window();
                }
            }
            X11Event::PropertyNotify(event)
                if event.atom == xlib::XA_WM_NAME || event.atom == self.atoms.NET_WM_NAME =>
            {
                if self.tray.contains_window(event.window) {
                    let title = unsafe {
                        utils::get_window_title(self.display, event.window, self.atoms.NET_WM_NAME)
                            .unwrap_or_default()
                    };
                    let effect = self.tray.change_title(event.window, title);
                    self.apply_effect(effect);
                }
            }
            _ => {}
        }
    }

    fn show_window(&mut self) {
        unsafe {
            let window_position = get_centered_position_on_display(self.display, self.window_size);
            xlib::XMoveWindow(
                self.display,
                self.window,
                window_position.x,
                window_position.y,
            );
            xlib::XMapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }

    fn hide_window(&mut self) {
        let effect = self.tray.deselect_item();
        self.apply_effect(effect);
        unsafe {
            xlib::XUnmapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }

    fn toggle_window(&mut self) {
        if self.window_mapped {
            self.hide_window();
        } else {
            self.show_window();
        }
    }

    fn run_command(&mut self, command: Command) -> bool {
        match command {
            Command::HideWindow => {
                self.hide_window();
                true
            }
            Command::ShowWindow => {
                self.show_window();
                true
            }
            Command::ToggleWindow => {
                self.toggle_window();
                true
            }
            Command::SelectNextItem => {
                let effect = self.tray.select_next_item();
                self.apply_effect(effect)
            }
            Command::SelectPreviousItem => {
                let effect = self.tray.select_previous_item();
                self.apply_effect(effect)
            }
            Command::ClickLeftButton => {
                let effect = self.tray.click_selected_item(xlib::Button1, xlib::Button1Mask);
                self.apply_effect(effect)
            }
            Command::ClickRightButton => {
                let effect = self.tray.click_selected_item(xlib::Button3, xlib::Button3Mask);
                self.apply_effect(effect)
            }
            Command::ClickMiddleButton => {
                let effect = self.tray.click_selected_item(xlib::Button2, xlib::Button2Mask);
                self.apply_effect(effect)
            }
            Command::ClickX1Button => {
                let effect = self.tray.click_selected_item(xlib::Button4, xlib::Button4Mask);
                self.apply_effect(effect)
            }
            Command::ClickX2Button => {
                let effect = self.tray.click_selected_item(xlib::Button5, xlib::Button5Mask);
                self.apply_effect(effect)
            }
        }
    }

    fn apply_effect(&mut self, effect: Effect) -> bool {
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
                    action(self.display, self.window);
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
            self.recaclulate_layout();
        } else if redraw_requested {
            self.request_redraw();
        }

        result
    }

    fn recaclulate_layout(&mut self) {
        self.layout = self.tray.layout(self.window_size.unsnap());

        let size = self.layout.size.snap();

        unsafe {
            if self.window_size != size {
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

                let x = self.window_position.x;
                let y = self.window_position.y
                    - (((size.height as i32 - self.window_size.height as i32) / 2) as i32);

                xlib::XMoveResizeWindow(self.display, self.window, x, y, size.width, size.height);
            } else {
                self.request_redraw();
            }

            xlib::XFlush(self.display);
        }
    }

    fn request_redraw(&self) {
        unsafe {
            xlib::XClearArea(self.display, self.window, 0, 0, 0, 0, xlib::True);
            xlib::XFlush(self.display);
        }
    }

    fn redraw(&mut self) {
        let mut context = RenderContext::new(
            self.display,
            self.window,
            self.window_size,
            &mut self.text_renderer,
        );

        self.tray
            .render(Point::default(), &self.layout, &mut context);

        context.commit();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.text_renderer.clear_caches(self.display);

        unsafe {
            xlib::XDestroyWindow(self.display, self.window);
            xlib::XSync(self.display, xlib::True);
            xlib::XCloseDisplay(self.display);
            xlib::XSetErrorHandler(self.old_error_handler);
        }
    }
}

unsafe fn create_window(
    display: *mut xlib::Display,
    position: PhysicalPoint,
    size: PhysicalSize,
    config: &Config,
    atoms: &Atoms,
) -> xlib::Window {
    let window = {
        let screen = xlib::XDefaultScreenOfDisplay(display);
        let root = xlib::XRootWindowOfScreen(screen);

        let mut attributes: xlib::XSetWindowAttributes = mem::MaybeUninit::uninit().assume_init();
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

    {
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

    {
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

    {
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

    window
}

unsafe fn get_centered_position_on_display(
    display: *mut xlib::Display,
    window_size: PhysicalSize,
) -> PhysicalPoint {
    let screen_number = xlib::XDefaultScreen(display);
    let display_width = xlib::XDisplayWidth(display, screen_number);
    let display_height = xlib::XDisplayHeight(display, screen_number);
    PhysicalPoint {
        x: (display_width as f32 / 2.0 - window_size.width as f32 / 2.0) as i32,
        y: (display_height as f32 / 2.0 - window_size.height as f32 / 2.0) as i32,
    }
}
