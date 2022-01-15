use libdbus_sys as dbus_sys;
use serde::ser;
use serde::ser::Serialize as _;
use std::error;
use std::ffi::{CString, NulError};
use std::fmt;
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::rc::Rc;

use super::message::Message;
use super::types::{ArgType, Argument, Signature};

#[derive(Clone)]
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

    pub fn write<T>(&self, value: T) -> Result<(), Error>
    where
        T: Argument + ser::Serialize,
    {
        let mut serializer = Serializer::new(self.clone(), T::signature());
        value.serialize(&mut serializer)?;
        if let Some(signature) = serializer.peek() {
            Err(Error::UnexpectedEOF(signature))
        } else {
            Ok(())
        }
    }

    fn open_container(
        &self,
        arg_type: ArgType,
        element_signature: Option<Signature>,
    ) -> MessageWriter<'a> {
        let iter = unsafe {
            let mut iter = mem::MaybeUninit::uninit();
            let signature_str = element_signature.map(|signature| signature.to_string());
            let signature_ptr = if let Some(s) = signature_str.as_ref() {
                s.as_ptr() as *const c_char
            } else {
                ptr::null()
            };
            dbus_sys::dbus_message_iter_open_container(
                Rc::as_ptr(&self.iter) as *mut _,
                arg_type.into(),
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

    fn append_basic<T>(&self, value: &T)
    where
        T: ?Sized + Argument,
    {
        unsafe {
            dbus_sys::dbus_message_iter_append_basic(
                Rc::as_ptr(&self.iter) as *mut _,
                T::arg_type().into(),
                value as *const T as *const c_void,
            );
        }
    }

    fn is_container(&self) -> bool {
        self.parent_iter.is_some()
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

pub struct Serializer<'a> {
    iter: MessageWriter<'a>,
    signature: Signature,
    signature_index: usize,
}

impl<'a> Serializer<'a> {
    fn new(iter: MessageWriter<'a>, signature: Signature) -> Self {
        Self {
            iter,
            signature,
            signature_index: 0,
        }
    }

    fn push<T>(&mut self, value: &T) -> Result<(), Error>
    where
        T: ?Sized + Argument,
    {
        if let Some(expected) = self.consume() {
            if expected == T::signature()
                || (expected.is_string_like() && T::signature() == Signature::String)
            {
                self.iter.append_basic(value);
                Ok(())
            } else {
                Err(Error::UnexpectedSignature {
                    expected: expected.clone(),
                    actual: T::signature(),
                })
            }
        } else {
            Err(Error::TrailingSignature(T::signature()))
        }
    }

    fn push_array(&mut self) -> Result<Serializer<'a>, Error> {
        match self.consume() {
            Some(Signature::Array(element)) => {
                let container = self
                    .iter
                    .open_container(ArgType::Array, Some(element.as_ref().clone()));
                Ok(Self::new(container, Signature::Array(element)))
            }
            Some(expected) => Err(Error::UnexpectedArgType {
                expected: expected.arg_type(),
                actual: ArgType::Array,
            }),
            None => Err(Error::TrailingArgType(ArgType::Array)),
        }
    }

    fn push_struct(&mut self) -> Result<Serializer<'a>, Error> {
        match self.consume() {
            Some(signature @ Signature::Struct(_)) => {
                let container = self.iter.open_container(ArgType::Struct, None);
                Ok(Self::new(container, signature))
            }
            Some(expected) => Err(Error::UnexpectedArgType {
                expected: expected.arg_type(),
                actual: ArgType::Struct,
            }),
            None => Err(Error::TrailingArgType(ArgType::Struct)),
        }
    }

    fn push_variant(&mut self) -> Result<Serializer<'a>, Error> {
        match self.consume() {
            Some(signature) => {
                let container = self
                    .iter
                    .open_container(ArgType::Variant, Some(signature.clone()));
                Ok(Self::new(container, signature))
            }
            None => Err(Error::TrailingArgType(ArgType::Variant)),
        }
    }

    fn push_dict_entry(&mut self) -> Result<Serializer<'a>, Error> {
        match self.consume() {
            Some(signature @ Signature::DictEntry(_)) => {
                let container = self.iter.open_container(ArgType::DictEntry, None);
                Ok(Self::new(container, signature))
            }
            Some(expected) => Err(Error::UnexpectedArgType {
                expected: expected.arg_type(),
                actual: ArgType::DictEntry,
            }),
            None => Err(Error::TrailingArgType(ArgType::Variant)),
        }
    }

    fn consume(&mut self) -> Option<Signature> {
        let signature = self.peek();
        self.signature_index += 1;
        signature
    }

    fn peek(&self) -> Option<Signature> {
        if self.iter.is_container() {
            match &self.signature {
                signature
                @
                (Signature::Byte
                | Signature::Boolean
                | Signature::Int16
                | Signature::Uint16
                | Signature::Int32
                | Signature::Uint32
                | Signature::Int64
                | Signature::Uint64
                | Signature::Double
                | Signature::String
                | Signature::ObjectPath
                | Signature::Signature
                | Signature::UnixFd)
                    if self.signature_index == 0 =>
                {
                    Some(signature.clone())
                }
                Signature::Array(element) => Some(element.as_ref().clone()),
                Signature::Struct(values) if self.signature_index < values.len() => {
                    Some(values[self.signature_index].clone())
                }
                Signature::DictEntry(entry) if self.signature_index == 0 => Some(entry.0.clone()),
                Signature::DictEntry(entry) if self.signature_index == 1 => Some(entry.1.clone()),
                Signature::Variant(Some(value)) if self.signature_index == 0 => {
                    Some(value.as_ref().clone())
                }
                _ => None,
            }
        } else if self.signature_index == 0 {
            Some(self.signature.clone())
        } else {
            None
        }
    }
}

