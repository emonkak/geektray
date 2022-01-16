use std::array;
use x11rb::errors::ReplyError;
use x11rb::protocol::xkb;
use x11rb::protocol::xkb::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

use super::xkbcommon_sys as ffi;
use crate::ui::{KeyEvent, KeyState, Keysym, Modifiers};

#[derive(Debug)]
pub struct State {
    state: *mut ffi::xkb_state,
    mod_indices: ModIndices,
    keymap: Keymap,
}

impl State {
    pub fn from_keymap(keymap: Keymap) -> Self {
        let state = unsafe { ffi::xkb_state_new(keymap.keymap) };
        let mod_indices = ModIndices::from_keymap(&keymap);
        Self {
            state,
            mod_indices,
            keymap,
        }
    }

    pub fn lookup_keycode(&self, keysym: Keysym) -> Option<u32> {
        let min_keycode = self.keymap.min_keycode();
        let max_keycode = self.keymap.max_keycode();

        for keycode in min_keycode..=max_keycode {
            if self.get_keysym(keycode) == keysym {
                return Some(keycode);
            }
        }

        None
    }

    pub fn get_keysym(&self, keycode: u32) -> Keysym {
        Keysym(unsafe { ffi::xkb_state_key_get_one_sym(self.state, keycode) })
    }

    pub fn key_event(&self, keycode: u32) -> KeyEvent {
        let keysym = self.get_keysym(keycode);
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
                ffi::xkb_state_mod_index_is_active(self.state, index, ffi::XKB_STATE_MODS_EFFECTIVE)
                    != 0
            };
            if is_active {
                acc | modifier
            } else {
                acc
            }
        });

        KeyEvent { keysym, modifiers }
    }

    pub fn update_key(&self, keycode: u32, state: KeyState) {
        unsafe {
            ffi::xkb_state_update_key(
                self.state,
                keycode,
                match state {
                    KeyState::Down => ffi::XKB_KEY_DOWN,
                    KeyState::Up => ffi::XKB_KEY_UP,
                },
            );
        }
    }

    pub fn update_mask(&self, event: &xkb::StateNotifyEvent) {
        unsafe {
            ffi::xkb_state_update_mask(
                self.state,
                event.base_mods as u32,
                event.latched_mods as u32,
                event.locked_mods as u32,
                event.base_group as u32,
                event.latched_group as u32,
                u32::from(event.locked_group),
            );
        }
    }
}

impl Clone for State {
    fn clone(&self) -> Self {
        Self {
            keymap: self.keymap.clone(),
            mod_indices: self.mod_indices.clone(),
            state: unsafe { ffi::xkb_state_ref(self.state) },
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        unsafe { ffi::xkb_state_unref(self.state) }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ModIndices {
    control: ffi::xkb_mod_index_t,
    shift: ffi::xkb_mod_index_t,
    alt: ffi::xkb_mod_index_t,
    super_: ffi::xkb_mod_index_t,
    caps_lock: ffi::xkb_mod_index_t,
    num_lock: ffi::xkb_mod_index_t,
}

impl ModIndices {
    fn from_keymap(keymap: &Keymap) -> Self {
        let mod_index = |name: &'static [u8]| unsafe {
            ffi::xkb_keymap_mod_get_index(keymap.keymap, name.as_ptr().cast())
        };
        Self {
            control: mod_index(ffi::XKB_MOD_NAME_CTRL),
            shift: mod_index(ffi::XKB_MOD_NAME_SHIFT),
            alt: mod_index(ffi::XKB_MOD_NAME_ALT),
            super_: mod_index(ffi::XKB_MOD_NAME_LOGO),
            caps_lock: mod_index(ffi::XKB_MOD_NAME_CAPS),
            num_lock: mod_index(ffi::XKB_MOD_NAME_NUM),
        }
    }
}

#[derive(Debug)]
pub struct Keymap {
    keymap: *mut ffi::xkb_keymap,
    context: Context,
}

impl Keymap {
    pub fn from_device(
        context: Context,
        connection: &XCBConnection,
        device_id: DeviceId,
    ) -> Option<Self> {
        let keymap = unsafe {
            ffi::xkb_x11_keymap_new_from_device(
                context.0,
                connection.get_raw_xcb_connection().cast(),
                device_id.0 as i32,
                ffi::XKB_KEYMAP_COMPILE_NO_FLAGS,
            )
        };
        if keymap.is_null() {
            None
        } else {
            Some(Self { keymap, context })
        }
    }

    pub fn min_keycode(&self) -> u32 {
        unsafe { ffi::xkb_keymap_min_keycode(self.keymap) }
    }

    pub fn max_keycode(&self) -> u32 {
        unsafe { ffi::xkb_keymap_max_keycode(self.keymap) }
    }
}

impl Clone for Keymap {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            keymap: unsafe { ffi::xkb_keymap_ref(self.keymap) },
        }
    }
}

impl Drop for Keymap {
    fn drop(&mut self) {
        unsafe {
            ffi::xkb_keymap_unref(self.keymap);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DeviceId(u8);

impl DeviceId {
    pub fn core_keyboard(connection: &XCBConnection) -> Result<Self, ReplyError> {
        let reply = connection
            .xkb_get_device_info(
                xkb::ID::USE_CORE_KBD.into(),
                0u16,
                false,
                0u8,
                0u8,
                xkb::LedClass::KBD_FEEDBACK_CLASS,
                1u16,
            )?
            .reply()?;
        Ok(Self(reply.device_id))
    }
}

#[derive(Debug)]
pub struct Context(*mut ffi::xkb_context);

impl Context {
    pub fn new() -> Self {
        Self(unsafe { ffi::xkb_context_new(ffi::XKB_CONTEXT_NO_FLAGS) })
    }
}

impl Clone for Context {
    fn clone(&self) -> Self {
        unsafe { Self(ffi::xkb_context_ref(self.0)) }
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            ffi::xkb_context_unref(self.0);
        }
    }
}
