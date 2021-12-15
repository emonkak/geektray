use nix::sys::signal;
use nix::sys::signalfd;
use std::borrow::Borrow;
use std::cell::RefCell;
use std::env;
use std::ffi::CString;
use std::mem;
use std::os::raw::*;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::ptr;
use x11::xft;
use x11::xlib;
use x11::xrender;

use config::Config;
use error_handler;
use font::FontDescriptor;
use font::FontRenderer;
use font::FontSet;
use layout::Layout;
use layout::Layoutable;
use task::TaskScheduler;
use task::CallbackResult;
use utils;
use xembed::XEmbedInfo;
use xembed::XEmbedMessage;

pub struct Context {
    pub display: *mut xlib::Display,
    pub atoms: Atoms,
    pub icon_size: u32,
    pub window_width: u32,
    pub padding: u32,
    pub font_set: FontSet,
    pub font_renderer: FontRenderer,
    pub normal_background: Color,
    pub normal_foreground: Color,
    pub selected_background: Color,
    pub selected_foreground: Color,
    pub task_scheduler: TaskScheduler,
    old_error_handler: Option<unsafe extern "C" fn (*mut xlib::Display, *mut xlib::XErrorEvent) -> c_int>,
    signal_fd: RefCell<signalfd::SignalFd>,
}

pub enum Event {
    XEvent(xlib::XEvent),
    Signal(signalfd::siginfo),
}

impl Context {
    pub fn new(config: Config) -> Result<Self, String> {
        let signal_fd = {
            let mut mask = signalfd::SigSet::empty();
            mask.add(signal::Signal::SIGINT);
            mask.thread_block().unwrap();
            signalfd::SignalFd::new(&mask)
        }.map_err(|error| format!("Failed to initialize SignalFd: {}", error))?;
        let old_error_handler = unsafe {
            xlib::XSetErrorHandler(Some(error_handler::handle))
        };

        let display = unsafe { xlib::XOpenDisplay(ptr::null()) };
        if display.is_null() {
            return Err(format!(
                    "No display found at {}",
                    env::var("DISPLAY").unwrap_or_default()
                )
            );
        }

        let mut task_scheduler = TaskScheduler::new()
            .map_err(|error| format!("Failed to initialize TaskScheduler: {}", error))?;
        task_scheduler.watch(unsafe { xlib::XConnectionNumber(display) as RawFd })
            .map_err(|error| format!("Failed to register X connection file descriptor: {}", error))?;
        task_scheduler.watch(signal_fd.as_raw_fd())
            .map_err(|error| format!("Failed to register signal file descriptor: {}", error))?;

        Ok(Context {
            display,
            atoms: Atoms::new(display),
            icon_size: config.icon_size,
            window_width: config.window_width,
            padding: config.padding,
            font_set: FontSet::new(FontDescriptor {
                    family_name: config.font_family.clone(),
                    weight: config.font_weight,
                    style: config.font_style,
                    pixel_size: config.font_size,
                })
                .ok_or(format!("Failed to initialize `font_set`: {:?}", config.font_family))?,
            font_renderer: FontRenderer::new(),
            normal_background: Color::new(display, &config.normal_background)
                .ok_or(format!("Failed to parse `normal_background`: {:?}", config.normal_background))?,
            normal_foreground: Color::new(display, &config.normal_foreground)
                .ok_or(format!("Failed to parse `normal_foreground`: {:?}", config.normal_foreground))?,
            selected_background: Color::new(display, &config.selected_background)
                .ok_or(format!("Failed to parse `selected_background`: {:?}", config.selected_background))?,
            selected_foreground: Color::new(display, &config.selected_foreground)
                .ok_or(format!("Failed to parse `selected_foreground`: {:?}", config.selected_foreground))?,
            task_scheduler,
            old_error_handler,
            signal_fd: RefCell::new(signal_fd),
        })
    }

    pub fn wait_events<F: FnMut(Event) -> CallbackResult<T>, T>(&self, mut callback: F) -> T {
        let x11_fd = unsafe { xlib::XConnectionNumber(self.display) as RawFd };
        let signal_fd = self.signal_fd.borrow().as_raw_fd();

        let mut xevent: xlib::XEvent = unsafe { mem::MaybeUninit::uninit().assume_init() };

        self.task_scheduler.wait(-1, |fd| {
            if fd == x11_fd {
                let pendings = unsafe { xlib::XPending(self.display) };
                for _ in 0..pendings {
                    unsafe {
                        xlib::XNextEvent(self.display, &mut xevent);
                    }

                    return callback(Event::XEvent(xevent));
                }
            } else if fd == signal_fd {
                if let Ok(Some(signal)) = self.signal_fd.borrow_mut().read_signal() {
                    return callback(Event::Signal(signal));
                }
            }

            CallbackResult::Continue
        })
    }

    pub fn get_layout<T: Layoutable>(&self) -> Layout<T> {
        Layout::new(self.window_width, self.icon_size + self.padding * 2)
    }

