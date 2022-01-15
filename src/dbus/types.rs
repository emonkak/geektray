use std::ffi::{CStr, CString};
use std::os::raw::*;

use super::values::{ArgType, BasicValue, DictEntry, ObjectPath, Signature, UnixFd, Variant};

pub trait BasicType
where
    Self: Sized,
{
    fn from_basic(value: BasicValue) -> Self;

    fn to_basic(&self) -> BasicValue;
}

impl BasicType for bool {
    fn from_basic(value: BasicValue) -> Self {
        unsafe { value.bool }
    }

    fn to_basic(&self) -> BasicValue {
        BasicValue { bool: *self }
    }
}

impl BasicType for u8 {
    fn from_basic(value: BasicValue) -> Self {
        unsafe { value.byte }
    }

    fn to_basic(&self) -> BasicValue {
        BasicValue { byte: *self }
    }
}

impl BasicType for i16 {
    fn from_basic(value: BasicValue) -> Self {
        unsafe { value.i16 }
    }

    fn to_basic(&self) -> BasicValue {
        BasicValue { i16: *self }
    }
}

impl BasicType for i32 {
    fn from_basic(value: BasicValue) -> Self {
        unsafe { value.i32 }
    }

    fn to_basic(&self) -> BasicValue {
        BasicValue { i32: *self }
    }
}

impl BasicType for i64 {
    fn from_basic(value: BasicValue) -> Self {
        unsafe { value.i64 }
    }

    fn to_basic(&self) -> BasicValue {
        BasicValue { i64: *self }
    }
}

impl BasicType for u16 {
    fn from_basic(value: BasicValue) -> Self {
        unsafe { value.u16 }
    }

    fn to_basic(&self) -> BasicValue {
        BasicValue { u16: *self }
    }
}

impl BasicType for u32 {
    fn from_basic(value: BasicValue) -> Self {
        unsafe { value.u32 }
    }

    fn to_basic(&self) -> BasicValue {
        BasicValue { u32: *self }
    }
}

impl BasicType for u64 {
    fn from_basic(value: BasicValue) -> Self {
        unsafe { value.u64 }
    }

    fn to_basic(&self) -> BasicValue {
        BasicValue { u64: *self }
    }
}

impl BasicType for f64 {
    fn from_basic(value: BasicValue) -> Self {
        unsafe { value.f64 }
    }

    fn to_basic(&self) -> BasicValue {
        BasicValue { f64: *self }
    }
}

impl BasicType for *const c_char {
    fn from_basic(value: BasicValue) -> Self {
        unsafe { value.str }
    }

    fn to_basic(&self) -> BasicValue {
        BasicValue { str: *self }
    }
}

pub trait Argument {
    fn signature() -> Signature;

    fn arg_type() -> ArgType {
        Self::signature().arg_type()
    }
}

impl Argument for bool {
    fn signature() -> Signature {
        Signature::Boolean
    }
}

impl Argument for u8 {
    fn signature() -> Signature {
        Signature::Byte
    }
}

impl Argument for i16 {
    fn signature() -> Signature {
        Signature::Int16
    }
}

impl Argument for i32 {
    fn signature() -> Signature {
        Signature::Int32
    }
}

impl Argument for i64 {
    fn signature() -> Signature {
        Signature::Int64
    }
}

impl Argument for u16 {
    fn signature() -> Signature {
        Signature::Uint16
    }
}

impl Argument for u32 {
    fn signature() -> Signature {
        Signature::Uint32
    }
}

impl Argument for u64 {
    fn signature() -> Signature {
        Signature::Uint64
    }
}

impl Argument for f64 {
    fn signature() -> Signature {
        Signature::Double
    }
}

impl Argument for *const c_char {
    fn signature() -> Signature {
        Signature::String
    }
}

impl Argument for str {
    fn signature() -> Signature {
        Signature::String
    }
}

impl Argument for String {
    fn signature() -> Signature {
        Signature::String
    }
}

impl Argument for CStr {
    fn signature() -> Signature {
        Signature::String
    }
}

impl Argument for CString {
    fn signature() -> Signature {
        Signature::String
    }
}

impl<'a> Argument for ObjectPath<'a> {
    fn signature() -> Signature {
        Signature::ObjectPath
    }
}

impl Argument for ArgType {
    fn signature() -> Signature {
        Signature::Signature
    }
}

