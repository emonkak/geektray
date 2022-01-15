use std::borrow::Cow;
use std::ffi::{CStr, CString};
use std::fmt;
use std::fmt::Write as _;
use std::iter::Peekable;
use std::os::raw::*;
use std::os::unix::io::RawFd;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Signature {
    Boolean,
    Byte,
    Int16,
    Int32,
    Int64,
    Uint16,
    Uint32,
    Uint64,
    Double,
    String,
    ObjectPath,
    Signature,
    Array(Box<Signature>),
    Struct(Vec<Signature>),
    Variant,
    DictEntry(Box<(Signature, Signature)>),
    UnixFd,
}

impl Signature {
    pub fn parse(bytes: &[u8]) -> Result<Self, SignatureParseError> {
        SignatureParser(bytes.iter().copied().peekable()).parse()
    }

    pub const fn arg_type(&self) -> ArgType {
        match self {
            Signature::Byte => ArgType::Byte,
            Signature::Boolean => ArgType::Boolean,
            Signature::Int16 => ArgType::Int16,
            Signature::Uint16 => ArgType::Uint16,
            Signature::Int32 => ArgType::Int32,
            Signature::Uint32 => ArgType::Uint32,
            Signature::Int64 => ArgType::Int64,
            Signature::Uint64 => ArgType::Uint64,
            Signature::Double => ArgType::Double,
            Signature::String => ArgType::String,
            Signature::ObjectPath => ArgType::ObjectPath,
            Signature::Signature => ArgType::Signature,
            Signature::Array(_) => ArgType::Array,
            Signature::Struct(_) => ArgType::Struct,
            Signature::Variant => ArgType::Variant,
            Signature::DictEntry(_) => ArgType::DictEntry,
            Signature::UnixFd => ArgType::UnixFd,
        }
    }

    pub fn is_string_like(&self) -> bool {
        match self {
            Signature::Array(element) if *element.as_ref() == Signature::Byte => true,
            Signature::String => true,
            Signature::ObjectPath => true,
            Signature::Signature => true,
            _ => false,
        }
    }

    pub fn is_byte_array(&self) -> bool {
        matches!(self, Signature::Array(element) if *element.as_ref() == Signature::Byte)
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Signature::Byte => f.write_char('y'),
            Signature::Boolean => f.write_char('b'),
            Signature::Int16 => f.write_char('n'),
            Signature::Uint16 => f.write_char('q'),
            Signature::Int32 => f.write_char('i'),
            Signature::Uint32 => f.write_char('u'),
            Signature::Int64 => f.write_char('x'),
            Signature::Uint64 => f.write_char('t'),
            Signature::Double => f.write_char('d'),
            Signature::String => f.write_char('s'),
            Signature::ObjectPath => f.write_char('o'),
            Signature::Signature => f.write_char('g'),
            Signature::Array(signature) => {
                f.write_char('a')?;
                signature.fmt(f)?;
                Ok(())
            }
            Signature::Struct(signatures) => {
                f.write_char('(')?;
                for signature in signatures {
                    signature.fmt(f)?;
                }
                f.write_char(')')?;
                Ok(())
            }
            Signature::Variant => f.write_char('v'),
            Signature::DictEntry(entry) => {
                let (key, value) = entry.as_ref();
                f.write_char('{')?;
                key.fmt(f)?;
                value.fmt(f)?;
                f.write_char('}')?;
                Ok(())
            }
            Signature::UnixFd => f.write_char('h'),
        }
    }
}

struct SignatureParser<I>(Peekable<I>)
where
    I: Iterator<Item = u8>;

