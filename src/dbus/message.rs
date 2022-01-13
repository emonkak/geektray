use libdbus_sys as dbus_sys;
use std::ffi::CStr;
use std::fmt;
use std::str;

use super::c_str_to_slice;

pub struct Message(pub *mut dbus_sys::DBusMessage);

impl Message {
    pub fn new_method_call(destination: &CStr, path: &CStr, iface: &CStr, method: &CStr) -> Self {
        let message = unsafe {
            dbus_sys::dbus_message_new_method_call(
                destination.as_ptr() as *const i8,
                path.as_ptr() as *const i8,
                iface.as_ptr() as *const i8,
                method.as_ptr() as *const i8,
            )
        };
        Self(message)
    }

    pub fn new_method_return(&self) -> Self {
        let message = unsafe { dbus_sys::dbus_message_new_method_return(self.0) };
        assert!(!message.is_null());
        Self(message)
    }

    pub fn message_type(&self) -> dbus_sys::DBusMessageType {
        match unsafe { dbus_sys::dbus_message_get_type(self.0) } {
            1 => dbus_sys::DBusMessageType::MethodCall,
            2 => dbus_sys::DBusMessageType::MethodReturn,
            3 => dbus_sys::DBusMessageType::Error,
            4 => dbus_sys::DBusMessageType::Signal,
            x => unreachable!("Invalid message type: {}", x),
        }
    }

    pub fn reply_serial(&self) -> u32 {
        unsafe { dbus_sys::dbus_message_get_reply_serial(self.0) }
    }

    pub fn serial(&self) -> u32 {
        unsafe { dbus_sys::dbus_message_get_serial(self.0) }
    }

    pub fn path(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus_sys::dbus_message_get_path(self.0)) }
    }

    pub fn interface(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus_sys::dbus_message_get_interface(self.0)) }
    }

    pub fn destination(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus_sys::dbus_message_get_destination(self.0)) }
    }

    pub fn member(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus_sys::dbus_message_get_member(self.0)) }
    }

    pub fn sender(&self) -> Option<&str> {
        unsafe { c_str_to_slice(dbus_sys::dbus_message_get_sender(self.0)) }
    }

    pub fn no_reply(&self) -> bool {
        unsafe { dbus_sys::dbus_message_get_no_reply(self.0) != 0 }
    }

    pub fn auto_start(&self) -> bool {
        unsafe { dbus_sys::dbus_message_get_auto_start(self.0) != 0 }
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Message")
            .field("message_type", &self.message_type())
            .field("reply_serial", &self.reply_serial())
            .field("serial", &self.serial())
            .field("path", &self.path())
            .field("interface", &self.interface())
            .field("destination", &self.destination())
            .field("member", &self.member())
            .field("sender", &self.sender())
            .field("no_reply", &self.no_reply())
            .field("auto_start", &self.auto_start())
            .finish()
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        unsafe {
            dbus_sys::dbus_message_unref(self.0);
        }
    }
}
