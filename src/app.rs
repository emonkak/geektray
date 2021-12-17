use nix;
use nix::sys::signal;
use nix::sys::signalfd;
use std::env;
use std::ffi::CString;
use std::mem::ManuallyDrop;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use x11::keysym;
use x11::xlib;

use crate::config::Config;
use crate::error_handler;
use crate::event_loop::{run_event_loop, ControlFlow, Event};
use crate::geometrics::Size;
use crate::styles::Styles;
use crate::text_renderer::TextRenderer;
use crate::tray::Tray;
use crate::tray_item::TrayItem;
use crate::utils;
use crate::xembed::{XEmbedInfo, XEmbedMessage};

const SYSTEM_TRAY_REQUEST_DOCK: i64 = 0;
const SYSTEM_TRAY_BEGIN_MESSAGE: i64 = 1;
const SYSTEM_TRAY_CANCEL_MESSAGE: i64 = 2;

#[derive(Debug)]
pub struct App {
    display: *mut xlib::Display,
    atoms: Atoms,
    styles: Rc<Styles>,
    tray: ManuallyDrop<Tray>,
    window_size: Size<u32>,
    text_renderer: ManuallyDrop<TextRenderer>,
    previous_selection_owner: xlib::Window,
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

        let atoms = Atoms::new(display);
        let styles = Rc::new(Styles::new(display, &config)?);
        let tray = Tray::new(display, styles.clone());
        let previous_selection_owner = acquire_tray_selection(display, tray.window());

        Ok(App {
            display,
            atoms,
            styles,
            window_size: Size {
                width: config.window_width,
                height: 0,
            },
            tray: ManuallyDrop::new(tray),
            text_renderer: ManuallyDrop::new(TextRenderer::new(display)),
            previous_selection_owner,
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

        self.request_layout();

        self.tray.show_window();

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
                self.tray.select_next_icon();
                self.request_redraw();
            }
            keysym::XK_Up | keysym::XK_k => {
                self.tray.select_previous_icon();
                self.request_redraw();
            }
            keysym::XK_Right | keysym::XK_l => self
                .tray
                .click_selected_icon(xlib::Button1, xlib::Button1Mask),
            keysym::XK_Left | keysym::XK_h => self
                .tray
                .click_selected_icon(xlib::Button3, xlib::Button3Mask),
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
                        .unwrap_or("<No Title>".to_owned());
                    let mut tray_item = TrayItem::new(
                        self.display,
                        self.tray.window(),
                        icon_window,
                        icon_title,
                        self.styles.clone(),
                    );
                    if embed_info.is_mapped() {
                        tray_item.show_window();
                    }
                    send_embedded_notify(
                        self.display,
                        &self.atoms,
                        tray_item.icon_window(),
                        xlib::CurrentTime,
                        tray_item.embedder_window(),
                        embed_info.version,
                    );
                    self.tray.add_item(tray_item);
                    self.request_layout();
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
        if event.window == self.tray.window() {
            return ControlFlow::Break;
        }
        if let Some(mut icon) = self.tray.remove_item(event.window) {
            icon.mark_as_destroyed();
        }
        self.request_layout();
        ControlFlow::Continue
    }

    fn on_expose(&mut self, event: xlib::XExposeEvent) -> ControlFlow {
        if event.count == 0 {
            self.request_redraw();
        }
        ControlFlow::Continue
    }

    fn on_property_notify(&mut self, event: xlib::XPropertyEvent) -> ControlFlow {
        if event.atom == self.atoms.XEMBED_INFO {
            if let Some(tray_item) = self.tray.find_item_mut(event.window) {
                match get_xembed_info(self.display, &self.atoms, event.window) {
                    Some(embed_info) if embed_info.is_mapped() => {
                        tray_item.show_window();
                    }
                    _ => {
                        self.tray.remove_item(event.window);
                    }
                }
            }
        }
        ControlFlow::Continue
    }

    fn on_reparent_notify(&mut self, event: xlib::XReparentEvent) -> ControlFlow {
        if let Some(tray_item) = self.tray.find_item(event.window) {
            if tray_item.embedder_window() != event.parent {
                self.tray.remove_item(event.window);
            }
        }
        ControlFlow::Continue
    }

    fn on_configure_notify(&mut self, event: xlib::XConfigureEvent) -> ControlFlow {
        if event.window == self.tray.window() {
            let window_size = Size {
                width: event.width as u32,
                height: event.height as u32,
            };
            if self.window_size != window_size {
                self.window_size = window_size;
                self.request_layout();
            }
        }
        ControlFlow::Continue
    }

