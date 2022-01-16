use std::borrow::Cow;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Variant<T>(pub T);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DictEntry<K, V>(pub K, pub V);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct UnixFd(pub RawFd);

#[derive(Clone, Debug, PartialEq)]
pub enum Any<'a> {
    Boolean(bool),
    Byte(u32),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    Uint16(u16),
    Uint32(u32),
    Uint64(u64),
    Double(f64),
    String(Cow<'a, str>),
    ObjectPath(ObjectPath<'a>),
    Signature(ArgType),
    Array(Vec<Any<'a>>, Signature),
    Struct(Vec<Any<'a>>),
    Variant(Box<Any<'a>>),
    DictEntry(Box<DictEntry<Any<'a>, Any<'a>>>),
    UnixFd(UnixFd),
}

impl<'a> Any<'a> {
    pub fn signature(&self) -> Signature {
        match self {
            Self::Boolean(_) => Signature::Boolean,
            Self::Byte(_) => Signature::Byte,
            Self::Int16(_) => Signature::Int16,
            Self::Int32(_) => Signature::Int32,
            Self::Int64(_) => Signature::Int64,
            Self::Uint16(_) => Signature::Uint16,
            Self::Uint32(_) => Signature::Uint32,
            Self::Uint64(_) => Signature::Uint64,
            Self::Double(_) => Signature::Double,
            Self::String(_) => Signature::String,
            Self::ObjectPath(_) => Signature::ObjectPath,
            Self::Signature(_) => Signature::Signature,
            Self::Array(_, signature) => Signature::Array(Box::new(signature.clone())),
            Self::Struct(values) => {
                Signature::Struct(values.iter().map(|value| value.signature()).collect())
            }
            Self::Variant(_) => Signature::Variant,
            Self::DictEntry(entry) => {
                let DictEntry(key, value) = entry.as_ref();
                Signature::DictEntry(Box::new((key.signature(), value.signature())))
            }
            Self::UnixFd(_) => Signature::UnixFd,
        }
    }
}

pub union BasicValue {
    pub bool: bool,
    pub byte: u8,
    pub i16: i16,
    pub i32: i32,
    pub i64: i64,
    pub u16: u16,
    pub u32: u32,
    pub u64: u64,
    pub f64: f64,
    pub str: *const c_char,
}
