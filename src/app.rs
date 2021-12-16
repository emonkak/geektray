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

use color::Color;
use config::Config;
use error_handler;
use event_loop::{run_event_loop, ControlFlow, Event};
use font::FontDescriptor;
use geometrics::Point;
use text_renderer::{FontSet, TextRenderer};
use tray::Tray;
use tray_item::TrayItem;
use utils;
use xembed::{XEmbedInfo, XEmbedMessage};

const SYSTEM_TRAY_REQUEST_DOCK: i64 = 0;
const SYSTEM_TRAY_BEGIN_MESSAGE: i64 = 1;
const SYSTEM_TRAY_CANCEL_MESSAGE: i64 = 2;

pub struct App {
    display: *mut xlib::Display,
    atoms: Atoms,
    styles: Rc<Styles>,
    tray: Tray,
    text_renderer: TextRenderer,
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
        let tray = Tray::new(display, &atoms, styles.clone());
        let previous_selection_owner = acquire_tray_selection(display, tray.window());

        Ok(App {
            display,
            atoms,
            styles,
            tray,
            text_renderer: TextRenderer::new(),
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

        self.tray.layout(Point::ZERO);
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
            keysym::XK_Down | keysym::XK_j => self.tray.select_next_icon(),
            keysym::XK_Up | keysym::XK_k => self.tray.select_previous_icon(),
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
                println!("SYSTEM_TRAY_REQUEST_DOCK");
                let icon_window = event.data.get_long(2) as xlib::Window;
                if let Some(embed_info) = get_xembed_info(self.display, &self.atoms, icon_window) {
                    println!("embed_info: {:?}", embed_info);
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
                    self.tray.layout(Point::ZERO);
                    // self.tray.render(&mut RenderContext {
                    //     text_renderer: &mut self.text_renderer,
                    // });
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
        self.tray.layout(Point::ZERO);
        self.tray.render(&mut RenderContext {
            text_renderer: &mut self.text_renderer,
        });
        ControlFlow::Continue
    }

    fn on_expose(&mut self, event: xlib::XExposeEvent) -> ControlFlow {
        println!("on_expose: {}", event.count);
        if event.count == 0 {
            self.tray.render(&mut RenderContext {
                text_renderer: &mut self.text_renderer,
            });
        }
        ControlFlow::Continue
    }

    fn on_property_notify(&mut self, event: xlib::XPropertyEvent) -> ControlFlow {
        if event.atom == self.atoms.XEMBED_INFO {
            println!("XEMBED_INFO");
            if let Some(tray_item) = self.tray.find_item_mut(event.window) {
                match get_xembed_info(self.display, &self.atoms, event.window) {
                    Some(embed_info) if embed_info.is_mapped() => {
                        tray_item.show_window();
                        // tray_item.render(&mut RenderContext {
                        //     text_renderer: &mut self.text_renderer,
                        // });
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
}

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
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
pub struct Atoms {
    pub MANAGER: xlib::Atom,
    pub NET_SYSTEM_TRAY_MESSAGE_DATA: xlib::Atom,
    pub NET_SYSTEM_TRAY_OPCODE: xlib::Atom,
    pub NET_WM_PID: xlib::Atom,
    pub WM_DELETE_WINDOW: xlib::Atom,
    pub WM_PING: xlib::Atom,
    pub WM_PROTOCOLS: xlib::Atom,
    pub WM_TAKE_FOCUS: xlib::Atom,
    pub XEMBED: xlib::Atom,
    pub XEMBED_INFO: xlib::Atom,
}

impl Atoms {
    fn new(display: *mut xlib::Display) -> Self {
        Self {
            MANAGER: utils::new_atom(display, "MANAGER\0"),
            NET_SYSTEM_TRAY_MESSAGE_DATA: utils::new_atom(
                display,
                "_NET_SYSTEM_TRAY_MESSAGE_DATA\0",
            ),
            NET_SYSTEM_TRAY_OPCODE: utils::new_atom(display, "_NET_SYSTEM_TRAY_OPCODE\0"),
            NET_WM_PID: utils::new_atom(display, "_NET_WM_PID\0"),
            WM_DELETE_WINDOW: utils::new_atom(display, "WM_DELETE_WINDOW\0"),
            WM_PING: utils::new_atom(display, "WM_PING\0"),
            WM_PROTOCOLS: utils::new_atom(display, "WM_PROTOCOLS\0"),
            WM_TAKE_FOCUS: utils::new_atom(display, "WM_TAKE_FOCUS\0"),
            XEMBED: utils::new_atom(display, "_XEMBED\0"),
            XEMBED_INFO: utils::new_atom(display, "_XEMBED_INFO\0"),
        }
    }
}

pub struct Styles {
    pub icon_size: f32,
    pub window_width: f32,
    pub padding: f32,
    pub font_set: FontSet,
    pub font_size: f32,
    pub normal_background: Color,
    pub normal_foreground: Color,
    pub selected_background: Color,
    pub selected_foreground: Color,
}

impl Styles {
    fn new(display: *mut xlib::Display, config: &Config) -> Result<Self, String> {
        Ok(Self {
            icon_size: config.icon_size,
            window_width: config.window_width,
            padding: config.padding,
            font_set: FontSet::new(FontDescriptor {
                family: config.font_family.clone(),
                style: config.font_style,
                weight: config.font_weight,
                stretch: config.font_stretch,
            })
            .ok_or(format!(
                "Failed to initialize `font_set`: {:?}",
                config.font_family
            ))?,
            font_size: config.font_size,
            normal_background: Color::new(display, &config.normal_background).ok_or(format!(
                "Failed to parse `normal_background`: {:?}",
                config.normal_background
            ))?,
            normal_foreground: Color::new(display, &config.normal_foreground).ok_or(format!(
                "Failed to parse `normal_foreground`: {:?}",
                config.normal_foreground
            ))?,
            selected_background: Color::new(display, &config.selected_background).ok_or(
                format!(
                    "Failed to parse `selected_background`: {:?}",
                    config.selected_background
                ),
            )?,
            selected_foreground: Color::new(display, &config.selected_foreground).ok_or(
                format!(
                    "Failed to parse `selected_foreground`: {:?}",
                    config.selected_foreground
                ),
            )?,
        })
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

    let result = unsafe { xlib::XFetchName(display, window, &mut name_ptr) };
    if result == xlib::True && !name_ptr.is_null() && unsafe { *name_ptr } != 0 {
        unsafe { CString::from_raw(name_ptr).into_string().ok() }
    } else {
        utils::get_window_property::<c_ulong, 1>(display, window, atoms.NET_WM_PID).and_then(
            |prop| {
                let pid = prop[0] as u32;
                utils::get_process_name(pid as u32).ok()
            },
        )
    }
}

fn get_xembed_info(
    display: *mut xlib::Display,
    atoms: &Atoms,
    window: xlib::Window,
) -> Option<XEmbedInfo> {
    utils::get_window_property::<_, 2>(display, window, atoms.XEMBED_INFO).map(|prop| XEmbedInfo {
        version: (*prop)[0],
        flags: (*prop)[1],
    })
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

    utils::send_client_message(display, window, window, atoms.XEMBED, data);

    unsafe {
        xlib::XFlush(display);
    }
}