impl<'a> ser::Serializer for &mut Serializer<'a> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Serializer<'a>;
    type SerializeTuple = Serializer<'a>;
    type SerializeTupleStruct = Serializer<'a>;
    type SerializeTupleVariant = Serializer<'a>;
    type SerializeMap = MapSerializer<'a>;
    type SerializeStruct = Serializer<'a>;
    type SerializeStructVariant = Serializer<'a>;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.push(&v)?;
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.push(&(v as u8))?;
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.push(&v)?;
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.push(&v)?;
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.push(&v)?;
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.push(&v)?;
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.push(&v)?;
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.push(&v)?;
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.push(&v)?;
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.push(&(v as f64))?;
        Ok(())
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.push(&v)?;
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        self.push(&(v as u32))?;
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        let c_string = CString::new(v)?;
        self.push(&c_string.as_ptr())?;
        Ok(())
    }

    fn serialize_bytes(self, vs: &[u8]) -> Result<Self::Ok, Self::Error> {
        let mut array = self.push_array()?;
        for v in vs {
            array.push(v)?;
        }
        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        let mut variant_container = self.push_variant()?;
        variant_container.push_struct()?;
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        let mut variant_container = self.push_variant()?;
        value.serialize(&mut variant_container)?;
        Ok(())
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.push_struct()?;
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        let mut struct_container = self.push_struct()?;
        variant.serialize(&mut struct_container)?;
        let mut variant_container = struct_container.push_variant()?;
        value.serialize(&mut variant_container)?;
        Ok(())
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.push_array()
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.push_struct()
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.push_struct()
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        let mut struct_container = self.push_struct()?;
        variant.serialize(&mut struct_container)?;
        let mut variant_container = struct_container.push_variant()?;
        let inner_struct_container = variant_container.push_struct()?;
        Ok(inner_struct_container)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(MapSerializer {
            array: self.push_array()?,
            dict_entry: None,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.push_array()
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let mut struct_container = self.push_struct()?;
        variant.serialize(&mut struct_container)?;
        let array_container = struct_container.push_array()?;
        Ok(array_container)
    }
}

impl<'a> ser::SerializeSeq for Serializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> ser::SerializeTuple for Serializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        if let Some(signature) = self.peek() {
            Err(Error::UnexpectedEOF(signature))
        } else {
            Ok(())
        }
    }
}

