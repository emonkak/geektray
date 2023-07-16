use serde::de;
use serde::ser;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ffi::CString;
use std::fmt;
use std::ops::{BitOr, BitOrAssign};
use std::str;
use x11rb::protocol::xproto;

use crate::xkbcommon_sys as ffi;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Keysym(xproto::Keysym);

impl Keysym {
    pub fn get(&self) -> xproto::Keysym {
        self.0
    }
}

impl fmt::Display for Keysym {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut buffer = [0u8; 256];
        let length = unsafe {
            ffi::xkb_keysym_get_name((self.0).into(), buffer.as_mut_ptr().cast(), buffer.len())
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

impl From<Keysym> for xproto::Keysym {
    fn from(keysym: Keysym) -> Self {
        keysym.0
    }
}

impl Serialize for Keysym {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut buffer = [0u8; 256];
        let length = unsafe {
            ffi::xkb_keysym_get_name((self.0).into(), buffer.as_mut_ptr().cast(), buffer.len())
        };
        if length < 0 {
            return Err(ser::Error::custom(format!(
                "Keysym \"{}\" is not defined.",
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
        let keysym = unsafe { ffi::xkb_keysym_from_name(c_str.as_ptr(), ffi::XKB_KEYSYM_NO_FLAGS) };
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
    #[serde(skip_serializing_if = "is_false")]
    pub control: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub shift: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub alt: bool,
    #[serde(skip_serializing_if = "is_false", rename = "super")]
    pub super_: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub caps_lock: bool,
    #[serde(skip_serializing_if = "is_false")]
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

    pub fn without_locks(&self) -> Self {
        Self {
            control: self.control,
            shift: self.shift,
            alt: self.alt,
            super_: self.super_,
            caps_lock: false,
            num_lock: false,
        }
    }
}

impl From<u16> for Modifiers {
    fn from(mod_mask: u16) -> Self {
        let mut modifiers = Modifiers::NONE;
        if (mod_mask & u16::from(xproto::ModMask::CONTROL)) != 0 {
            modifiers |= Modifiers::CONTROL;
        }
        if (mod_mask & u16::from(xproto::ModMask::SHIFT)) != 0 {
            modifiers |= Modifiers::SHIFT;
        }
        if (mod_mask & u16::from(xproto::ModMask::M1)) != 0 {
            modifiers |= Modifiers::ALT;
        }
        if (mod_mask & u16::from(xproto::ModMask::M4)) != 0 {
            modifiers |= Modifiers::SUPER;
        }
        if (mod_mask & u16::from(xproto::ModMask::LOCK)) != 0 {
            modifiers |= Modifiers::CAPS_LOCK;
        }
        if (mod_mask & u16::from(xproto::ModMask::M2)) != 0 {
            modifiers |= Modifiers::NUM_LOCK;
        }
        modifiers
    }
}

impl From<Modifiers> for u16 {
    fn from(modifiers: Modifiers) -> Self {
        let mut mod_mask = 0;
        if modifiers.control {
            mod_mask |= u16::from(xproto::ModMask::CONTROL);
        }
        if modifiers.shift {
            mod_mask |= u16::from(xproto::ModMask::SHIFT);
        }
        if modifiers.alt {
            mod_mask |= u16::from(xproto::ModMask::M1);
        }
        if modifiers.super_ {
            mod_mask |= u16::from(xproto::ModMask::M4);
        }
        if modifiers.caps_lock {
            mod_mask |= u16::from(xproto::ModMask::LOCK);
        }
        if modifiers.num_lock {
            mod_mask |= u16::from(xproto::ModMask::M2);
        }
        mod_mask
    }
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

impl BitOrAssign for Modifiers {
    fn bitor_assign(&mut self, rhs: Self) {
        self.control |= rhs.control;
        self.shift |= rhs.shift;
        self.alt |= rhs.alt;
        self.super_ |= rhs.super_;
        self.caps_lock |= rhs.caps_lock;
        self.num_lock |= rhs.num_lock;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

fn is_false(value: &bool) -> bool {
    *value == false
}
