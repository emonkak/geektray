use libdbus_sys as dbus_sys;
use std::error;
use std::ffi::{CStr, CString, NulError};
use std::fmt;
use std::mem;
use std::str::Utf8Error;

use super::message::Message;
use super::types::{
    ArgType, Argument, BasicValue, DictEntry, ObjectPath, Signature, UnixFd, Variant,
};

pub struct MessageReader<'a> {
    iter: dbus_sys::DBusMessageIter,
    message: &'a Message,
}

impl<'a> MessageReader<'a> {
    pub fn from_message(message: &'a Message) -> Self {
        let iter = unsafe {
            let mut iter = mem::MaybeUninit::uninit();
            dbus_sys::dbus_message_iter_init(message.0, iter.as_mut_ptr());
            iter.assume_init()
        };
        Self { message, iter }
    }

    pub fn arg_type(&self) -> ArgType {
        ArgType::from(unsafe {
            dbus_sys::dbus_message_iter_get_arg_type(&self.iter as *const _ as *mut _) as u8
        })
    }

    pub fn element_type(&self) -> ArgType {
        ArgType::from(unsafe {
            dbus_sys::dbus_message_iter_get_element_type(&self.iter as *const _ as *mut _) as u8
        })
    }

    pub fn signature(&self) -> Option<Signature> {
        let signature_ptr =
            unsafe { dbus_sys::dbus_message_iter_get_signature(&self.iter as *const _ as *mut _) };
        if signature_ptr.is_null() || unsafe { *signature_ptr } == 0 {
            None
        } else {
            let signature_str = unsafe { CString::from_raw(signature_ptr) };
            Some(Signature::parse(signature_str.to_bytes()).expect("parse signature"))
        }
    }

    pub fn peek<T>(&self) -> Result<T, Error>
    where
        T: Readable,
    {
        T::peek(self)
    }

    pub fn consume<T>(&mut self) -> Result<T, Error>
    where
        T: Readable,
    {
        T::consume(self)
    }

    fn get_basic(&self) -> BasicValue {
        assert!(self.arg_type().is_basic());
        let mut value = mem::MaybeUninit::<BasicValue>::uninit();
        unsafe {
            dbus_sys::dbus_message_iter_get_basic(
                &self.iter as *const _ as *mut _,
                value.as_mut_ptr().cast(),
            );
            value.assume_init()
        }
    }

    fn recurse(&self) -> MessageReader<'a> {
        assert!(self.arg_type().is_container());
        let subiter = unsafe {
            let mut subiter = mem::MaybeUninit::uninit();
            dbus_sys::dbus_message_iter_recurse(
                &self.iter as *const _ as *mut _,
                subiter.as_mut_ptr(),
            );
            subiter.assume_init()
        };
        Self {
            iter: subiter,
            message: self.message,
        }
    }

    fn next(&mut self) {
        unsafe {
            dbus_sys::dbus_message_iter_next(&mut self.iter);
        }
    }

    fn has_next(&self) -> bool {
        self.arg_type() != ArgType::Invalid
    }

    fn ensure_argument<T: Argument>(&self) -> Result<(), Error> {
        if self.arg_type() == T::arg_type() {
            Ok(())
        } else {
            Err(Error::UnexpectedSignature {
                expected: T::signature(),
                actual: self.signature(),
            })
        }
    }

    fn ensure_terminated(&self) -> Result<(), Error> {
        if !self.has_next() {
            Ok(())
        } else {
            Err(Error::TrailingSignature {
                actual: self.signature(),
            })
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Utf8Error(Utf8Error),
    NulError(NulError),
    UnexpectedEOF {
        expected: Signature,
    },
    TrailingSignature {
        actual: Option<Signature>,
    },
    UnexpectedSignature {
        expected: Signature,
        actual: Option<Signature>,
    },
}

impl From<Utf8Error> for Error {
    fn from(error: Utf8Error) -> Self {
        Self::Utf8Error(error)
    }
}

impl From<NulError> for Error {
    fn from(error: NulError) -> Self {
        Self::NulError(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Utf8Error(error) => error.fmt(f),
            Error::NulError(error) => error.fmt(f),
            Error::TrailingSignature { actual } => {
                write!(f, "Trailing signature `{:?}` found.", actual)
            }
            Error::UnexpectedEOF { expected } => {
                write!(f, "Expected signature type `{:?}` but EOF found.", expected)
            }
            Error::UnexpectedSignature { expected, actual } => {
                write!(
                    f,
                    "Expected signature `{:?}` but got `{:?}`.",
                    expected, actual,
                )
            }
        }
    }
}

impl error::Error for Error {}

pub trait Readable: Argument
where
    Self: Sized,
{
    fn peek(reader: &MessageReader) -> Result<Self, Error>;

    fn consume(reader: &mut MessageReader) -> Result<Self, Error> {
        let result = Self::peek(reader);
        reader.next();
        result
    }
}

impl Readable for bool {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(unsafe { reader.get_basic().bool })
    }
}