impl<'a> ser::SerializeTupleStruct for Serializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        if let Some(signature) = self.peek() {
            Err(Error::UnexpectedEOF(signature))
        } else {
            Ok(())
        }
    }
}

impl<'a> ser::SerializeTupleVariant for Serializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        if let Some(signature) = self.peek() {
            Err(Error::UnexpectedEOF(signature))
        } else {
            Ok(())
        }
    }
}

impl<'a> ser::SerializeStruct for Serializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        let mut dict_entry_container = self.push_dict_entry()?;
        key.serialize(&mut dict_entry_container)?;
        let mut variant_container = dict_entry_container.push_dict_entry()?;
        value.serialize(&mut variant_container)?;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        if let Some(signature) = self.peek() {
            Err(Error::UnexpectedEOF(signature))
        } else {
            Ok(())
        }
    }
}

impl<'a> ser::SerializeStructVariant for Serializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        let mut dict_entry_container = self.push_dict_entry()?;
        key.serialize(&mut dict_entry_container)?;
        let mut variant_container = dict_entry_container.push_variant()?;
        value.serialize(&mut variant_container)?;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        if let Some(signature) = self.peek() {
            Err(Error::UnexpectedEOF(signature))
        } else {
            Ok(())
        }
    }
}

pub struct MapSerializer<'a> {
    array: Serializer<'a>,
    dict_entry: Option<Serializer<'a>>,
}

impl<'a> ser::SerializeMap for MapSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        assert!(self.dict_entry.is_none());
        let mut dict_entry = self.array.push_dict_entry()?;
        key.serialize(&mut dict_entry)?;
        self.dict_entry = Some(dict_entry);
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        let mut dict_entry = self.dict_entry.take().expect("take dict entry");
        value.serialize(&mut dict_entry)
    }

    fn serialize_entry<K: ?Sized, V: ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), Self::Error>
    where
        K: ser::Serialize,
        V: ser::Serialize,
    {
        let mut dict_entry = self.array.push_dict_entry()?;
        key.serialize(&mut dict_entry)?;
        value.serialize(&mut dict_entry)?;
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        if let Some(signature) = self.dict_entry.and_then(|dict_entry| dict_entry.peek()) {
            Err(Error::UnexpectedEOF(signature))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Message(String),
    NulError(NulError),
    UnexpectedEOF(Signature),
    UnexpectedArgType {
        expected: ArgType,
        actual: ArgType,
    },
    UnexpectedSignature {
        expected: Signature,
        actual: Signature,
    },
    TrailingArgType(ArgType),
    TrailingSignature(Signature),
}

impl From<NulError> for Error {
    fn from(error: NulError) -> Self {
        Self::NulError(error)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Message(message) => f.write_str(message),
            Error::NulError(error) => error.fmt(f),
            Error::UnexpectedEOF(expected) => {
                write!(f, "Expected signature type `{:?}` but EOF found.", expected,)
            }
            Error::UnexpectedArgType { expected, actual } => {
                write!(
                    f,
                    "Expected argument type `{:?}` but got `{:?}`.",
                    expected, actual,
                )
            }
            Error::UnexpectedSignature { expected, actual } => {
                write!(
                    f,
                    "Expected signature `{:?}` but got `{:?}`.",
                    expected, actual,
                )
            }
            Error::TrailingSignature(signature) => {
                write!(f, "Trailing signature `{:?}`.", signature,)
            }
            Error::TrailingArgType(arg_type) => {
                write!(f, "Trailing argument type `{:?}`.", arg_type,)
            }
        }
    }
}

impl error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Error::Message(message.to_string())
    }
}
