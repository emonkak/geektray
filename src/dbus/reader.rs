use libdbus_sys as dbus_sys;
use serde::de;
use std::borrow::Cow;
use std::error;
use std::ffi::CStr;
use std::fmt;
use std::mem;
use std::os::raw::*;
use std::str;

use super::message::Message;
use super::types::{ArgType, Argument, Signature, SignatureParseError, Variant};

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
        unsafe { dbus_sys::dbus_message_iter_get_arg_type(&self.iter as *const _ as *mut _).into() }
    }

    pub fn element_type(&self) -> ArgType {
        unsafe {
            dbus_sys::dbus_message_iter_get_element_type(&self.iter as *const _ as *mut _).into()
        }
    }

    pub fn signature(&self) -> Signature {
        let signature_str = unsafe {
            CStr::from_ptr(dbus_sys::dbus_message_iter_get_signature(
                &self.iter as *const _ as *mut _,
            ))
        };
        Signature::parse(signature_str.to_bytes()).expect("parse signature")
    }

    pub fn has_next(&self) -> bool {
        self.arg_type() != ArgType::Invalid
    }

    pub fn next<T: Readable<'a>>(&mut self) -> Result<T, Error> {
        let value = T::read(self);
        self.advance();
        value
    }

    pub fn next_iter(&mut self) -> MessageReader<'a> {
        let subiter = self.recurse();
        self.advance();
        subiter
    }

    fn advance(&mut self) {
        unsafe {
            dbus_sys::dbus_message_iter_next(&mut self.iter);
        }
    }

    fn recurse(&self) -> MessageReader<'a> {
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

    fn get_basic<T>(&self) -> Result<T, Error>
    where
        T: Argument,
    {
        if self.arg_type() == T::arg_type() {
            let mut value = mem::MaybeUninit::<T>::uninit();
            unsafe {
                dbus_sys::dbus_message_iter_get_basic(
                    &self.iter as *const _ as *mut _,
                    value.as_mut_ptr().cast(),
                )
            };
            Ok(unsafe { value.assume_init() })
        } else {
            Err(Error::UnexpectedArgType {
                expected: T::arg_type(),
                actual: self.arg_type(),
            })
        }
    }
}