impl Readable for u8 {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(unsafe { reader.get_basic().byte })
    }
}

impl Readable for i16 {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(unsafe { reader.get_basic().i16 })
    }
}

impl Readable for i32 {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(unsafe { reader.get_basic().i32 })
    }
}

impl Readable for i64 {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(unsafe { reader.get_basic().i64 })
    }
}

impl Readable for u16 {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(unsafe { reader.get_basic().u16 })
    }
}

impl Readable for u32 {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(unsafe { reader.get_basic().u32 })
    }
}

impl Readable for u64 {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(unsafe { reader.get_basic().u64 })
    }
}

impl Readable for f64 {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(unsafe { reader.get_basic().f64 })
    }
}

impl Readable for &str {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let c_str = unsafe { CStr::from_ptr(reader.get_basic().str) };
        Ok(c_str.to_str()?)
    }
}

impl Readable for String {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let c_str = unsafe { CStr::from_ptr(reader.get_basic().str) };
        Ok(c_str.to_string_lossy().into_owned())
    }
}

impl Readable for &CStr {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(unsafe { CStr::from_ptr(reader.get_basic().str) })
    }
}

impl Readable for CString {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let c_str = unsafe { CStr::from_ptr(reader.get_basic().str) };
        Ok(CString::new(c_str.to_bytes())?)
    }
}

impl<'a> Readable for ObjectPath<'a> {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let c_str = unsafe { CStr::from_ptr(reader.get_basic().str) };
        Ok(ObjectPath(c_str.to_string_lossy()))
    }
}

impl Readable for ArgType {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        unsafe { Ok(ArgType::from(reader.get_basic().byte)) }
    }
}

impl<T: Readable> Readable for Vec<T> {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let mut sub_reader = reader.recurse();
        let mut elements = Vec::new();
        while sub_reader.has_next() {
            let element = T::consume(&mut sub_reader)?;
            elements.push(element);
        }
        Ok(elements)
    }
}

impl Readable for () {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let sub_reader = reader.recurse();
        sub_reader.ensure_terminated()
    }
}

impl<A, B> Readable for (A, B)
where
    A: Readable,
    B: Readable,
{
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let mut sub_reader = reader.recurse();
        let a = A::consume(&mut sub_reader)?;
        let b = B::consume(&mut sub_reader)?;
        sub_reader.ensure_terminated()?;
        Ok((a, b))
    }
}

impl<A, B, C> Readable for (A, B, C)
where
    A: Readable,
    B: Readable,
    C: Readable,
{
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let mut sub_reader = reader.recurse();
        let a = A::consume(&mut sub_reader)?;
        let b = B::consume(&mut sub_reader)?;
        let c = C::consume(&mut sub_reader)?;
        sub_reader.ensure_terminated()?;
        Ok((a, b, c))
    }
}

impl<A, B, C, D> Readable for (A, B, C, D)
where
    A: Readable,
    B: Readable,
    C: Readable,
    D: Readable,
{
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let mut sub_reader = reader.recurse();
        let a = A::consume(&mut sub_reader)?;
        let b = B::consume(&mut sub_reader)?;
        let c = C::consume(&mut sub_reader)?;
        let d = D::consume(&mut sub_reader)?;
        sub_reader.ensure_terminated()?;
        Ok((a, b, c, d))
    }
}

