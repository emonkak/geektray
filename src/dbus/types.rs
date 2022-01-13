use std::ffi::CStr;
use std::fmt;
use std::fmt::Write as _;
use std::iter::Peekable;
use std::os::raw::*;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Signature {
    Byte,
    Boolean,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Int64,
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
        if let Some(byte) = self.0.next() {
            return Err(SignatureParseError::TrailingCharacter(byte as char));
        }
        Ok(signature)
    }

    fn parse_any(&mut self) -> Result<Signature, SignatureParseError> {
        match self.consume()? {
            '(' => {
                let mut signatures = Vec::new();

                loop {
                    if self.peek()? == ')' {
                        self.0.next();
                        break Ok(Signature::Struct(signatures));
                    }
                    signatures.push(self.parse_any()?);
                }
            }
            '{' => {
                let key = self.parse_any()?;
                let value = self.parse_any()?;

                match self.consume()? {
                    '}' => Ok(Signature::DictEntry(Box::new((key, value)))),
                    c => Err(SignatureParseError::DictEntryEndExpected(c)),
                }
            }
            'a' => Ok(Signature::Array(Box::new(self.parse_any()?))),
            's' => Ok(Signature::String),
            'y' => Ok(Signature::Byte),
            'n' => Ok(Signature::Int16),
            'q' => Ok(Signature::Uint16),
            'i' => Ok(Signature::Int32),
            'u' => Ok(Signature::Uint32),
            'x' => Ok(Signature::Int64),
            't' => Ok(Signature::Uint64),
            'd' => Ok(Signature::Double),
            'o' => Ok(Signature::ObjectPath),
            'g' => Ok(Signature::Signature),
            'v' => Ok(Signature::Variant),
            'h' => Ok(Signature::UnixFd),
            c => Err(SignatureParseError::UnexpectedCharacter(c)),
        }
    }

    fn consume(&mut self) -> Result<char, SignatureParseError> {
        if let Some(current) = self.0.next() {
            Ok(current as char)
        } else {
            Err(SignatureParseError::UnexpectedEOF)
        }
    }

    fn peek(&mut self) -> Result<char, SignatureParseError> {
        if let Some(current) = self.0.peek() {
            Ok(*current as char)
        } else {
            Err(SignatureParseError::UnexpectedEOF)
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SignatureParseError {
    DictEntryEndExpected(char),
    TrailingCharacter(char),
    UnexpectedCharacter(char),
    UnexpectedEOF,
}

impl fmt::Display for SignatureParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SignatureParseError::DictEntryEndExpected(c) => {
                write!(
                    f,
                    "'}}' is expected for DICT_ENTRY but actually found '{}'",
                    c
                )
            }
            SignatureParseError::TrailingCharacter(c) => {
                write!(f, "Expected EOF but got character '{}'", c)
            }
            SignatureParseError::UnexpectedCharacter(c) => {
                write!(f, "Unexpected character '{}'", c)
            }
            SignatureParseError::UnexpectedEOF => f.write_str("Unexpected EOF"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ArgType {
    Byte,
    Boolean,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Int64,
    Uint64,
    Double,
    String,
    ObjectPath,
    Signature,
    Array,
    Struct,
    Variant,
    DictEntry,
    UnixFd,
    Invalid,
}

impl From<c_int> for ArgType {
    fn from(c: c_int) -> Self {
        match c as u8 {
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

impl From<ArgType> for c_int {
    fn from(arg_type: ArgType) -> Self {
        match arg_type {
            ArgType::Byte => 'y' as c_int,
            ArgType::Boolean => 'b' as c_int,
            ArgType::Int16 => 'n' as c_int,
            ArgType::Uint16 => 'q' as c_int,
            ArgType::Int32 => 'i' as c_int,
            ArgType::Uint32 => 'u' as c_int,
            ArgType::Int64 => 'x' as c_int,
            ArgType::Uint64 => 't' as c_int,
            ArgType::Double => 'd' as c_int,
            ArgType::String => 's' as c_int,
            ArgType::ObjectPath => 'o' as c_int,
            ArgType::Signature => 'g' as c_int,
            ArgType::Array => 'a' as c_int,
            ArgType::Struct => 'r' as c_int,
            ArgType::Variant => 'v' as c_int,
            ArgType::DictEntry => 'e' as c_int,
            ArgType::UnixFd => 'h' as c_int,
            ArgType::Invalid => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Variant<T>(pub T);

pub trait Argument {
    fn signature() -> Signature;

    fn arg_type() -> ArgType {
        Self::signature().arg_type()
    }
}

impl Argument for Signature {
    fn signature() -> Signature {
        Signature::Signature
    }
}

impl Argument for u8 {
    fn signature() -> Signature {
        Signature::Byte
    }
}

impl Argument for bool {
    fn signature() -> Signature {
        Signature::Boolean
    }
}

impl Argument for i16 {
    fn signature() -> Signature {
        Signature::Int16
    }
}

impl Argument for u16 {
    fn signature() -> Signature {
        Signature::Uint16
    }
}

impl Argument for i32 {
    fn signature() -> Signature {
        Signature::Int32
    }
}

impl Argument for u32 {
    fn signature() -> Signature {
        Signature::Uint32
    }
}

impl Argument for i64 {
    fn signature() -> Signature {
        Signature::Int64
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

impl Argument for CStr {
    fn signature() -> Signature {
        Signature::String
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

impl<K: Argument, V: Argument> Argument for (K, V) {
    fn signature() -> Signature {
        Signature::DictEntry(Box::new((K::signature(), V::signature())))
    }
}

impl Argument for () {
    fn signature() -> Signature {
        Signature::Struct(Vec::new())
    }
}

impl<T: Argument> Argument for Variant<T> {
    fn signature() -> Signature {
        Signature::Variant
    }
}

impl<T: Argument> Argument for Option<T> {
    fn signature() -> Signature {
        Signature::Variant
    }
}

impl<T> Argument for &T
where
    T: Argument + ?Sized,
{
    fn signature() -> Signature {
        T::signature()
    }

    fn arg_type() -> ArgType {
        T::arg_type()
    }
}
