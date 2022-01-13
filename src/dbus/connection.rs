use libdbus_sys as dbus_sys;
use std::error;
use std::ffi::CStr;
use std::fmt;
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::str;
use std::time::Duration;

use super::c_str_to_slice;
use super::message::Message;

#[derive(Debug)]
pub struct Connection {
    connection: *mut dbus_sys::DBusConnection,
}

impl Connection {
    pub fn new_session(name: &CStr) -> Result<Self, Error> {
        let mut error = Error::new();

        let connection = unsafe {
            dbus_sys::dbus_bus_get_private(dbus_sys::DBusBusType::Session, error.as_mut_ptr())
        };
        if error.is_set() {
            return Err(error);
        }

        unsafe {
            dbus_sys::dbus_bus_request_name(
                connection,
                name.as_ptr() as *const c_char,
                dbus_sys::DBUS_NAME_FLAG_REPLACE_EXISTING as c_uint,
                error.as_mut_ptr(),
            );
        }
        if error.is_set() {
            return Err(error);
        }

        Ok(Self { connection })
    }

    pub fn set_watch_functions(
        &self,
        add_function: dbus_sys::DBusAddWatchFunction,
        remove_function: dbus_sys::DBusRemoveWatchFunction,
        toggled_function: dbus_sys::DBusWatchToggledFunction,
        data: *mut c_void,
        free_data_function: dbus_sys::DBusFreeFunction,
    ) {
        unsafe {
            dbus_sys::dbus_connection_set_watch_functions(
                self.connection,
                add_function,
                remove_function,
                toggled_function,
                data,
                free_data_function,
            );
        }
    }

    pub fn read_write(&self, timeout: Option<Duration>) -> bool {
        let timeout = timeout.map_or(-1, |timeout| timeout.as_millis() as i32);
        let result = unsafe { dbus_sys::dbus_connection_read_write(self.connection, timeout) };
        result != 0
    }

    pub fn pop_message(&self) -> Option<Message> {
        let message = unsafe { dbus_sys::dbus_connection_pop_message(self.connection) };
        if !message.is_null() {
            Some(Message(message))
        } else {
            None
        }
    }

    pub fn send(&self, message: &Message, mut serial: Option<u32>) -> bool {
        unsafe {
            dbus_sys::dbus_connection_send(
                self.connection,
                message.0,
                serial
                    .as_mut()
                    .map(|serial| serial as *mut _)
                    .unwrap_or(ptr::null_mut()),
            ) != 0
        }
    }

    pub fn flush(&self) {
        unsafe {
            dbus_sys::dbus_connection_flush(self.connection);
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            dbus_sys::dbus_connection_close(self.connection);
        }
    }
}

pub struct Error {
    error: dbus_sys::DBusError,
}

impl Error {
    pub fn new() -> Self {
        unsafe {
            let mut error = mem::MaybeUninit::uninit();
            dbus_sys::dbus_error_init(error.as_mut_ptr());
            Self {
                error: error.assume_init(),
            }
        }
    }

    pub fn name(&self) -> Option<&str> {
        unsafe { c_str_to_slice(self.error.name) }
    }

    pub fn message(&self) -> Option<&str> {
        unsafe { c_str_to_slice(self.error.name) }
    }

    pub fn is_set(&self) -> bool {
        !self.error.name.is_null()
    }

    pub fn as_mut_ptr(&mut self) -> *mut dbus_sys::DBusError {
        &mut self.error
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Error")
            .field("name", &self.name())
            .field("message", &self.message())
            .finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some((name, message)) = self.message().zip(self.name()) {
            write!(f, "{} ({})", message, name)?;
        }
        Ok(())
    }
}

impl Drop for Error {
    fn drop(&mut self) {
        unsafe {
            dbus_sys::dbus_error_free(&mut self.error);
        }
    }
}

unsafe impl Send for Error {}

unsafe impl Sync for Error {}

impl error::Error for Error {}