    fn request_layout(&mut self) {
        self.tray.layout(self.window_size);
    }

    fn request_redraw(&mut self) {
        self.tray.render(&mut RenderContext {
            text_renderer: &mut self.text_renderer,
        });
    }
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.tray);
            ManuallyDrop::drop(&mut self.text_renderer);

            release_tray_selection(self.display, self.previous_selection_owner);

            xlib::XSync(self.display, xlib::False);
            xlib::XCloseDisplay(self.display);
            xlib::XSetErrorHandler(self.old_error_handler);
        }
    }
}

pub struct RenderContext<'a> {
    pub text_renderer: &'a mut TextRenderer,
}

#[allow(non_snake_case)]
#[derive(Debug)]
struct Atoms {
    pub NET_SYSTEM_TRAY_MESSAGE_DATA: xlib::Atom,
    pub NET_SYSTEM_TRAY_OPCODE: xlib::Atom,
    pub NET_WM_PID: xlib::Atom,
    pub WM_DELETE_WINDOW: xlib::Atom,
    pub WM_PROTOCOLS: xlib::Atom,
    pub XEMBED: xlib::Atom,
    pub XEMBED_INFO: xlib::Atom,
}

impl Atoms {
    fn new(display: *mut xlib::Display) -> Self {
        unsafe {
            Self {
                NET_SYSTEM_TRAY_MESSAGE_DATA: utils::new_atom(
                    display,
                    "_NET_SYSTEM_TRAY_MESSAGE_DATA\0",
                ),
                NET_SYSTEM_TRAY_OPCODE: utils::new_atom(display, "_NET_SYSTEM_TRAY_OPCODE\0"),
                NET_WM_PID: utils::new_atom(display, "_NET_WM_PID\0"),
                WM_DELETE_WINDOW: utils::new_atom(display, "WM_DELETE_WINDOW\0"),
                WM_PROTOCOLS: utils::new_atom(display, "WM_PROTOCOLS\0"),
                XEMBED: utils::new_atom(display, "_XEMBED\0"),
                XEMBED_INFO: utils::new_atom(display, "_XEMBED_INFO\0"),
            }
        }
    }
}

fn acquire_tray_selection(display: *mut xlib::Display, tray_window: xlib::Window) -> xlib::Window {
    unsafe {
        let screen = xlib::XDefaultScreenOfDisplay(display);
        let screen_number = xlib::XScreenNumberOfScreen(screen);
        let root = xlib::XRootWindowOfScreen(screen);
        let manager_atom = utils::new_atom(display, "MANAGER\0");
        let net_system_tray_atom =
            utils::new_atom(display, &format!("_NET_SYSTEM_TRAY_S{}\0", screen_number));

        let previous_selection_owner = xlib::XGetSelectionOwner(display, net_system_tray_atom);
        xlib::XSetSelectionOwner(
            display,
            net_system_tray_atom,
            tray_window,
            xlib::CurrentTime,
        );

        let mut data = xlib::ClientMessageData::new();
        data.set_long(0, xlib::CurrentTime as c_long);
        data.set_long(1, net_system_tray_atom as c_long);
        data.set_long(2, tray_window as c_long);

        utils::send_client_message(display, root, root, manager_atom, data);

        previous_selection_owner
    }
}

fn release_tray_selection(display: *mut xlib::Display, previous_selection_owner: xlib::Window) {
    unsafe {
        let screen = xlib::XDefaultScreenOfDisplay(display);
        let screen_number = xlib::XScreenNumberOfScreen(screen);
        let net_system_tray_atom =
            utils::new_atom(display, &format!("_NET_SYSTEM_TRAY_S{}\0", screen_number));

        xlib::XSetSelectionOwner(
            display,
            net_system_tray_atom,
            previous_selection_owner,
            xlib::CurrentTime,
        );
    }
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

fn send_embedded_notify(
    display: *mut xlib::Display,
    atoms: &Atoms,
    window: xlib::Window,
    timestamp: xlib::Time,
    embedder_window: xlib::Window,
    version: u64,
) {
    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, timestamp as c_long);
    data.set_long(1, XEmbedMessage::EmbeddedNotify as c_long);
    data.set_long(2, embedder_window as c_long);
    data.set_long(3, version as c_long);

    unsafe {
        utils::send_client_message(display, window, window, atoms.XEMBED, data);
        xlib::XFlush(display);
    }
}