    pub fn get_xembed_info(&self, window: xlib::Window) -> Option<XEmbedInfo> {
        utils::get_window_property::<_, 2>(self.display, window, self.atoms.XEMBED_INFO).map(|prop| {
            XEmbedInfo {
                version: (*prop)[0],
                flags: (*prop)[1],
            }
        })
    }

    pub fn get_window_title(&self, window: xlib::Window) -> Option<String> {
        let mut name_ptr: *mut i8 = ptr::null_mut();

        let result = unsafe { xlib::XFetchName(self.display, window, &mut name_ptr) };
        if result == xlib::True && !name_ptr.is_null() && unsafe { *name_ptr } != 0 {
            unsafe {
                CString::from_raw(name_ptr).into_string().ok()
            }
        } else {
            utils::get_window_property::<c_ulong, 1>(self.display, window, self.atoms.NET_WM_PID)
                .and_then(|prop| {
                    let pid = prop[0] as u32;
                    utils::get_process_name(pid as u32).ok()
                })
        }
    }

    pub fn send_embedded_notify(&self, window: xlib::Window, timestamp: xlib::Time, embedder_window: xlib::Window, version: u64) {
        let mut data = xlib::ClientMessageData::new();
        data.set_long(0, timestamp as c_long);
        data.set_long(1, XEmbedMessage::EmbeddedNotify as c_long);
        data.set_long(2, embedder_window as c_long);
        data.set_long(3, version as c_long);

        utils::send_client_message(
            self.display,
            window,
            window,
            self.atoms.XEMBED,
            data
        );
    }

    pub fn acquire_tray_selection(&self, tray_window: xlib::Window) -> xlib::Window {
        unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(self.display);
            let screen_number = xlib::XScreenNumberOfScreen(screen);
            let root = xlib::XRootWindowOfScreen(screen);
            let net_system_tray_atom = self.get_atom(format!("_NET_SYSTEM_TRAY_S{}\0", screen_number));

            let previous_selection_owner = xlib::XGetSelectionOwner(self.display, net_system_tray_atom);
            xlib::XSetSelectionOwner(self.display, net_system_tray_atom, tray_window, xlib::CurrentTime);

            let mut data = xlib::ClientMessageData::new();
            data.set_long(0, xlib::CurrentTime as c_long);
            data.set_long(1, net_system_tray_atom as c_long);
            data.set_long(2, tray_window as c_long);

            utils::send_client_message(
                self.display,
                root,
                root,
                self.atoms.MANAGER,
                data
            );

            previous_selection_owner
        }
    }

    pub fn release_tray_selection(&self, previous_selection_owner: xlib::Window) {
        unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(self.display);
            let screen_number = xlib::XScreenNumberOfScreen(screen);
            let net_system_tray_atom = self.get_atom(format!("_NET_SYSTEM_TRAY_S{}\0", screen_number));

            xlib::XSetSelectionOwner(self.display, net_system_tray_atom, previous_selection_owner, xlib::CurrentTime);
        }
    }

    #[inline]
    fn get_atom<T: Borrow<str>>(&self, null_terminated_name: T) -> xlib::Atom {
        utils::new_atom(self.display, null_terminated_name.borrow())
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            xlib::XSync(self.display, xlib::False);
            xlib::XCloseDisplay(self.display);
            xlib::XSetErrorHandler(self.old_error_handler);
        }
    }
}

#[derive(Debug)]
pub struct Color {
    color: xlib::XColor,
}

impl Color {
    pub fn new(display: *mut xlib::Display, color_spec: &str) -> Option<Self> {
        let color_spec_cstr = CString::new(color_spec).ok()?;
        unsafe {
            let screen_number = xlib::XDefaultScreen(display);
            let colormap = xlib::XDefaultColormap(display, screen_number);
            let mut color: xlib::XColor = mem::MaybeUninit::uninit().assume_init();

            if xlib::XParseColor(display, colormap, color_spec_cstr.as_ptr(), &mut color) == xlib::False {
                return None;
            }

            if xlib::XAllocColor(display, colormap, &mut color) == xlib::False {
                return None;
            }

            Some(Self { color })
        }
    }

    pub fn pixel(&self) -> c_ulong {
        self.color.pixel
    }

    pub fn xcolor(&self) -> xlib::XColor {
        self.color
    }

    pub fn xft_color(&self) -> xft::XftColor {
        xft::XftColor {
            color: xrender::XRenderColor {
                red: self.color.red,
                green: self.color.green,
                blue: self.color.blue,
                alpha: 0xffff,
            },
            pixel: self.color.pixel
        }
    }
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
    pub XEMBED: xlib:: Atom,
    pub XEMBED_INFO: xlib:: Atom,
}

impl Atoms {
    fn new(display: *mut xlib::Display) -> Self {
        Self {
            MANAGER: utils::new_atom(display, "MANAGER\0"),
            NET_SYSTEM_TRAY_MESSAGE_DATA: utils::new_atom(display, "_NET_SYSTEM_TRAY_MESSAGE_DATA\0"),
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
