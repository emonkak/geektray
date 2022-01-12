use serde::de;
use serde::ser;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::ffi::CString;
use std::ops::BitOr;
use std::str;
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::ConnectionExt;
use x11rb::protocol::xproto;

use super::xkbcommon_sys as xkb;

#[derive(Debug)]
pub struct KeyboardMapping {
    min_keycode: u8,
    max_keycode: u8,
    keysyms_per_keycode: u8,
    keysyms: Vec<xproto::Keysym>,
}

impl KeyboardMapping {
    pub fn from_connection<Connection: self::Connection>(
        connection: &Connection,
    ) -> Result<Self, ReplyError> {
        let setup = &connection.setup();
        let reply = connection
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
            .reply()?;
        Ok(Self {
            min_keycode: setup.min_keycode,
            max_keycode: setup.max_keycode,
            keysyms_per_keycode: reply.keysyms_per_keycode,
            keysyms: reply.keysyms,
        })
    }

    pub fn get_key(&self, keycode: xproto::Keycode, level: u8) -> Option<Key> {
        if level >= self.keysyms_per_keycode
            || keycode < self.min_keycode
            || keycode > self.max_keycode
        {
            return None;
        }

        let index = level as usize
            + (keycode as usize - self.min_keycode as usize) * self.keysyms_per_keycode as usize;
        let keysym = self.keysyms[index];

        if keysym == x11rb::NO_SYMBOL {
            return None;
        }

        Some(Key(keysym))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Key(pub xproto::Keysym);

impl From<xproto::Keysym> for Key {
    fn from(value: xproto::Keysym) -> Self {
        Self(value)
    }
}

impl Serialize for Key {
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
                "The specified Keysym `{}` is not defined",
                self.0
            )));
        }
        match str::from_utf8(&buffer[0..length as usize]) {
            Ok(s) => serializer.serialize_str(&s),
            Err(error) => Err(ser::Error::custom(error)),
        }
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let c_str = CString::new(s).map_err(de::Error::custom)?;
        let keysym = unsafe { xkb::xkb_keysym_from_name(c_str.as_ptr(), xkb::XKB_KEYSYM_NO_FLAGS) };
        if keysym == x11rb::NO_SYMBOL {
            return Err(de::Error::custom(format!(
                "The specified string `{}` does not match a valid Keysym.",
                c_str.to_string_lossy()
            )));
        }
        Ok(Key(keysym))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(default = "Modifiers::none")]
pub struct Modifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    #[serde(rename = "super")]
    pub super_: bool,
}

impl Modifiers {
    pub fn from_keymask(keymask: impl Into<xproto::KeyButMask>) -> Self {
        let mut modifiers = Self::none();
        let keymask = u32::from(keymask.into());
        if (keymask & u32::from(xproto::KeyButMask::CONTROL)) != 0 {
            modifiers.control = true;
        }
        if (keymask & u32::from(xproto::KeyButMask::SHIFT)) != 0 {
            modifiers.shift = true;
        }
        if (keymask & u32::from(xproto::KeyButMask::MOD1)) != 0 {
            modifiers.alt = true;
        }
        if (keymask & u32::from(xproto::KeyButMask::MOD4)) != 0 {
            modifiers.super_ = true;
        }
        modifiers
    }

    pub const fn none() -> Self {
        Self {
            control: false,
            shift: false,
            alt: false,
            super_: false,
        }
    }

    pub const fn all() -> Self {
        Self {
            control: true,
            shift: true,
            alt: true,
            super_: true,
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

    pub fn keymask(&self) -> xproto::KeyButMask {
        let mut keymask = xproto::KeyButMask::from(0u16);
        if self.control {
            keymask = keymask | xproto::KeyButMask::CONTROL;
        }
        if self.shift {
            keymask = keymask | xproto::KeyButMask::SHIFT;
        }
        if self.alt {
            keymask = keymask | xproto::KeyButMask::MOD1;
        }
        if self.super_ {
            keymask = keymask | xproto::KeyButMask::MOD4;
        }
        keymask
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
        }
    }
}
