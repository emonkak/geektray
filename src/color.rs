use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error;
use std::fmt;
use std::num;
use std::str::FromStr;

#[derive(Clone, Copy, Debug)]
pub struct Color {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl Color {
    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub const fn from_rgb(color: u32) -> Self {
        let [_, red, green, blue] = color.to_be_bytes();
        Self {
            red,
            green,
            blue,
            alpha: u8::MAX,
        }
    }

    pub const fn from_rgba(color: u32) -> Self {
        let [red, green, blue, alpha] = color.to_be_bytes();
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub fn into_f64_components(self) -> [f64; 4] {
        [
            self.red as f64 / u8::MAX as f64,
            self.green as f64 / u8::MAX as f64,
            self.blue as f64 / u8::MAX as f64,
            self.alpha as f64 / u8::MAX as f64,
        ]
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.alpha < u8::MAX {
            write!(
                f,
                "#{:02x}{:02x}{:02x}{:02x}",
                self.red, self.green, self.blue, self.alpha
            )
        } else {
            write!(f, "#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)
        }
    }
}

impl FromStr for Color {
    type Err = ColorParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.chars().nth(0) != Some('#') {
            return Err(ColorParseError::MissingPrefix);
        }

        let s = &s[1..];

        let (red, green, blue, alpha) = match s.len() {
            6 => {
                let red =
                    u8::from_str_radix(&s[0..=1], 16).map_err(ColorParseError::ParseIntError)?;
                let green =
                    u8::from_str_radix(&s[2..=3], 16).map_err(ColorParseError::ParseIntError)?;
                let blue =
                    u8::from_str_radix(&s[4..=5], 16).map_err(ColorParseError::ParseIntError)?;
                (red, green, blue, u8::MAX)
            }
            8 => {
                let red =
                    u8::from_str_radix(&s[0..=1], 16).map_err(ColorParseError::ParseIntError)?;
                let green =
                    u8::from_str_radix(&s[2..=3], 16).map_err(ColorParseError::ParseIntError)?;
                let blue =
                    u8::from_str_radix(&s[4..=5], 16).map_err(ColorParseError::ParseIntError)?;
                let alpha =
                    u8::from_str_radix(&s[6..=7], 16).map_err(ColorParseError::ParseIntError)?;
                (red, green, blue, alpha)
            }
            len => return Err(ColorParseError::InvalidLength(len).into()),
        };

        Ok(Self {
            red,
            green,
            blue,
            alpha,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ColorParseError {
    MissingPrefix,
    InvalidLength(usize),
    ParseIntError(num::ParseIntError),
}

impl fmt::Display for ColorParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::MissingPrefix => f.write_str("Missing leading '#' descriptor"),
            Self::InvalidLength(len) => write!(f, "Invalid length, expected 6 or 8, got {}", len),
            Self::ParseIntError(error) => error.fmt(f),
        }
    }
}

impl error::Error for ColorParseError {}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ColorVisitor;

        impl<'de> de::Visitor<'de> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a hex RGB color code, such as #ffffff or #ffffffff.")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Color, E>
            where
                E: de::Error,
            {
                Color::from_str(value).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format! {"{}", self})
    }
}
