use nix;
use nix::sys::signal;
use nix::sys::signalfd;
use std::env;
use std::ffi::CString;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use x11::keysym;
use x11::xlib;

use crate::atoms::Atoms;
use crate::config::Config;
use crate::error_handler;
use crate::event_loop::{run_event_loop, ControlFlow, Event};
use crate::geometrics::{Point, Size};
use crate::styles::Styles;
use crate::text_renderer::TextRenderer;
use crate::tray::Tray;
use crate::tray_item::TrayItem;
use crate::utils;
use crate::widget::WidgetPod;
use crate::xembed::XEmbedInfo;

const SYSTEM_TRAY_REQUEST_DOCK: i64 = 0;
const SYSTEM_TRAY_BEGIN_MESSAGE: i64 = 1;
const SYSTEM_TRAY_CANCEL_MESSAGE: i64 = 2;

#[derive(Debug)]
pub struct App {
    display: *mut xlib::Display,
    atoms: Rc<Atoms>,
    styles: Rc<Styles>,
    tray: WidgetPod<Tray>,
    window_size: Size<u32>,
    text_renderer: TextRenderer,
    old_error_handler:
        Option<unsafe extern "C" fn(*mut xlib::Display, *mut xlib::XErrorEvent) -> c_int>,
}

impl App {
    pub fn new(config: Config) -> Result<Self, String> {
        let old_error_handler = unsafe { xlib::XSetErrorHandler(Some(error_handler::handle)) };

        let display = unsafe { xlib::XOpenDisplay(ptr::null()) };
        if display.is_null() {
            return Err(format!(
                "No display found at {}",
                env::var("DISPLAY").unwrap_or_default()
            ));
        }

        let atoms = Rc::new(Atoms::new(display));
        let styles = Rc::new(Styles::new(display, &config)?);
        let mut tray = WidgetPod::new(Tray::new(styles.clone()));

        let window_size = tray.layout(Size {
            width: config.window_width,
            height: 0.0,
        });

        Ok(Self {
            display,
            atoms,
            styles,
            tray,
            window_size: window_size.snap(),
            text_renderer: TextRenderer::new(),
            old_error_handler,
        })
    }

    pub fn run(&mut self) -> nix::Result<()> {
        let mut signal_fd = {
            let mut mask = signalfd::SigSet::empty();
            mask.add(signal::Signal::SIGINT);
            mask.thread_block().unwrap();
            signalfd::SignalFd::new(&mask)
        }?;

        self.initialze_window();

        run_event_loop(self.display, &mut signal_fd, move |event| match event {
            Event::X11(event) => match event.get_type() {
                xlib::KeyRelease => self.on_key_release(xlib::XKeyEvent::from(event)),
                xlib::ClientMessage => {
                    self.on_client_message(xlib::XClientMessageEvent::from(event))
                }
                xlib::DestroyNotify => {
                    self.on_destroy_notify(xlib::XDestroyWindowEvent::from(event))
                }
                xlib::Expose => self.on_expose(xlib::XExposeEvent::from(event)),
                xlib::PropertyNotify => self.on_property_notify(xlib::XPropertyEvent::from(event)),
                xlib::ReparentNotify => self.on_reparent_notify(xlib::XReparentEvent::from(event)),
                xlib::ConfigureNotify => {
                    self.on_configure_notify(xlib::XConfigureEvent::from(event))
                }
                _ => ControlFlow::Continue,
            },
            Event::Signal(_) => ControlFlow::Break,
        })?;

        Ok(())
    }

    fn on_key_release(&mut self, event: xlib::XKeyEvent) -> ControlFlow {
        let keysym = unsafe {
            xlib::XkbKeycodeToKeysym(
                self.display,
                event.keycode as c_uchar,
                if event.state & xlib::ShiftMask != 0 {
                    1
                } else {
                    0
                },
                0,
            )
        };
        match keysym as c_uint {
            keysym::XK_Down | keysym::XK_j => {
                self.tray.widget.select_next();
                self.perform_render();
            }
            keysym::XK_Up | keysym::XK_k => {
                self.tray.widget.select_previous();
                self.perform_render();
            }
            keysym::XK_Right | keysym::XK_l => {
                self.tray
                    .widget
                    .click_selected_icon(self.display, xlib::Button1, xlib::Button1Mask)
            }
            keysym::XK_Left | keysym::XK_h => {
                self.tray
                    .widget
                    .click_selected_icon(self.display, xlib::Button3, xlib::Button3Mask)
            }
            _ => (),
        }
        ControlFlow::Continue
    }

    fn on_client_message(&mut self, event: xlib::XClientMessageEvent) -> ControlFlow {
        if event.message_type == self.atoms.WM_PROTOCOLS && event.format == 32 {
            let protocol = event.data.get_long(0) as xlib::Atom;
            if protocol == self.atoms.WM_DELETE_WINDOW {
                return ControlFlow::Break;
            }
        } else if event.message_type == self.atoms.NET_SYSTEM_TRAY_OPCODE {
            let opcode = event.data.get_long(1);
            if opcode == SYSTEM_TRAY_REQUEST_DOCK {
                let icon_window = event.data.get_long(2) as xlib::Window;
                if let Some(embed_info) = get_xembed_info(self.display, &self.atoms, icon_window) {
                    let icon_title = get_window_title(self.display, &self.atoms, icon_window)
                        .unwrap_or_default();
                    let tray_item = WidgetPod::new(TrayItem::new(
                        icon_window,
                        icon_title,
                        self.atoms.clone(),
                        self.styles.clone(),
                        embed_info.version,
                        embed_info.is_mapped(),
                    ));
                    self.tray.widget.add_item(tray_item);
                    self.perform_layout();
                }
            } else if opcode == SYSTEM_TRAY_BEGIN_MESSAGE {
                // TODO:
            } else if opcode == SYSTEM_TRAY_CANCEL_MESSAGE {
                // TODO:
            }
        } else if event.message_type == self.atoms.NET_SYSTEM_TRAY_MESSAGE_DATA {
            // TODO:
        }
        ControlFlow::Continue
    }

