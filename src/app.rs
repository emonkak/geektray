use nix::sys::signal;
use nix::sys::signalfd;
use nix;
use std::env;
use std::ffi::CString;
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;
use x11::keysym;
use x11::xlib;

use crate::atoms::Atoms;
use crate::config::Config;
use crate::error_handler;
use crate::event_loop::{run_event_loop, ControlFlow, Event};
use crate::geometrics::Size;
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
    window: xlib::Window,
    window_size: Size<u32>,
    text_renderer: TextRenderer,
    previous_selection_owner: Option<xlib::Window>,
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
        let window = unsafe { create_window(display, window_size) };

        Ok(Self {
            display,
            atoms,
            styles,
            tray,
            window,
            window_size: window_size.snap(),
            text_renderer: TextRenderer::new(),
            previous_selection_owner: None,
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

        unsafe {
            self.previous_selection_owner = Some(acquire_tray_selection(self.display, self.window));
            xlib::XMapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }

        run_event_loop(self.display, &mut signal_fd, move |event| {
            match event {
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
            }
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
        if self.window == event.window {
            return ControlFlow::Break;
        }
        if let Some(mut tray_item) = self.tray.widget.remove_item(event.window) {
            tray_item.widget.set_embedded(false);
            tray_item.unmount(self.display, self.window);
            self.perform_layout();
        }
        ControlFlow::Continue
    }

    fn on_expose(&mut self, event: xlib::XExposeEvent) -> ControlFlow {
        if event.count == 0 && self.window == event.window {
            self.perform_render();
        }
        ControlFlow::Continue
    }

    fn on_property_notify(&mut self, event: xlib::XPropertyEvent) -> ControlFlow {
        if event.atom == self.atoms.XEMBED_INFO {
            if let Some(tray_item) = self.tray.widget.find_item_mut(event.window) {
                match get_xembed_info(self.display, &self.atoms, event.window) {
                    Some(embed_info) if embed_info.is_mapped() => {
                        tray_item.widget.embed_icon(self.display, self.window, tray_item.bounds());
                    }
                    _ => {
                        if let Some(mut tray_item) = self.tray.widget.remove_item(event.window) {
                            tray_item.unmount(self.display, self.window);
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
            tray_item.unmount(self.display, self.window);
        }
        ControlFlow::Continue
    }

    fn on_configure_notify(&mut self, event: xlib::XConfigureEvent) -> ControlFlow {
        if self.window == event.window {
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

    fn perform_layout(&mut self) {
        let size = self.tray.layout(self.window_size.unsnap()).snap();

        if self.window_size != size {
            unsafe {
                xlib::XResizeWindow(self.display, self.window, size.width, size.height);
            }
        }
    }

    fn perform_render(&mut self) {
        let mut context = RenderContext {
            text_renderer: &mut self.text_renderer,
        };
        unsafe {
            self.tray.render(self.display, self.window, &mut context);
            xlib::XFlush(self.display);
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.tray.unmount(self.display, self.window);
        self.text_renderer.clear_caches(self.display);

        if let Some(previous_selection_owner) = self.previous_selection_owner.take() {
            unsafe {
                release_tray_selection(self.display, previous_selection_owner);
            }
        }

        unsafe {
            xlib::XDestroyWindow(self.display, self.window);
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

unsafe fn create_window(
    display: *mut xlib::Display,
    window_size: Size,
) -> xlib::Window {
    let screen = xlib::XDefaultScreenOfDisplay(display);
    let screen_number = xlib::XDefaultScreen(display);
    let root = xlib::XRootWindowOfScreen(screen);

    let display_width = xlib::XDisplayWidth(display, screen_number) as f32;
    let display_height = xlib::XDisplayHeight(display, screen_number) as f32;

    let x = (display_width / 2.0 - window_size.width / 2.0) as i32;
    let y = (display_height / 2.0 - window_size.height / 2.0) as i32;

    let mut attributes: xlib::XSetWindowAttributes =
        mem::MaybeUninit::uninit().assume_init();
    attributes.backing_store = xlib::WhenMapped;
    attributes.bit_gravity = xlib::CenterGravity;

    let window = xlib::XCreateWindow(
        display,
        root,
        x,
        y,
        window_size.width as u32,
        window_size.height as u32,
        0,
        xlib::CopyFromParent,
        xlib::InputOutput as u32,
        xlib::CopyFromParent as *mut xlib::Visual,
        xlib::CWBackingStore | xlib::CWBitGravity,
        &mut attributes,
    );

    xlib::XSelectInput(
        display,
        window,
        xlib::KeyPressMask
            | xlib::KeyReleaseMask
            | xlib::StructureNotifyMask
            | xlib::FocusChangeMask
            | xlib::PropertyChangeMask
            | xlib::ExposureMask,
    );

    window
}

unsafe fn acquire_tray_selection(
    display: *mut xlib::Display,
    window: xlib::Window,
) -> xlib::Window {
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
        window,
        xlib::CurrentTime,
    );

    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, xlib::CurrentTime as c_long);
    data.set_long(1, net_system_tray_atom as c_long);
    data.set_long(2, window as c_long);

    utils::send_client_message(display, root, root, manager_atom, data);

    previous_selection_owner
}

unsafe fn release_tray_selection(
    display: *mut xlib::Display,
    previous_selection_owner: xlib::Window,
) {
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
