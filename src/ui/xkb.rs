use std::array;
use x11rb::errors::ReplyError;
use x11rb::protocol;
use x11rb::protocol::xkb::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

use super::xkbcommon_sys as xkb;
use crate::ui::{KeyEvent, KeyState, Keysym, Modifiers};

#[derive(Debug)]
pub struct State {
    state: *mut xkb::xkb_state,
    mod_indices: ModIndices,
    keymap: Keymap,
}

impl State {
    pub fn from_keymap(keymap: Keymap) -> Self {
        let state = unsafe { xkb::xkb_state_new(keymap.keymap) };
        let mod_indices = ModIndices::from_keymap(&keymap);
        Self {
            state,
            mod_indices,
            keymap,
        }
    }

    pub fn key_event(&self, keycode: u32, state: KeyState) -> KeyEvent {
        let keysym = Keysym(unsafe { xkb::xkb_state_key_get_one_sym(self.state, keycode) });
        let modifiers = array::IntoIter::new([
            (self.mod_indices.control, Modifiers::CONTROL),
            (self.mod_indices.alt, Modifiers::ALT),
            (self.mod_indices.shift, Modifiers::SHIFT),
            (self.mod_indices.super_, Modifiers::SUPER),
            (self.mod_indices.caps_lock, Modifiers::CAPS_LOCK),
            (self.mod_indices.num_lock, Modifiers::NUM_LOCK),
        ])
        .fold(Modifiers::NONE, |acc, (index, modifier)| {
            let is_active = unsafe {
                xkb::xkb_state_mod_index_is_active(self.state, index, xkb::XKB_STATE_MODS_EFFECTIVE)
                    != 0
            };
            if is_active {
                acc | modifier
            } else {
                acc
            }
        });

        KeyEvent {
            keysym,
            modifiers,
            state,
        }
    }

    pub fn update(&self, keycode: u32, state: KeyState) {
        unsafe {
            xkb::xkb_state_update_key(
                self.state,
                keycode,
                match state {
                    KeyState::Down => xkb::XKB_KEY_DOWN,
                    KeyState::Up => xkb::XKB_KEY_UP,
                },
            );
        }
    }
}

impl Clone for State {
    fn clone(&self) -> Self {
        Self {
            keymap: self.keymap.clone(),
            mod_indices: self.mod_indices.clone(),
            state: unsafe { xkb::xkb_state_ref(self.state) },
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        unsafe { xkb::xkb_state_unref(self.state) }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ModIndices {
    control: xkb::xkb_mod_index_t,
    shift: xkb::xkb_mod_index_t,
    alt: xkb::xkb_mod_index_t,
    super_: xkb::xkb_mod_index_t,
    caps_lock: xkb::xkb_mod_index_t,
    num_lock: xkb::xkb_mod_index_t,
}

impl ModIndices {
    fn from_keymap(keymap: &Keymap) -> Self {
        let mod_index = |name: &'static [u8]| unsafe {
            xkb::xkb_keymap_mod_get_index(keymap.keymap, name.as_ptr().cast())
        };
        Self {
            control: mod_index(xkb::XKB_MOD_NAME_CTRL),
            shift: mod_index(xkb::XKB_MOD_NAME_SHIFT),
            alt: mod_index(xkb::XKB_MOD_NAME_ALT),
            super_: mod_index(xkb::XKB_MOD_NAME_LOGO),
            caps_lock: mod_index(xkb::XKB_MOD_NAME_CAPS),
            num_lock: mod_index(xkb::XKB_MOD_NAME_NUM),
        }
    }
}

#[derive(Debug)]
pub struct Keymap {
    keymap: *mut xkb::xkb_keymap,
    context: Context,
}

impl Keymap {
    pub fn from_device(
        context: Context,
        connection: &XCBConnection,
        device_id: DeviceId,
    ) -> Option<Self> {
        let keymap = unsafe {
            xkb::xkb_x11_keymap_new_from_device(
                context.0,
                connection.get_raw_xcb_connection().cast(),
                device_id.0 as i32,
                xkb::XKB_KEYMAP_COMPILE_NO_FLAGS,
            )
        };
        if keymap.is_null() {
            None
        } else {
            Some(Self { keymap, context })
        }
    }
}

impl Clone for Keymap {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            keymap: unsafe { xkb::xkb_keymap_ref(self.keymap) },
        }
    }
}

impl Drop for Keymap {
    fn drop(&mut self) {
        unsafe {
            xkb::xkb_keymap_unref(self.keymap);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeviceId(u8);

impl DeviceId {
    pub fn core_keyboard(connection: &XCBConnection) -> Result<Self, ReplyError> {
        let reply = connection
            .xkb_get_device_info(
                protocol::xkb::ID::USE_CORE_KBD.into(),
                0u16,
                false,
                0u8,
                0u8,
                protocol::xkb::LedClass::KBD_FEEDBACK_CLASS,
                1u16,
            )?
            .reply()?;
        Ok(Self(reply.device_id))
    }
}

#[derive(Debug)]
pub struct Context(*mut xkb::xkb_context);

impl Context {
    pub fn new() -> Self {
        Self(unsafe { xkb::xkb_context_new(xkb::XKB_CONTEXT_NO_FLAGS) })
    }
}

impl Clone for Context {
    fn clone(&self) -> Self {
        unsafe { Self(xkb::xkb_context_ref(self.0)) }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            xkb::xkb_context_unref(self.0);
        }
    }
}
