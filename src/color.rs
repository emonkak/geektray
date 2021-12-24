use std::ffi::CString;
use std::mem;
use std::os::raw::*;
use x11::xft;
use x11::xlib;
use x11::xrender;

#[derive(Clone, Copy, Debug)]
pub struct Color {
    color: xlib::XColor,
}

impl Color {
    pub fn parse(display: *mut xlib::Display, color_spec: &str) -> Option<Self> {
        let color_spec_cstr = CString::new(color_spec).ok()?;
        unsafe {
            let screen_number = xlib::XDefaultScreen(display);
            let colormap = xlib::XDefaultColormap(display, screen_number);
            let mut color: xlib::XColor = mem::MaybeUninit::uninit().assume_init();

            if xlib::XParseColor(display, colormap, color_spec_cstr.as_ptr(), &mut color)
                == xlib::False
            {
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

    pub fn as_xft_color(&self) -> xft::XftColor {
        xft::XftColor {
            color: xrender::XRenderColor {
                red: self.color.red,
                green: self.color.green,
                blue: self.color.blue,
                alpha: 0xffff,
            },
            pixel: self.color.pixel,
        }
    }
}
