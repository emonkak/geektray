use libdbus_sys as dbus;
use std::env;
use std::error::Error;
use std::ffi::CString;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use std::str::FromStr;
use x11::xlib;

use crate::atoms::Atoms;
use crate::command::{Command, MouseButton};
use crate::config::{Config, UiConfig};
use crate::error_handler;
use crate::event_loop::{ControlFlow, Event, EventLoop, X11Event};
use crate::font::FontDescription;
use crate::key_mapping::{KeyInterpreter, Keysym, Modifiers};
use crate::tray_container::TrayContainer;
use crate::tray_manager::{TrayEvent, TrayManager};
use crate::utils;
use crate::window::Window;

#[derive(Debug)]
pub struct App {
    display: *mut xlib::Display,
    config: Rc<UiConfig>,
    atoms: Rc<Atoms>,
    key_interpreter: KeyInterpreter,
    old_error_handler:
        Option<unsafe extern "C" fn(*mut xlib::Display, *mut xlib::XErrorEvent) -> c_int>,
}

impl App {
    pub fn new(config: Config) -> Result<Self, Box<dyn Error>> {
        let old_error_handler = unsafe {
            xlib::XSetErrorHandler(if config.print_x11_errors {
                Some(error_handler::print_error)
            } else {
                Some(error_handler::ignore_error)
            })
        };

        let display = unsafe { xlib::XOpenDisplay(ptr::null()) };
        if display.is_null() {
            Err(format!(
                "No display found at {}",
                env::var("DISPLAY").unwrap_or_default()
            ))?;
        }

        Ok(Self {
            display,
            config: Rc::new(config.ui),
            atoms: Rc::new(Atoms::new(display)),
            key_interpreter: KeyInterpreter::new(config.keys),
            old_error_handler,
        })
    }

    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let font = FontDescription::new(
            self.config.font.family.clone(),
            self.config.font.style,
            self.config.font.weight.into(),
            self.config.font.stretch,
        );
        let tray_container = TrayContainer::new(self.config.clone(), Rc::new(font));

        let mut window = Window::new(tray_container, self.display, &self.atoms, &self.config)?;
        let mut tray_manager = TrayManager::new(self.display, window.window(), self.atoms.clone());
        let mut event_loop = EventLoop::new(self.display)?;

        tray_manager.acquire_tray_selection();

        window.show();

        event_loop.run(|event, control_flow, context| match event {
            Event::X11Event(event) => {
                self.on_x11_event(&event, &mut window, control_flow);
                tray_manager.process_event(&event, |event| {
                    self.on_tray_event(&event, &mut window, context, control_flow);
                });
                if event.window() == window.window() {
                    window.on_event(&event, control_flow);
                }
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
                            run_command(command, &mut window);
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

    fn on_x11_event(
        &mut self,
        event: &X11Event,
        window: &mut Window<TrayContainer>,
        _control_flow: &mut ControlFlow,
    ) {
        match event {
            X11Event::KeyRelease(event) => {
                if let Some(keysym) =
                    Keysym::new(self.display, event.keycode as xlib::KeyCode, event.state)
                {
                    let modifiers = Modifiers::new(event.state);
                    let commands = self.key_interpreter.eval(keysym, modifiers);
                    for command in commands {
                        if !run_command(*command, window) {
                            break;
                        }
                    }
                }
            }
            X11Event::PropertyNotify(event)
                if event.atom == xlib::XA_WM_NAME || event.atom == self.atoms.NET_WM_NAME =>
            {
                if window.widget().contains_window(event.window) {
                    let title = unsafe {
                        utils::get_window_title(self.display, event.window, self.atoms.NET_WM_NAME)
                            .unwrap_or_default()
                    };
                    let effect = window.widget_mut().change_title(event.window, title);
                    window.apply_effect(effect);
                }
            }
            X11Event::ClientMessage(event)
                if event.message_type == self.atoms.WM_PROTOCOLS && event.format == 32 =>
            {
                let protocol = event.data.get_long(0) as xlib::Atom;
                if protocol == self.atoms.NET_WM_PING {
                    unsafe {
                        let root = xlib::XDefaultRootWindow(self.display);
                        let mut reply_event = event.clone();
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
                    window.request_redraw();
                } else if protocol == self.atoms.WM_DELETE_WINDOW {
                    window.hide();
                }
            }
            _ => {}
        }
    }

    fn on_tray_event(
        &mut self,
        event: &TrayEvent,
        window: &mut Window<TrayContainer>,
        context: &mut EventLoop,
        control_flow: &mut ControlFlow,
    ) {
        match event {
            TrayEvent::BalloonMessageReceived(icon_window, balloon_message) => {
                let summary = unsafe {
                    utils::get_window_title(self.display, *icon_window, self.atoms.NET_WM_NAME)
                }
                .and_then(|title| CString::new(title).ok())
                .unwrap_or_default();
                context.send_notification(
                    summary.as_c_str(),
                    balloon_message.as_c_str(),
                    *icon_window as u32,
                    Some(balloon_message.timeout()),
                );
            }
            TrayEvent::TrayIconAdded(icon_window) => {
                let title = unsafe {
                    utils::get_window_title(self.display, *icon_window, self.atoms.NET_WM_NAME)
                        .unwrap_or_default()
                };
                let effect = window.widget_mut().add_tray_item(*icon_window, title);
                window.apply_effect(effect);
            }
            TrayEvent::TrayIconRemoved(icon_window) => {
                let effect = window.widget_mut().remove_tray_item(*icon_window);
                window.apply_effect(effect);
            }
            TrayEvent::SelectionCleared => {
                *control_flow = ControlFlow::Break;
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            xlib::XSync(self.display, xlib::True);
            xlib::XCloseDisplay(self.display);
            xlib::XSetErrorHandler(self.old_error_handler);
        }
    }
}

fn run_command(command: Command, window: &mut Window<TrayContainer>) -> bool {
    match command {
        Command::HideWindow => {
            window.hide();
            let effect = window.widget_mut().select_item(None);
            window.apply_effect(effect);
            true
        }
        Command::ShowWindow => {
            window.move_at_center();
            window.show();
            true
        }
        Command::ToggleWindow => {
            window.toggle();
            true
        }
        Command::SelectItem(index) => {
            let effect = window.widget_mut().select_item(Some(index));
            window.apply_effect(effect)
        }
        Command::SelectNextItem => {
            let effect = window.widget_mut().select_next_item();
            window.apply_effect(effect)
        }
        Command::SelectPreviousItem => {
            let effect = window.widget_mut().select_previous_item();
            window.apply_effect(effect)
        }
        Command::ClickMouseButton(button) => {
            let (button, button_mask) = match button {
                MouseButton::Left => (xlib::Button1, xlib::Button1Mask),
                MouseButton::Right => (xlib::Button3, xlib::Button3Mask),
                MouseButton::Middle => (xlib::Button2, xlib::Button2Mask),
                MouseButton::X1 => (xlib::Button4, xlib::Button4Mask),
                MouseButton::X2 => (xlib::Button5, xlib::Button5Mask),
            };
            let effect = window.widget_mut().click_selected_item(button, button_mask);
            window.apply_effect(effect)
        }
    }
}
