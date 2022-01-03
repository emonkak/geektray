use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::ops::Add;
use std::os::raw::*;
use x11::keysym;

use crate::font::{FontFamily, FontStretch, FontStyle, FontWeight};
use crate::command::Command;

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub window_name: Cow<'static, str>,
    pub window_class: Cow<'static, str>,
    pub ui: UiConfig,
    pub font: FontConfig,
    pub color: ColorConfig,
    pub keys: Vec<KeyConfig>,
    pub print_x11_errors: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window_name: Cow::Borrowed("KeyTray"),
            window_class: Cow::Borrowed("KeyTray"),
            ui: UiConfig::default(),
            font: FontConfig::default(),
            color: ColorConfig::default(),
            keys: vec![
                KeyConfig {
                    key: Keysym(keysym::XK_j),
                    commands: vec![Command::SelectNextItem],
                    modifiers: Modifiers::none(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_Down),
                    commands: vec![Command::SelectNextItem],
                    modifiers: Modifiers::none(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_n),
                    commands: vec![Command::SelectNextItem],
                    modifiers: Modifiers::control(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_k),
                    commands: vec![Command::SelectPreviousItem],
                    modifiers: Modifiers::none(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_Down),
                    commands: vec![Command::SelectPreviousItem],
                    modifiers: Modifiers::none(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_p),
                    commands: vec![Command::SelectPreviousItem],
                    modifiers: Modifiers::control(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_l),
                    commands: vec![Command::ClickLeftButton],
                    modifiers: Modifiers::control(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_Return),
                    commands: vec![Command::ClickLeftButton],
                    modifiers: Modifiers::none(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_Return),
                    commands: vec![Command::ClickLeftButton],
                    modifiers: Modifiers::none(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_h),
                    commands: vec![Command::ClickRightButton],
                    modifiers: Modifiers::none(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_Return),
                    commands: vec![Command::ClickRightButton],
                    modifiers: Modifiers::shift(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_q),
                    commands: vec![Command::HideWindow],
                    modifiers: Modifiers::none(),
                },
                KeyConfig {
                    key: Keysym(keysym::XK_Escape),
                    commands: vec![Command::HideWindow],
                    modifiers: Modifiers::none(),
                },
            ],
            print_x11_errors: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct UiConfig {
    pub window_padding: f32,
    pub window_width: f32,
    pub item_gap: f32,
    pub icon_size: f32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_padding: 8.0,
            window_width: 480.0,
            item_gap: 8.0,
            icon_size: 24.0,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct FontConfig {
    pub family: FontFamily,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub stretch: FontStretch,
    pub size: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: FontFamily::default(),
            weight: FontWeight::default(),
            style: FontStyle::default(),
            stretch: FontStretch::default(),
            size: 12.0,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ColorConfig {
    pub window_background: String,
    pub normal_item_background: String,
    pub normal_item_foreground: String,
    pub selected_item_background: String,
    pub selected_item_foreground: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            window_background: "#21272b".to_owned(),
            normal_item_background: "#21272b".to_owned(),
            normal_item_foreground: "#e8eaeb".to_owned(),
            selected_item_background: "#1c95e6".to_owned(),
            selected_item_foreground: "#e8eaeb".to_owned(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KeyConfig {
    key: Keysym,
    commands: Vec<Command>,
    #[serde(default = "Modifiers::none")]
    modifiers: Modifiers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Keysym(#[serde(with = "keysym_serde")] c_uint);

mod keysym_serde {
    use serde::de;
    use serde::ser;
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::ffi::{CStr, CString};
    use std::os::raw::*;
    use std::str;
    use x11::xlib;

    pub fn serialize<S>(value: &c_uint, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let c = unsafe { xlib::XKeysymToString(*value as c_ulong) };
        if c.is_null() {
            return Err(ser::Error::custom(format!(
                "The specified Keysym `{}` is not defined",
                value
            )));
        }
        match str::from_utf8(unsafe { CStr::from_ptr(c) }.to_bytes()) {
            Ok(s) => serializer.serialize_str(&s),
            Err(error) => Err(ser::Error::custom(error)),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<c_uint, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let c_str = CString::new(s).map_err(de::Error::custom)?;
        let keysym = unsafe { xlib::XStringToKeysym(c_str.as_ptr()) };
        if keysym == xlib::NoSymbol as c_ulong {
            return Err(de::Error::custom(format!(
                "The specified string `{}` does not match a valid Keysym.",
                c_str.to_string_lossy()
            )));
        }
        Ok(keysym as c_uint)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Modifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    #[serde(rename = "super")]
    pub super_: bool,
}

impl Modifiers {
    pub const fn none() -> Self {
        Self {
            control: false,
            shift: false,
            alt: false,
            super_: false,
        }
    }

    pub const fn control() -> Self {
        Self {
            control: true,
            shift: false,
            alt: false,
            super_: false,
        }
    }

    pub const fn shift() -> Self {
        Self {
            control: false,
            shift: true,
            alt: false,
            super_: false,
        }
    }

    pub const fn alt() -> Self {
        Self {
            control: false,
            shift: false,
            alt: true,
            super_: false,
        }
    }

    pub const fn super_() -> Self {
        Self {
            control: false,
            shift: false,
            alt: false,
            super_: true,
        }
    }
}

impl Default for Modifiers {
    fn default() -> Self {
        Self::none()
    }
}

impl Add for Modifiers {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            control: self.control || rhs.control,
            shift: self.shift || rhs.shift,
            alt: self.alt || rhs.alt,
            super_: self.super_ || rhs.super_,
        }
    }
}
