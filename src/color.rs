use std::ffi::CString;
use std::mem;
use std::os::raw::*;
use x11::xft;
use x11::xlib;
use x11::xrender;

#[derive(Clone, Copy, Debug)]
pub struct Color {
    x_color: xlib::XColor,
}

impl Color {
    pub fn parse(display: *mut xlib::Display, color_spec: &str) -> Option<Self> {
        let color_spec_cstr = CString::new(color_spec).ok()?;
        unsafe {
            let screen_number = xlib::XDefaultScreen(display);
            let colormap = xlib::XDefaultColormap(display, screen_number);
            let mut x_color: xlib::XColor = mem::MaybeUninit::uninit().assume_init();

            if xlib::XParseColor(display, colormap, color_spec_cstr.as_ptr(), &mut x_color)
                == xlib::False
            {
                return None;
            }

            if xlib::XAllocColor(display, colormap, &mut x_color) == xlib::False {
                return None;
            }

            Some(Self { x_color })
        }
    }

    pub fn pixel(&self) -> c_ulong {
        self.x_color.pixel
    }

    pub fn into_xft_color(self) -> xft::XftColor {
        xft::XftColor {
            color: xrender::XRenderColor {
                red: self.x_color.red,
                green: self.x_color.green,
                blue: self.x_color.blue,
                alpha: 0xffff,
            },
            pixel: self.x_color.pixel,
        }
    }

    pub fn into_f64_components(self) -> [f64; 3] {
        [
            self.x_color.red as f64 / c_ushort::MAX as f64,
            self.x_color.green as f64 / c_ushort::MAX as f64,
            self.x_color.blue as f64 / c_ushort::MAX as f64,
        ]
    }
}