    fn on_destroy_notify(&mut self, event: xlib::XDestroyWindowEvent) -> ControlFlow {
        if self.tray.window() == Some(event.window) {
            return ControlFlow::Break;
        }
        if let Some(mut tray_item) = self.tray.widget.remove_item(event.window) {
            tray_item.widget.set_embedded(false);
            tray_item.finalize(self.display);
            self.perform_layout();
        }
        ControlFlow::Continue
    }

    fn on_expose(&mut self, event: xlib::XExposeEvent) -> ControlFlow {
        if event.count == 0 && self.tray.window() == Some(event.window) {
            self.perform_render();
        }
        ControlFlow::Continue
    }

    fn on_property_notify(&mut self, event: xlib::XPropertyEvent) -> ControlFlow {
        if event.atom == self.atoms.XEMBED_INFO {
            if let Some(tray_item) = self.tray.widget.find_item_mut(event.window) {
                match get_xembed_info(self.display, &self.atoms, event.window) {
                    Some(embed_info) if embed_info.is_mapped() => {
                        if let Some(window) = tray_item.window() {
                            tray_item.widget.embed_icon(self.display, window);
                        } else {
                            tray_item.widget.request_embed_icon();
                        }
                    }
                    _ => {
                        if let Some(mut tray_item) = self.tray.widget.remove_item(event.window) {
                            tray_item.finalize(self.display);
                        }
                        self.perform_layout();
                    }
                }
            }
        } else if event.atom == self.atoms.NET_WM_NAME {
            if let Some(tray_item) = self.tray.widget.find_item_mut(event.window) {
                let icon_title = get_window_title(self.display, &self.atoms, event.window).unwrap_or_default();
                tray_item.widget.change_icon_title(icon_title);
            }
        }
        ControlFlow::Continue
    }

    fn on_reparent_notify(&mut self, event: xlib::XReparentEvent) -> ControlFlow {
        if let Some(mut tray_item) = self.tray.widget.remove_item(event.window) {
            tray_item.finalize(self.display);
        }
        ControlFlow::Continue
    }

    fn on_configure_notify(&mut self, event: xlib::XConfigureEvent) -> ControlFlow {
        if self.tray.window() == Some(event.window) {
            let window_size = Size {
                width: event.width as u32,
                height: event.height as u32,
            };
            if self.window_size != window_size {
                self.window_size = window_size;
                self.perform_layout();
            }
        }
        ControlFlow::Continue
    }

    fn initialze_window(&mut self) {
        let window_size = self.tray.layout(self.window_size.unsnap());

        unsafe {
            let screen_number = xlib::XDefaultScreen(self.display);
            let display_width = xlib::XDisplayWidth(self.display, screen_number) as f32;
            let display_height = xlib::XDisplayHeight(self.display, screen_number) as f32;

            self.tray.reposition(Point {
                x: display_width / 2.0 - window_size.width / 2.0,
                y: display_height / 2.0 - window_size.height / 2.0,
            });
        }

        self.perform_render();
    }

    fn perform_layout(&mut self) {
        self.tray.layout(self.window_size.unsnap());
    }

    fn perform_render(&mut self) {
        let mut context = RenderContext {
            text_renderer: &mut self.text_renderer,
        };
        unsafe {
            let screen_number = xlib::XDefaultScreen(self.display);
            let root = xlib::XRootWindow(self.display, screen_number);
            self.tray.render(self.display, root, &mut context);
            xlib::XFlush(self.display);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.tray.finalize(self.display);
        self.text_renderer.clear_caches(self.display);

        unsafe {
            xlib::XSync(self.display, xlib::True);
            xlib::XCloseDisplay(self.display);
            xlib::XSetErrorHandler(self.old_error_handler);
        }
    }
}

pub struct RenderContext<'a> {
    pub text_renderer: &'a mut TextRenderer,
}

fn get_window_title(
    display: *mut xlib::Display,
    atoms: &Atoms,
    window: xlib::Window,
) -> Option<String> {
    let mut name_ptr: *mut i8 = ptr::null_mut();

    unsafe {
        let result = xlib::XFetchName(display, window, &mut name_ptr);
        if result == xlib::True && !name_ptr.is_null() && *name_ptr != 0 {
            CString::from_raw(name_ptr).into_string().ok()
        } else {
            utils::get_window_property::<c_ulong, 1>(display, window, atoms.NET_WM_PID).and_then(
                |prop| {
                    let pid = prop[0] as u32;
                    utils::get_process_name(pid as u32).ok()
                },
            )
        }
    }
}

fn get_xembed_info(
    display: *mut xlib::Display,
    atoms: &Atoms,
    window: xlib::Window,
) -> Option<XEmbedInfo> {
    unsafe {
        utils::get_window_property::<_, 2>(display, window, atoms.XEMBED_INFO).map(|prop| {
            XEmbedInfo {
                version: (*prop)[0],
                flags: (*prop)[1],
            }
        })
    }
}
