use libdbus_sys as dbus_sys;
use std::ffi::CStr;
use std::mem;
use std::os::raw::*;
use std::ptr;

use super::message::Message;
use super::types::{ArgType, Argument, Signature, Variant};

pub struct MessageWriter<'a> {
    iter: dbus_sys::DBusMessageIter,
    message: &'a Message,
}

impl<'a> MessageWriter<'a> {
    pub fn from_message(message: &'a Message) -> Self {
        let iter = unsafe {
            let mut iter = mem::MaybeUninit::uninit();
            dbus_sys::dbus_message_iter_init_append(message.0, iter.as_mut_ptr());
            iter.assume_init()
        };
        Self { message, iter }
    }

    pub fn append<T: Writable>(&mut self, value: T) {
        value.write(self);
    }

    fn append_basic<T>(&mut self, value: &T)
    where
        T: Argument,
    {
        unsafe {
            dbus_sys::dbus_message_iter_append_basic(
                &mut self.iter,
                T::arg_type().into(),
                value as *const T as *const c_void,
            );
        }
    }

    fn append_array<T, I>(&mut self, elements: I)
    where
        T: Writable + Argument,
        I: Iterator<Item = T>,
    {
        let mut array_writer = unsafe {
            let mut array_iter = mem::MaybeUninit::uninit();
            dbus_sys::dbus_message_iter_open_container(
                &mut self.iter,
                ArgType::Array.into(),
                T::signature().to_string().as_ptr() as *const i8,
                array_iter.as_mut_ptr(),
            );
            MessageWriter {
                iter: array_iter.assume_init(),
                message: self.message,
            }
        };

        for element in elements {
            element.write(&mut array_writer);
        }

        unsafe {
            dbus_sys::dbus_message_iter_close_container(&mut self.iter, &mut array_writer.iter);
        }
    }

    fn append_dict_entry<K, V>(&mut self, key: &K, value: &V)
    where
        K: Writable,
        V: Writable,
    {
        let mut entry_writer = unsafe {
            let mut entry_iter = mem::MaybeUninit::uninit();
            dbus_sys::dbus_message_iter_open_container(
                &mut self.iter,
                ArgType::DictEntry.into(),
                ptr::null(),
                entry_iter.as_mut_ptr(),
            );
            MessageWriter {
                iter: entry_iter.assume_init(),
                message: self.message,
            }
        };

        key.write(&mut entry_writer);
        value.write(&mut entry_writer);

        unsafe {
            dbus_sys::dbus_message_iter_close_container(&mut self.iter, &mut entry_writer.iter);
        }
    }

    fn append_signature(&mut self, signature: &Signature) {
        unsafe {
            dbus_sys::dbus_message_iter_append_basic(
                &mut self.iter,
                ArgType::Signature.into(),
                &signature.to_string().as_ptr() as *const _ as *const c_void,
            );
        }
    }

    fn append_variant<T>(&mut self, value: &T)
    where
        T: Writable + Argument,
    {
        let mut variant_writer = unsafe {
            let mut variant_iter = mem::MaybeUninit::uninit();
            dbus_sys::dbus_message_iter_open_container(
                &mut self.iter,
                ArgType::Variant.into(),
                T::signature().to_string().as_ptr() as *const i8,
                variant_iter.as_mut_ptr(),
            );
            MessageWriter {
                iter: variant_iter.assume_init(),
                message: self.message,
            }
        };

        value.write(&mut variant_writer);

        unsafe {
            dbus_sys::dbus_message_iter_close_container(&mut self.iter, &mut variant_writer.iter);
        }
    }

    fn append_unit(&mut self) {
        let mut unit_iter = unsafe {
            let mut unit_iter = mem::MaybeUninit::uninit();
            dbus_sys::dbus_message_iter_open_container(
                &mut self.iter,
                ArgType::Struct.into(),
                ptr::null(),
                unit_iter.as_mut_ptr(),
            );
            unit_iter.assume_init()
        };

        unsafe {
            dbus_sys::dbus_message_iter_close_container(&mut self.iter, &mut unit_iter);
        }
    }
}

pub trait Writable {
    fn write(&self, writer: &mut MessageWriter);
}

impl Writable for bool {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(self)
    }
}

impl Writable for u8 {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(self)
    }
}

impl Writable for i16 {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(self)
    }
}

impl Writable for u16 {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(self)
    }
}

impl Writable for i32 {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(self)
    }
}

impl Writable for u32 {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(self)
    }
}

impl Writable for i64 {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(self)
    }
}

impl Writable for u64 {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(self)
    }
}

impl Writable for f64 {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(self)
    }
}

impl Writable for *const c_char {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(self)
    }
}

impl Writable for CStr {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_basic(&self.as_ptr())
    }
}

impl Writable for Signature {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_signature(self)
    }
}

impl<T> Writable for Vec<T>
where
    T: Writable + Argument,
{
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_array(self.iter())
    }
}

impl<T> Writable for [T]
where
    T: Writable + Argument,
{
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_array(self.iter())
    }
}

impl<K, V> Writable for (K, V)
where
    K: Writable,
    V: Writable,
{
    fn write(&self, writer: &mut MessageWriter) {
        let (key, value) = self;
        writer.append_dict_entry(key, value)
    }
}

impl<T> Writable for Variant<T>
where
    T: Writable + Argument,
{
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_variant(&self.0)
    }
}

impl<T> Writable for Option<T>
where
    T: Writable + Argument,
{
    fn write(&self, writer: &mut MessageWriter) {
        match self {
            Some(value) => writer.append_variant(value),
            None => writer.append_variant(&()),
        }
    }
}

impl Writable for () {
    fn write(&self, writer: &mut MessageWriter) {
        writer.append_unit()
    }
}

impl<T> Writable for &T
where
    T: Writable + ?Sized,
{
    fn write(&self, writer: &mut MessageWriter) {
        (*self).write(writer)
    }
}