impl<T: Argument> Argument for Vec<T> {
    fn signature() -> Signature {
        Signature::Array(Box::new(T::signature()))
    }
}

impl<T: Argument> Argument for [T] {
    fn signature() -> Signature {
        Signature::Array(Box::new(T::signature()))
    }
}

impl<T: Argument> Argument for Option<T> {
    fn signature() -> Signature {
        Signature::Variant
    }
}

impl Argument for () {
    fn signature() -> Signature {
        Signature::Struct(vec![])
    }
}

impl<A, B> Argument for (A, B)
where
    A: Argument,
    B: Argument,
{
    fn signature() -> Signature {
        Signature::Struct(vec![A::signature(), B::signature()])
    }
}

impl<A, B, C> Argument for (A, B, C)
where
    A: Argument,
    B: Argument,
    C: Argument,
{
    fn signature() -> Signature {
        Signature::Struct(vec![A::signature(), B::signature(), C::signature()])
    }
}

impl<A, B, C, D> Argument for (A, B, C, D)
where
    A: Argument,
    B: Argument,
    C: Argument,
    D: Argument,
{
    fn signature() -> Signature {
        Signature::Struct(vec![
            A::signature(),
            B::signature(),
            C::signature(),
            D::signature(),
        ])
    }
}

impl<A, B, C, D, E> Argument for (A, B, C, D, E)
where
    A: Argument,
    B: Argument,
    C: Argument,
    D: Argument,
    E: Argument,
{
    fn signature() -> Signature {
        Signature::Struct(vec![
            A::signature(),
            B::signature(),
            C::signature(),
            D::signature(),
            E::signature(),
        ])
    }
}

impl<A, B, C, D, E, F> Argument for (A, B, C, D, E, F)
where
    A: Argument,
    B: Argument,
    C: Argument,
    D: Argument,
    E: Argument,
    F: Argument,
{
    fn signature() -> Signature {
        Signature::Struct(vec![
            A::signature(),
            B::signature(),
            C::signature(),
            D::signature(),
            E::signature(),
            F::signature(),
        ])
    }
}

impl<A, B, C, D, E, F, G> Argument for (A, B, C, D, E, F, G)
where
    A: Argument,
    B: Argument,
    C: Argument,
    D: Argument,
    E: Argument,
    F: Argument,
    G: Argument,
{
    fn signature() -> Signature {
        Signature::Struct(vec![
            A::signature(),
            B::signature(),
            C::signature(),
            D::signature(),
            E::signature(),
            F::signature(),
            G::signature(),
        ])
    }
}

impl<A, B, C, D, E, F, G, H> Argument for (A, B, C, D, E, F, G, H)
where
    A: Argument,
    B: Argument,
    C: Argument,
    D: Argument,
    E: Argument,
    F: Argument,
    G: Argument,
    H: Argument,
{
    fn signature() -> Signature {
        Signature::Struct(vec![
            A::signature(),
            B::signature(),
            C::signature(),
            D::signature(),
            E::signature(),
            F::signature(),
            G::signature(),
            H::signature(),
        ])
    }
}

impl<A, B, C, D, E, F, G, H, I> Argument for (A, B, C, D, E, F, G, H, I)
where
    A: Argument,
    B: Argument,
    C: Argument,
    D: Argument,
    E: Argument,
    F: Argument,
    G: Argument,
    H: Argument,
    I: Argument,
{
    fn signature() -> Signature {
        Signature::Struct(vec![
            A::signature(),
            B::signature(),
            C::signature(),
            D::signature(),
            E::signature(),
            F::signature(),
            G::signature(),
            H::signature(),
            I::signature(),
        ])
    }
}

impl<T: Argument> Argument for Variant<T> {
    fn signature() -> Signature {
        Signature::Variant
    }
}

impl<K: Argument, V: Argument> Argument for DictEntry<K, V> {
    fn signature() -> Signature {
        Signature::DictEntry(Box::new((K::signature(), V::signature())))
    }
}

impl Argument for UnixFd {
    fn signature() -> Signature {
        Signature::UnixFd
    }
}

impl<T> Argument for &T
where
    T: ?Sized + Argument,
{
    fn signature() -> Signature {
        T::signature()
    }

    fn arg_type() -> ArgType {
        T::arg_type()
    }
}