impl<I> SignatureParser<I>
where
    I: Iterator<Item = u8>,
{
    pub fn parse(&mut self) -> Result<Signature, SignatureParseError> {
        let signature = self.parse_any()?;
        if let Some(c) = self.0.next() {
            return Err(SignatureParseError::TrailingCharacter(c));
        }
        Ok(signature)
    }

    fn parse_any(&mut self) -> Result<Signature, SignatureParseError> {
        match self.consume()? {
            b'(' => {
                let mut signatures = Vec::new();

                loop {
                    if self.peek()? == b')' {
                        self.0.next();
                        break Ok(Signature::Struct(signatures));
                    }
                    signatures.push(self.parse_any()?);
                }
            }
            b'{' => {
                let key = self.parse_any()?;
                let value = self.parse_any()?;

                match self.consume()? {
                    b'}' => Ok(Signature::DictEntry(Box::new((key, value)))),
                    c => Err(SignatureParseError::DictEntryEndExpected(c)),
                }
            }
            b'a' => Ok(Signature::Array(Box::new(self.parse_any()?))),
            b's' => Ok(Signature::String),
            b'y' => Ok(Signature::Byte),
            b'n' => Ok(Signature::Int16),
            b'q' => Ok(Signature::Uint16),
            b'i' => Ok(Signature::Int32),
            b'u' => Ok(Signature::Uint32),
            b'x' => Ok(Signature::Int64),
            b't' => Ok(Signature::Uint64),
            b'd' => Ok(Signature::Double),
            b'o' => Ok(Signature::ObjectPath),
            b'g' => Ok(Signature::Signature),
            b'v' => Ok(Signature::Variant),
            b'h' => Ok(Signature::UnixFd),
            c => Err(SignatureParseError::UnexpectedCharacter(c)),
        }
    }

    fn consume(&mut self) -> Result<u8, SignatureParseError> {
        if let Some(current) = self.0.next() {
            Ok(current)
        } else {
            Err(SignatureParseError::UnexpectedEOF)
        }
    }

    fn peek(&mut self) -> Result<u8, SignatureParseError> {
        if let Some(current) = self.0.peek() {
            Ok(*current)
        } else {
            Err(SignatureParseError::UnexpectedEOF)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignatureParseError {
    DictEntryEndExpected(u8),
    TrailingCharacter(u8),
    UnexpectedCharacter(u8),
    UnexpectedEOF,
}

impl fmt::Display for SignatureParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SignatureParseError::DictEntryEndExpected(c) => {
                write!(
                    f,
                    "'}}' is expected for DICT_ENTRY but actually found '{}'",
                    *c as char
                )
            }
            SignatureParseError::TrailingCharacter(c) => {
                write!(f, "Expected EOF but got character '{}'", *c as char)
            }
            SignatureParseError::UnexpectedCharacter(c) => {
                write!(f, "Unexpected character '{}'", *c as char)
            }
            SignatureParseError::UnexpectedEOF => f.write_str("Unexpected EOF"),
        }
    }
}

pub union BasicValue {
    pub byte: u8,
    pub bool: bool,
    pub i16: i16,
    pub i32: i32,
    pub i64: i64,
    pub u16: u16,
    pub u32: u32,
    pub u64: u64,
    pub f64: f64,
    pub str: *const c_char,
    pub fd: RawFd,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ArgType {
    Boolean = b'b',
    Byte = b'y',
    Int16 = b'n',
    Int32 = b'i',
    Int64 = b'x',
    Uint16 = b'q',
    Uint32 = b'u',
    Uint64 = b't',
    Double = b'd',
    String = b's',
    ObjectPath = b'o',
    Signature = b'g',
    Array = b'a',
    Struct = b'r',
    Variant = b'v',
    DictEntry = b'e',
    UnixFd = b'h',
    Invalid = 0,
}

impl ArgType {
    pub fn is_basic(&self) -> bool {
        match self {
            Self::Boolean
            | Self::Byte
            | Self::Int16
            | Self::Int32
            | Self::Int64
            | Self::Uint16
            | Self::Uint32
            | Self::Uint64
            | Self::Double
            | Self::String
            | Self::ObjectPath
            | Self::Signature
            | Self::UnixFd => true,
            _ => false,
        }
    }

    pub fn is_container(&self) -> bool {
        match self {
            Self::Array | Self::Struct | Self::Variant | Self::DictEntry => true,
            _ => false,
        }
    }

    pub fn to_byte(&self) -> u8 {
        (*self) as u8
    }
}

impl From<u8> for ArgType {
    fn from(c: u8) -> Self {
        match c {
            b'y' => Self::Byte,
            b'b' => Self::Boolean,
            b'n' => Self::Int16,
            b'q' => Self::Uint16,
            b'i' => Self::Int32,
            b'u' => Self::Uint32,
            b'x' => Self::Int64,
            b't' => Self::Uint64,
            b'd' => Self::Double,
            b's' => Self::String,
            b'o' => Self::ObjectPath,
            b'g' => Self::Signature,
            b'a' => Self::Array,
            b'r' => Self::Struct,
            b'v' => Self::Variant,
            b'e' => Self::DictEntry,
            b'h' => Self::UnixFd,
            _ => Self::Invalid,
        }
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

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ObjectPath<'a>(pub Cow<'a, str>);

impl<'a> From<String> for ObjectPath<'a> {
    fn from(value: String) -> Self {
        Self(Cow::Owned(value))
    }
}

impl<'a> From<&'a str> for ObjectPath<'a> {
    fn from(value: &'a str) -> Self {
        Self(Cow::Borrowed(value))
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Variant<T>(pub T);

impl<T: Argument> Argument for Variant<T> {
    fn signature() -> Signature {
        Signature::Variant
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DictEntry<K, V>(pub K, pub V);

impl<K: Argument, V: Argument> Argument for DictEntry<K, V> {
    fn signature() -> Signature {
        Signature::DictEntry(Box::new((K::signature(), V::signature())))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct UnixFd(pub RawFd);

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