impl<A, B, C, D, E> Readable for (A, B, C, D, E)
where
    A: Readable,
    B: Readable,
    C: Readable,
    D: Readable,
    E: Readable,
{
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let mut sub_reader = reader.recurse();
        let a = A::consume(&mut sub_reader)?;
        let b = B::consume(&mut sub_reader)?;
        let c = C::consume(&mut sub_reader)?;
        let d = D::consume(&mut sub_reader)?;
        let e = E::consume(&mut sub_reader)?;
        sub_reader.ensure_terminated()?;
        Ok((a, b, c, d, e))
    }
}

impl<A, B, C, D, E, F> Readable for (A, B, C, D, E, F)
where
    A: Readable,
    B: Readable,
    C: Readable,
    D: Readable,
    E: Readable,
    F: Readable,
{
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let mut sub_reader = reader.recurse();
        let a = A::consume(&mut sub_reader)?;
        let b = B::consume(&mut sub_reader)?;
        let c = C::consume(&mut sub_reader)?;
        let d = D::consume(&mut sub_reader)?;
        let e = E::consume(&mut sub_reader)?;
        let f = F::consume(&mut sub_reader)?;
        sub_reader.ensure_terminated()?;
        Ok((a, b, c, d, e, f))
    }
}

impl<A, B, C, D, E, F, G> Readable for (A, B, C, D, E, F, G)
where
    A: Readable,
    B: Readable,
    C: Readable,
    D: Readable,
    E: Readable,
    F: Readable,
    G: Readable,
{
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let mut sub_reader = reader.recurse();
        let a = A::consume(&mut sub_reader)?;
        let b = B::consume(&mut sub_reader)?;
        let c = C::consume(&mut sub_reader)?;
        let d = D::consume(&mut sub_reader)?;
        let e = E::consume(&mut sub_reader)?;
        let f = F::consume(&mut sub_reader)?;
        let g = G::consume(&mut sub_reader)?;
        sub_reader.ensure_terminated()?;
        Ok((a, b, c, d, e, f, g))
    }
}

impl<A, B, C, D, E, F, G, H> Readable for (A, B, C, D, E, F, G, H)
where
    A: Readable,
    B: Readable,
    C: Readable,
    D: Readable,
    E: Readable,
    F: Readable,
    G: Readable,
    H: Readable,
{
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let mut sub_reader = reader.recurse();
        let a = A::consume(&mut sub_reader)?;
        let b = B::consume(&mut sub_reader)?;
        let c = C::consume(&mut sub_reader)?;
        let d = D::consume(&mut sub_reader)?;
        let e = E::consume(&mut sub_reader)?;
        let f = F::consume(&mut sub_reader)?;
        let g = G::consume(&mut sub_reader)?;
        let h = H::consume(&mut sub_reader)?;
        sub_reader.ensure_terminated()?;
        Ok((a, b, c, d, e, f, g, h))
    }
}

impl<A, B, C, D, E, F, G, H, I> Readable for (A, B, C, D, E, F, G, H, I)
where
    A: Readable,
    B: Readable,
    C: Readable,
    D: Readable,
    E: Readable,
    F: Readable,
    G: Readable,
    H: Readable,
    I: Readable,
{
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let mut sub_reader = reader.recurse();
        let a = A::consume(&mut sub_reader)?;
        let b = B::consume(&mut sub_reader)?;
        let c = C::consume(&mut sub_reader)?;
        let d = D::consume(&mut sub_reader)?;
        let e = E::consume(&mut sub_reader)?;
        let f = F::consume(&mut sub_reader)?;
        let g = G::consume(&mut sub_reader)?;
        let h = H::consume(&mut sub_reader)?;
        let i = I::consume(&mut sub_reader)?;
        sub_reader.ensure_terminated()?;
        Ok((a, b, c, d, e, f, g, h, i))
    }
}

impl<T: Readable> Readable for Variant<T> {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(Variant(T::peek(&reader.recurse())?))
    }
}

impl<K: Readable, V: Readable> Readable for DictEntry<K, V> {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        let mut sub_reader = reader.recurse();
        let key = K::consume(&mut sub_reader)?;
        let value = V::consume(&mut sub_reader)?;
        sub_reader.ensure_terminated()?;
        Ok(DictEntry(key, value))
    }
}

impl Readable for UnixFd {
    fn peek(reader: &MessageReader) -> Result<Self, Error> {
        reader.ensure_argument::<Self>()?;
        Ok(UnixFd(unsafe { reader.get_basic().fd }))
    }
}
