use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::BitOr;
use std::os::raw::*;
use x11::xlib;

use crate::command::Command;

type Keymask = c_uint;

#[derive(Debug)]
pub struct KeyInterpreter {
    command_table: HashMap<(Keysym, Modifiers), Vec<Command>>,
}

impl KeyInterpreter {
    pub fn new(key_mappings: Vec<KeyMapping>) -> Self {
        let mut command_table: HashMap<(Keysym, Modifiers), Vec<Command>> = HashMap::new();
        for key_mapping in key_mappings {
            command_table.insert(
                (key_mapping.key, key_mapping.modifiers),
                key_mapping.commands,
            );
        }
        Self { command_table }
    }

    pub fn eval(&self, keysym: Keysym, modifiers: Modifiers) -> Vec<Command> {
        self.command_table
            .get(&(keysym, modifiers))
            .map(|commands| commands.clone())
            .unwrap_or_default()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KeyMapping {
    key: Keysym,
    #[serde(default = "Modifiers::none")]
    modifiers: Modifiers,
    commands: Vec<Command>,
}

impl KeyMapping {
    pub const fn new(key: Keysym, modifiers: Modifiers, commands: Vec<Command>) -> Self {
        Self {
            key,
            modifiers,
            commands,
        }
    }

    pub const fn matches(&self, keysym: xlib::KeySym, keymask: Keymask) -> bool {
        self.key.0 == keysym && (keymask & Modifiers::all().keymask()) == self.modifiers.keymask()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub struct Keysym(#[serde(with = "keysym_serde")] pub xlib::KeySym);

impl Keysym {
    pub fn new(
        display: *mut xlib::Display,
        keycode: xlib::KeyCode,
        keymask: Keymask,
    ) -> Option<Self> {
        let keysym = unsafe {
            xlib::XkbKeycodeToKeysym(
                display,
                keycode,
                if keymask & xlib::ShiftMask != 0 { 1 } else { 0 },
                0,
            )
        };
        if keysym != xlib::NoSymbol as xlib::KeySym {
            Some(Keysym(keysym))
        } else {
            None
        }
    }
}

mod keysym_serde {
    use serde::de;
    use serde::ser;
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::ffi::{CStr, CString};
    use std::str;
    use x11::xlib;

    pub fn serialize<S>(value: &xlib::KeySym, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let c = unsafe { xlib::XKeysymToString(*value) };
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

    pub fn deserialize<'de, D>(deserializer: D) -> Result<xlib::KeySym, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let c_str = CString::new(s).map_err(de::Error::custom)?;
        let keysym = unsafe { xlib::XStringToKeysym(c_str.as_ptr()) };
        if keysym == xlib::NoSymbol as xlib::KeySym {
            return Err(de::Error::custom(format!(
                "The specified string `{}` does not match a valid Keysym.",
                c_str.to_string_lossy()
            )));
        }
        Ok(keysym)
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
    pub const fn new(keymask: Keymask) -> Self {
        let mut modifiers = Self::none();
        if (keymask & xlib::ControlMask) != 0 {
            modifiers.control = true;
        }
        if (keymask & xlib::ShiftMask) != 0 {
            modifiers.shift = true;
        }
        if (keymask & xlib::Mod1Mask) != 0 {
            modifiers.alt = true;
        }
        if (keymask & xlib::Mod4Mask) != 0 {
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

    pub const fn keymask(&self) -> Keymask {
        let mut keymask = 0;
        if self.control {
            keymask |= xlib::ControlMask;
        }
        if self.shift {
            keymask |= xlib::ShiftMask;
        }
        if self.alt {
            keymask |= xlib::Mod1Mask;
        }
        if self.super_ {
            keymask |= xlib::Mod4Mask;
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
