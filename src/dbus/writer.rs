use libdbus_sys as dbus_sys;
use std::ffi::{CStr, CString};
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;

use super::message::Message;
use super::types::{Argument, BasicType};
use super::values::{ArgType, DictEntry, ObjectPath, Signature, UnixFd, Variant};

pub struct MessageWriter<'a> {
    pub iter: Rc<dbus_sys::DBusMessageIter>,
    pub parent_iter: Option<Rc<dbus_sys::DBusMessageIter>>,
    pub message: &'a Message,
}

impl<'a> MessageWriter<'a> {
    pub fn from_message(message: &'a Message) -> Self {
        let iter = unsafe {
            let mut iter = mem::MaybeUninit::uninit();
            dbus_sys::dbus_message_iter_init_append(message.0, iter.as_mut_ptr());
            iter.assume_init()
        };
        Self {
            iter: Rc::new(iter),
            parent_iter: None,
            message,
        }
    }

    pub fn append<T>(&mut self, value: T)
    where
        T: Writable,
    {
        value.append(self);
    }

    fn append_basic(&mut self, arg_type: ArgType, value: &impl BasicType) {
        assert!(arg_type.is_basic());
        unsafe {
            dbus_sys::dbus_message_iter_append_basic(
                Rc::as_ptr(&self.iter) as *mut _,
                arg_type.to_byte() as c_int,
                (&value.to_basic()) as *const _ as *const c_void,
            );
        }
    }

    fn open_array(&mut self, signature: Signature) -> MessageWriter<'a> {
        self.open_container(ArgType::Array, Some(signature))
    }

    fn open_struct(&mut self) -> MessageWriter<'a> {
        self.open_container(ArgType::Struct, None)
    }

    fn open_variant(&mut self, signature: Signature) -> MessageWriter<'a> {
        self.open_container(ArgType::Variant, Some(signature))
    }

    fn open_dict_entry(&mut self) -> MessageWriter<'a> {
        self.open_container(ArgType::DictEntry, None)
    }

    fn open_container(&mut self, arg_type: ArgType, signature: Option<Signature>) -> MessageWriter<'a> {
        assert!(arg_type.is_container());
        let iter = unsafe {
            let mut iter = mem::MaybeUninit::uninit();
            let signature_str = signature.map(|signature| signature.to_string());
            let signature_ptr = if let Some(s) = signature_str.as_ref() {
                s.as_ptr() as *const c_char
            } else {
                ptr::null()
            };
            dbus_sys::dbus_message_iter_open_container(
                Rc::as_ptr(&self.iter) as *mut _,
                arg_type.to_byte() as c_int,
                signature_ptr,
                iter.as_mut_ptr(),
            );
            iter.assume_init()
        };

        MessageWriter {
            iter: Rc::new(iter),
            parent_iter: Some(self.iter.clone()),
            message: &self.message,
        }
    }
}

impl<'a> Drop for MessageWriter<'a> {
    fn drop(&mut self) {
        if let Some(parent_iter) = self.parent_iter.take() {
            unsafe {
                dbus_sys::dbus_message_iter_close_container(
                    Rc::as_ptr(&parent_iter) as *mut _,
                    Rc::as_ptr(&self.iter) as *mut _,
                );
            }
        }
    }
}

pub trait Writable: Argument {
    fn append(&self, writer: &mut MessageWriter);
}

impl Writable for bool {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), self);
    }
}

impl Writable for u8 {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), self);
    }
}

impl Writable for i16 {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), self);
    }
}

impl Writable for i32 {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), self);
    }
}

impl Writable for i64 {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), self);
    }
}

impl Writable for u16 {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), self);
    }
}

impl Writable for u32 {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), self);
    }
}

impl Writable for u64 {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), self);
    }
}

impl Writable for str {
    fn append(&self, writer: &mut MessageWriter) {
        let c_string = unsafe { CString::from_vec_unchecked(self.bytes().collect()) };
        writer.append_basic(Self::arg_type(), &c_string.as_ptr());
    }
}

impl Writable for String {
    fn append(&self, writer: &mut MessageWriter) {
        let c_string = unsafe { CString::from_vec_unchecked(self.bytes().collect()) };
        writer.append_basic(Self::arg_type(), &c_string.as_ptr());
    }
}

impl Writable for CStr {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), &self.as_ptr());
    }
}

impl Writable for CString {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), &self.as_ptr());
    }
}