impl<'de, 'a> de::Deserializer<'de> for &mut MessageReader<'a> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.arg_type() {
            ArgType::Array | ArgType::Struct | ArgType::DictEntry => {
                let subiter = self.next_iter();
                visitor.visit_seq(subiter)
            }
            ArgType::Variant => {
                let mut subiter = self.next_iter();
                subiter.deserialize_any(visitor)
            }
            ArgType::Boolean => self.deserialize_bool(visitor),
            ArgType::String => self.deserialize_str(visitor),
            ArgType::Byte => self.deserialize_u8(visitor),
            ArgType::Int16 => self.deserialize_i16(visitor),
            ArgType::Uint16 => self.deserialize_u16(visitor),
            ArgType::Int32 => self.deserialize_i32(visitor),
            ArgType::Uint32 => self.deserialize_u32(visitor),
            ArgType::Int64 => self.deserialize_i64(visitor),
            ArgType::Uint64 => self.deserialize_u64(visitor),
            ArgType::Double => self.deserialize_f64(visitor),
            ArgType::UnixFd => self.deserialize_i32(visitor),
            ArgType::ObjectPath => self.deserialize_str(visitor),
            ArgType::Signature => self.deserialize_str(visitor),
            ArgType::Invalid => Err(Error::InvalidArgType),
        }
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_bool(bool::read(self)?)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_i8(self.next::<u8>()? as i8)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_u16(self.next()?)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_i32(self.next()?)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_i64(self.next()?)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_u8(self.next()?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_u16(self.next()?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_u32(self.next()?)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_u64(self.next()?)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_f32(self.next::<f64>()? as f32)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_f64(self.next()?)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_char(unsafe { char::from_u32_unchecked(self.next()?) })
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let c_str = unsafe { CStr::from_ptr(self.next()?) };
        visitor.visit_str(c_str.to_string_lossy().as_ref())
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let c_str = unsafe { CStr::from_ptr(self.next()?) };
        visitor.visit_string(c_str.to_string_lossy().into_owned())
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_bytes(self.next::<Vec<u8>>()?.as_slice())
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_byte_buf(self.next::<Vec<u8>>()?)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        if self.arg_type() == ArgType::Variant {
            let mut subiter = self.next_iter();
            match subiter.signature() {
                Signature::Struct(contents) if contents.len() == 0 => visitor.visit_unit(),
                _ => subiter.deserialize_any(visitor),
            }
        } else {
            Err(Error::UnexpectedArgType {
                expected: ArgType::Variant,
                actual: self.arg_type(),
            })
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.next::<()>()?;
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.next::<()>()?;
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        if self.arg_type() == ArgType::Array {
            let subiter = self.next_iter();
            visitor.visit_seq(subiter)
        } else {
            Err(Error::UnexpectedArgType {
                expected: ArgType::Array,
                actual: self.arg_type(),
            })
        }
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        match self.signature() {
            Signature::Struct(contents) if contents.len() == len => {
                let subiter = self.next_iter();
                visitor.visit_seq(subiter)
            }
            signature => Err(Error::UnexpectedArgType {
                expected: ArgType::Struct,
                actual: signature.arg_type(),
            }),
        }
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_seq(self)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        if self.arg_type() == ArgType::Array {
            if self.element_type() == ArgType::DictEntry {
                let subiter = self.next_iter();
                visitor.visit_map(subiter)
            } else {
                Err(Error::UnexpectedElementType {
                    expected: ArgType::DictEntry,
                    actual: self.element_type(),
                })
            }
        } else {
            Err(Error::UnexpectedArgType {
                expected: ArgType::Array,
                actual: self.arg_type(),
            })
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        if self.arg_type() == ArgType::Variant {
            let mut subiter = self.next_iter();
            subiter.deserialize_any(visitor)
        } else {
            Err(Error::UnexpectedArgType {
                expected: ArgType::Variant,
                actual: self.arg_type(),
            })
        }
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
}

impl<'de, 'a> de::SeqAccess<'de> for MessageReader<'a> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if self.has_next() {
            seed.deserialize(self).map(Some)
        } else {
            Ok(None)
        }
    }
}

impl<'de, 'a> de::MapAccess<'de> for MessageReader<'a> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        if self.has_next() {
            let mut subiter = self.recurse();
            seed.deserialize(&mut subiter).map(Some)
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let mut subiter = self.next_iter();
        subiter.advance();
        seed.deserialize(&mut subiter)
    }

    fn next_entry_seed<K, V>(
        &mut self,
        kseed: K,
        vseed: V,
    ) -> Result<Option<(K::Value, V::Value)>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
        V: de::DeserializeSeed<'de>,
    {
        if self.has_next() {
            let mut subiter = self.next_iter();
            let key = kseed.deserialize(&mut subiter).map(Some)?;
            let value = vseed.deserialize(&mut subiter).map(Some)?;
            Ok(key.zip(value))
        } else {
            Ok(None)
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Error {
    Message(Cow<'static, str>),
    SignatureParseError(SignatureParseError),
    InvalidArgType,
    UnexpectedArgType { expected: ArgType, actual: ArgType },
    UnexpectedElementType { expected: ArgType, actual: ArgType },
}

impl From<SignatureParseError> for Error {
    fn from(error: SignatureParseError) -> Self {
        Self::SignatureParseError(error)
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::Message(Cow::from(message))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Message(message) => f.write_str(message),
            Error::SignatureParseError(error) => error.fmt(f),
            Error::InvalidArgType => {
                write!(f, "Argument type is invalid.",)
            }
            Error::UnexpectedArgType { expected, actual } => {
                write!(
                    f,
                    "Expected argument type `{:?}` but got `{:?}`.",
                    expected, actual,
                )
            }
            Error::UnexpectedElementType { expected, actual } => {
                write!(
                    f,
                    "Expected element type `{:?}` but got `{:?}`.",
                    expected, actual,
                )
            }
        }
    }
}

impl error::Error for Error {}

impl de::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Error::Message(Cow::from(message.to_string()))
    }
}

pub trait Readable<'a>: Sized {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error>;
}

