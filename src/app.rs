use nix;
use nix::sys::signal;
use nix::sys::signalfd;
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
use crate::event_loop::{run_event_loop, ControlFlow, Event, X11Event};
use crate::geometrics::{PhysicalPoint, PhysicalSize, Size};
use crate::styles::Styles;
use crate::text_renderer::TextRenderer;
use crate::tray::Tray;
use crate::tray_item::TrayItem;
use crate::utils;
use crate::widget::{Command, RenderContext, WidgetPod};
use crate::xembed::{XEmbedInfo, XEmbedMessage};

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
    window_position: PhysicalPoint,
    window_size: PhysicalSize,
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
        let window_position = unsafe {
            let screen_number = xlib::XDefaultScreen(display);
            let display_width = xlib::XDisplayWidth(display, screen_number) as f32;
            let display_height = xlib::XDisplayHeight(display, screen_number) as f32;
            PhysicalPoint {
                x: (display_width / 2.0 - window_size.width / 2.0) as i32,
                y: (display_height / 2.0 - window_size.height / 2.0) as i32,
            }
        };
        let window_size = window_size.snap();

        let window = unsafe { create_window(display, window_position, window_size) };

        Ok(Self {
            display,
            atoms,
            styles,
            tray,
            window,
            window_position,
            window_size,
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

        run_event_loop(self.display, &mut signal_fd, move |event| match event {
            Event::X11Event(event) => {
                let control_flow = match event {
                    X11Event::ClientMessage(event) => self.on_client_message(event),
                    X11Event::ConfigureNotify(event) => self.on_configure_notify(event),
                    X11Event::DestroyNotify(event) => self.on_destroy_notify(event),
                    X11Event::Expose(event) => self.on_expose(event),
                    X11Event::KeyRelease(event) => self.on_key_release(event),
                    X11Event::PropertyNotify(event) => self.on_property_notify(event),
                    X11Event::ReparentNotify(event) => self.on_reparent_notify(event),
                    _ => ControlFlow::Continue,
                };
                if event.window() == self.window {
                    let command = self.tray.on_event(self.display, self.window, &event);
                    self.handle_command(command);
                }
                control_flow
            }
            Event::Signal(_) => ControlFlow::Break,
        })?;

        Ok(())
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
                if let Some(xembed_info) =
                    unsafe { get_xembed_info(self.display, icon_window, &self.atoms) }
                {
                    let icon_title = unsafe {
                        get_window_title(self.display, icon_window, &self.atoms).unwrap_or_default()
                    };
                    let tray_item = WidgetPod::new(TrayItem::new(
                        icon_window,
                        icon_title,
                        xembed_info.is_mapped(),
                        self.styles.clone(),
                    ));
                    self.tray.widget.add_tray_item(tray_item);
                    unsafe {
                        request_embedding(
                            self.display,
                            icon_window,
                            self.window,
                            &xembed_info,
                            &self.atoms,
                        );
                        xlib::XFlush(self.display);
                    }
                    self.recaclulate_layout();
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

    fn on_configure_notify(&mut self, event: xlib::XConfigureEvent) -> ControlFlow {
        if self.window == event.window {
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
        ControlFlow::Continue
    }

    fn on_destroy_notify(&mut self, event: xlib::XDestroyWindowEvent) -> ControlFlow {
        if self.window == event.window {
            return ControlFlow::Break;
        }
        if let Some(_) = self.tray.widget.remove_tray_item(event.window) {
            self.recaclulate_layout();
        }
        ControlFlow::Continue
    }

    fn on_expose(&mut self, event: xlib::XExposeEvent) -> ControlFlow {
        if self.window == event.window && event.count == 0 {
            self.redraw();
        }
        ControlFlow::Continue
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
                self.request_redraw();
            }
            keysym::XK_Up | keysym::XK_k => {
                self.tray.widget.select_previous();
                self.request_redraw();
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

    fn on_property_notify(&mut self, event: xlib::XPropertyEvent) -> ControlFlow {
        if event.atom == self.atoms.XEMBED_INFO {
            let xembed_info = unsafe { get_xembed_info(self.display, event.window, &self.atoms) };
            match xembed_info {
                Some(xembed_info) if xembed_info.is_mapped() => {
                    if let Some(tray_item) = self.tray.widget.find_tray_item_mut(event.window) {
                        tray_item.widget.set_embedded(true);
                        unsafe {
                            begin_embedding(self.display, event.window, self.window);
                        }
                        self.request_redraw();
                    }
                }
                _ => {
                    if let Some(tray_item) = self.tray.widget.remove_tray_item(event.window) {
                        if tray_item.widget.is_embedded() {
                            unsafe {
                                release_embedding(self.display, event.window);
                            }
                        }
                        self.recaclulate_layout();
                    }
                }
            }
        } else if event.atom == self.atoms.NET_WM_NAME {
            if let Some(tray_item) = self.tray.widget.find_tray_item_mut(event.window) {
                let icon_title = unsafe {
                    get_window_title(self.display, event.window, &self.atoms).unwrap_or_default()
                };
                tray_item.widget.change_icon_title(icon_title);
            }
        }
        ControlFlow::Continue
    }

    fn on_reparent_notify(&mut self, event: xlib::XReparentEvent) -> ControlFlow {
        if event.parent != self.window {
            self.tray.widget.remove_tray_item(event.window);
        }
        ControlFlow::Continue
    }

    fn request_redraw(&mut self) {
        unsafe {
            xlib::XClearArea(self.display, self.window, 0, 0, 0, 0, xlib::True);
            xlib::XFlush(self.display);
        }
    }

    fn recaclulate_layout(&mut self) {
        let size = self.tray.layout(self.window_size.unsnap()).snap();

        unsafe {
            if self.window_size != size {
                let mut size_hints = mem::MaybeUninit::<xlib::XSizeHints>::zeroed().assume_init();
                size_hints.flags = xlib::PMinSize | xlib::PMaxSize;
                size_hints.min_height = size.height as c_int;
                size_hints.max_height = size.height as c_int;
                xlib::XSetWMSizeHints(self.display, self.window, &mut size_hints, xlib::XA_WM_NORMAL_HINTS);

                let x = self.window_position.x;
                let y = self.window_position.y - (((size.height as i32 - self.window_size.height as i32) / 2) as i32);

                xlib::XMoveResizeWindow(self.display, self.window, x, y, size.width, size.height);
            } else {
                // Request redraw
                xlib::XClearArea(self.display, self.window, 0, 0, 0, 0, xlib::True);
            }
        }
    }

    fn redraw(&mut self) {
        let mut context = RenderContext {
            text_renderer: &mut self.text_renderer,
        };
        self.tray.render(self.display, self.window, &mut context);
        unsafe {
            xlib::XFlush(self.display);
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::None => {}
            Command::RequestLayout => self.recaclulate_layout(),
            Command::RequestRedraw => self.request_redraw(),
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        for tray_item in self.tray.widget.tray_items() {
            if tray_item.widget.is_embedded() {
                unsafe {
                    release_embedding(self.display, tray_item.widget.icon_window());
                }
            }
        }

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

unsafe fn create_window(display: *mut xlib::Display, position: PhysicalPoint, size: PhysicalSize) -> xlib::Window {
    let screen = xlib::XDefaultScreenOfDisplay(display);
    let root = xlib::XRootWindowOfScreen(screen);

    let mut attributes: xlib::XSetWindowAttributes = mem::MaybeUninit::uninit().assume_init();
    attributes.backing_store = xlib::WhenMapped;
    attributes.bit_gravity = xlib::CenterGravity;

    let window = xlib::XCreateWindow(
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
        xlib::CWBackingStore | xlib::CWBitGravity,
        &mut attributes,
    );

    xlib::XSelectInput(
        display,
        window,
        xlib::KeyPressMask
            | xlib::ButtonPressMask
            | xlib::ButtonReleaseMask
            | xlib::EnterWindowMask
            | xlib::ExposureMask
            | xlib::FocusChangeMask
            | xlib::LeaveWindowMask
            | xlib::KeyReleaseMask
            | xlib::PropertyChangeMask
            | xlib::StructureNotifyMask,
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
    xlib::XSetSelectionOwner(display, net_system_tray_atom, window, xlib::CurrentTime);

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

unsafe fn request_embedding(
    display: *mut xlib::Display,
    icon_window: xlib::Window,
    embedder_window: xlib::Window,
    xembed_info: &XEmbedInfo,
    atoms: &Atoms,
) {
    send_embedded_notify(
        display,
        atoms,
        icon_window,
        embedder_window,
        xembed_info.version,
    );

    if xembed_info.is_mapped() {
        xlib::XSelectInput(
            display,
            icon_window,
            xlib::PropertyChangeMask | xlib::StructureNotifyMask,
        );
        xlib::XReparentWindow(display, icon_window, embedder_window, 0, 0);
    } else {
        xlib::XSelectInput(display, icon_window, xlib::PropertyChangeMask);
    }
}

unsafe fn begin_embedding(
    display: *mut xlib::Display,
    icon_window: xlib::Window,
    embedder_window: xlib::Window,
) {
    xlib::XSelectInput(
        display,
        icon_window,
        xlib::PropertyChangeMask | xlib::StructureNotifyMask,
    );
    xlib::XReparentWindow(display, icon_window, embedder_window, 0, 0);
}

unsafe fn release_embedding(display: *mut xlib::Display, icon_window: xlib::Window) {
    let screen = xlib::XDefaultScreenOfDisplay(display);
    let root = xlib::XRootWindowOfScreen(screen);

    xlib::XSelectInput(display, icon_window, xlib::NoEventMask);
    xlib::XReparentWindow(display, icon_window, root, 0, 0);
    xlib::XUnmapWindow(display, icon_window);
}

unsafe fn send_embedded_notify(
    display: *mut xlib::Display,
    atoms: &Atoms,
    icon_window: xlib::Window,
    embedder_window: xlib::Window,
    xembed_version: u64,
) {
    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, xlib::CurrentTime as c_long);
    data.set_long(1, XEmbedMessage::EmbeddedNotify as c_long);
    data.set_long(2, embedder_window as c_long);
    data.set_long(3, xembed_version as c_long);
    utils::send_client_message(display, icon_window, icon_window, atoms.XEMBED, data);
}

unsafe fn get_window_title(
    display: *mut xlib::Display,
    window: xlib::Window,
    atoms: &Atoms,
) -> Option<String> {
    let mut name_ptr: *mut i8 = ptr::null_mut();

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

unsafe fn get_xembed_info(
    display: *mut xlib::Display,
    window: xlib::Window,
    atoms: &Atoms,
) -> Option<XEmbedInfo> {
    utils::get_window_property::<_, 2>(display, window, atoms.XEMBED_INFO).map(|prop| XEmbedInfo {
        version: (*prop)[0],
        flags: (*prop)[1],
    })
}