impl<'a> Writable for ObjectPath<'a> {
    fn append(&self, writer: &mut MessageWriter) {
        let c_string = unsafe { CString::from_vec_unchecked(self.0.as_ref().bytes().collect()) };
        writer.append_basic(Self::arg_type(), &c_string.as_ptr())
    }
}

impl Writable for ArgType {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), &self.to_byte());
    }
}

impl<T: Writable> Writable for Vec<T> {
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_array(T::signature());
        for element in self {
            container.append(element);
        }
    }
}

impl<T: Writable> Writable for [T] {
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_array(T::signature());
        for element in self {
            container.append(element);
        }
    }
}

impl Writable for () {
    fn append(&self, writer: &mut MessageWriter) {
        writer.open_struct();
    }
}

impl<A, B> Writable for (A, B)
where
    A: Writable,
    B: Writable,
{
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_struct();
        container.append(&self.0);
        container.append(&self.1);
    }
}

impl<A, B, C> Writable for (A, B, C)
where
    A: Writable,
    B: Writable,
    C: Writable,
{
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_struct();
        container.append(&self.0);
        container.append(&self.1);
        container.append(&self.2);
    }
}

impl<A, B, C, D> Writable for (A, B, C, D)
where
    A: Writable,
    B: Writable,
    C: Writable,
    D: Writable,
{
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_struct();
        container.append(&self.0);
        container.append(&self.1);
        container.append(&self.2);
        container.append(&self.3);
    }
}

impl<A, B, C, D, E> Writable for (A, B, C, D, E)
where
    A: Writable,
    B: Writable,
    C: Writable,
    D: Writable,
    E: Writable,
{
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_struct();
        container.append(&self.0);
        container.append(&self.1);
        container.append(&self.2);
        container.append(&self.3);
        container.append(&self.4);
    }
}

impl<A, B, C, D, E, F> Writable for (A, B, C, D, E, F)
where
    A: Writable,
    B: Writable,
    C: Writable,
    D: Writable,
    E: Writable,
    F: Writable,
{
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_struct();
        container.append(&self.0);
        container.append(&self.1);
        container.append(&self.2);
        container.append(&self.3);
        container.append(&self.4);
        container.append(&self.5);
    }
}

impl<A, B, C, D, E, F, G> Writable for (A, B, C, D, E, F, G)
where
    A: Writable,
    B: Writable,
    C: Writable,
    D: Writable,
    E: Writable,
    F: Writable,
    G: Writable,
{
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_struct();
        container.append(&self.0);
        container.append(&self.1);
        container.append(&self.2);
        container.append(&self.3);
        container.append(&self.4);
        container.append(&self.5);
        container.append(&self.6);
    }
}

impl<A, B, C, D, E, F, G, H> Writable for (A, B, C, D, E, F, G, H)
where
    A: Writable,
    B: Writable,
    C: Writable,
    D: Writable,
    E: Writable,
    F: Writable,
    G: Writable,
    H: Writable,
{
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_struct();
        container.append(&self.0);
        container.append(&self.1);
        container.append(&self.2);
        container.append(&self.3);
        container.append(&self.4);
        container.append(&self.5);
        container.append(&self.6);
        container.append(&self.7);
    }
}

impl<A, B, C, D, E, F, G, H, I> Writable for (A, B, C, D, E, F, G, H, I)
where
    A: Writable,
    B: Writable,
    C: Writable,
    D: Writable,
    E: Writable,
    F: Writable,
    G: Writable,
    H: Writable,
    I: Writable,
{
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_struct();
        container.append(&self.0);
        container.append(&self.1);
        container.append(&self.2);
        container.append(&self.3);
        container.append(&self.4);
        container.append(&self.5);
        container.append(&self.6);
        container.append(&self.7);
        container.append(&self.8);
    }
}

impl<T: Writable> Writable for Variant<T> {
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_variant(T::signature());
        container.append(&self.0);
    }
}

impl<K: Writable, V: Writable> Writable for DictEntry<K, V> {
    fn append(&self, writer: &mut MessageWriter) {
        let mut container = writer.open_dict_entry();
        container.append(&self.0);
        container.append(&self.1);
    }
}

impl Writable for UnixFd {
    fn append(&self, writer: &mut MessageWriter) {
        writer.append_basic(Self::arg_type(), &self.0);
    }
}

impl<T> Writable for &T
where
    T: ?Sized + Writable,
{
    fn append(&self, writer: &mut MessageWriter) {
        (*self).append(writer)
    }
}