impl<'a> Readable<'a> for bool {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        reader.get_basic()
    }
}

impl<'a> Readable<'a> for u8 {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        reader.get_basic()
    }
}

impl<'a> Readable<'a> for i16 {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        reader.get_basic()
    }
}

impl<'a> Readable<'a> for u16 {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        reader.get_basic()
    }
}

impl<'a> Readable<'a> for i32 {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        reader.get_basic()
    }
}

impl<'a> Readable<'a> for u32 {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        reader.get_basic()
    }
}

impl<'a> Readable<'a> for i64 {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        reader.get_basic()
    }
}

impl<'a> Readable<'a> for u64 {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        reader.get_basic()
    }
}

impl<'a> Readable<'a> for f64 {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        reader.get_basic()
    }
}

impl<'a> Readable<'a> for *const c_char {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        reader.get_basic()
    }
}

impl<'a> Readable<'a> for &CStr {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        Ok(unsafe { CStr::from_ptr(reader.get_basic()?) })
    }
}

impl<'a> Readable<'a> for Signature {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        let signature_str = unsafe { CStr::from_ptr(reader.get_basic()?) };
        let signature = Signature::parse(signature_str.to_bytes())?;
        Ok(signature)
    }
}

impl<'a, T: Readable<'a>> Readable<'a> for Vec<T> {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        if reader.arg_type() == ArgType::Array {
            let mut subiter = reader.recurse();
            let mut elements = Vec::new();
            while subiter.has_next() {
                let element = subiter.next::<T>()?;
                elements.push(element);
            }
            Ok(elements)
        } else {
            Err(Error::UnexpectedArgType {
                expected: ArgType::Array,
                actual: reader.arg_type(),
            })
        }
    }
}

impl<'a, K: Readable<'a>, V: Readable<'a>> Readable<'a> for (K, V) {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        if reader.arg_type() == ArgType::DictEntry {
            let mut subiter = reader.recurse();
            let key = subiter.next::<K>()?;
            let value = subiter.next::<V>()?;
            Ok((key, value))
        } else {
            Err(Error::UnexpectedArgType {
                expected: ArgType::DictEntry,
                actual: reader.arg_type(),
            })
        }
    }
}

impl<'a, T: Readable<'a>> Readable<'a> for Variant<T> {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        if reader.arg_type() == ArgType::Variant {
            let subiter = reader.recurse();
            let value = T::read(&subiter)?;
            Ok(Self(value))
        } else {
            Err(Error::UnexpectedArgType {
                expected: ArgType::Variant,
                actual: reader.arg_type(),
            })
        }
    }
}

impl<'a, T: Readable<'a>> Readable<'a> for Option<T> {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        if reader.arg_type() == ArgType::Variant {
            let subiter = reader.recurse();
            match subiter.signature() {
                Signature::Struct(contents) if contents.len() == 0 => Ok(None),
                _ => Ok(Some(T::read(reader)?)),
            }
        } else {
            Err(Error::UnexpectedArgType {
                expected: ArgType::Variant,
                actual: reader.arg_type(),
            })
        }
    }
}

impl<'a> Readable<'a> for () {
    fn read(reader: &MessageReader<'a>) -> Result<Self, Error> {
        if reader.arg_type() == ArgType::Struct {
            Ok(())
        } else {
            Err(Error::UnexpectedArgType {
                expected: ArgType::Struct,
                actual: reader.arg_type(),
            })
        }
    }
}
