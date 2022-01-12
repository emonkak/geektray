use serde::de;
use serde::ser;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ffi::CString;
use std::fmt;
use std::ops::BitOr;
use std::str;
use x11rb::protocol::xproto;

use super::xkbcommon_sys as xkb;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KeyEvent {
    pub keysym: Keysym,
    pub modifiers: Modifiers,
    pub state: KeyState,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Keysym(pub xproto::Keysym);

impl fmt::Display for Keysym {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut buffer = [0u8; 256];
        let length = unsafe {
            xkb::xkb_keysym_get_name((self.0).into(), buffer.as_mut_ptr().cast(), buffer.len())
        };
        if length < 0 {
            f.write_str("<NO_SYMBOL>")?;
        } else {
            match str::from_utf8(&buffer[0..length as usize]) {
                Ok(s) => f.write_str(s)?,
                Err(_) => f.write_str("<UTF8_ERROR>")?,
            }
        }
        Ok(())
    }
}

impl From<xproto::Keysym> for Keysym {
    fn from(value: xproto::Keysym) -> Self {
        Self(value)
    }
}

impl Serialize for Keysym {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut buffer = [0u8; 256];
        let length = unsafe {
            xkb::xkb_keysym_get_name((self.0).into(), buffer.as_mut_ptr().cast(), buffer.len())
        };
        if length < 0 {
            return Err(ser::Error::custom(format!(
                "Keysym `{}` is not defined.",
                self.0
            )));
        }
        match str::from_utf8(&buffer[0..length as usize]) {
            Ok(s) => serializer.serialize_str(&s),
            Err(error) => Err(ser::Error::custom(error)),
        }
    }
}

impl<'de> Deserialize<'de> for Keysym {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let c_str = CString::new(s).map_err(de::Error::custom)?;
        let keysym = unsafe { xkb::xkb_keysym_from_name(c_str.as_ptr(), xkb::XKB_KEYSYM_NO_FLAGS) };
        if keysym == x11rb::NO_SYMBOL {
            return Err(de::Error::custom(format!(
                "String \"{}\" does not match a valid Keysym.",
                c_str.to_string_lossy()
            )));
        }
        Ok(Keysym(keysym))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyState {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Modifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    #[serde(rename = "super")]
    pub super_: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        control: false,
        shift: false,
        alt: false,
        super_: false,
        caps_lock: false,
        num_lock: false,
    };

    pub const CONTROL: Self = Self {
        control: true,
        shift: false,
        alt: false,
        super_: false,
        caps_lock: false,
        num_lock: false,
    };

    pub const SHIFT: Self = Self {
        control: false,
        shift: true,
        alt: false,
        super_: false,
        caps_lock: false,
        num_lock: false,
    };

    pub const ALT: Self = Self {
        control: false,
        shift: false,
        alt: true,
        super_: false,
        caps_lock: false,
        num_lock: false,
    };

    pub const SUPER: Self = Self {
        control: false,
        shift: false,
        alt: false,
        super_: true,
        caps_lock: false,
        num_lock: false,
    };

    pub const CAPS_LOCK: Self = Self {
        control: false,
        shift: false,
        alt: false,
        super_: false,
        caps_lock: true,
        num_lock: false,
    };

    pub const NUM_LOCK: Self = Self {
        control: false,
        shift: false,
        alt: false,
        super_: false,
        caps_lock: false,
        num_lock: true,
    };
}

impl Default for Modifiers {
    fn default() -> Self {
        Modifiers::NONE
    }
}

impl BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            control: self.control || rhs.control,
            shift: self.shift || rhs.shift,
            alt: self.alt || rhs.alt,
            super_: self.super_ || rhs.super_,
            caps_lock: self.caps_lock || rhs.caps_lock,
            num_lock: self.num_lock || rhs.num_lock,
        }
    }
}
