use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::error;
use std::fmt;
use std::num;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Color {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl Color {
    pub const BLACK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 255,
    };

    pub const WHITE: Self = Self {
        red: 255,
        green: 255,
        blue: 255,
        alpha: 255,
    };

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

    pub const fn to_u16_components(&self) -> [u16; 4] {
        let r = self.red as u16;
        let g = self.green as u16;
        let b = self.blue as u16;
        let a = self.alpha as u16;
        [r << 8 | r, g << 8 | g, b << 8 | b, a << 8 | a]
    }

    pub fn to_f64_components(&self) -> [f64; 4] {
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

        match s.len() {
            7 => {
                let red = u8::from_str_radix(&s[1..=2], 16)?;
                let green = u8::from_str_radix(&s[3..=4], 16)?;
                let blue = u8::from_str_radix(&s[5..=6], 16)?;
                Ok(Self {
                    red,
                    green,
                    blue,
                    alpha: u8::MAX,
                })
            }
            9 => {
                let red = u8::from_str_radix(&s[1..=2], 16)?;
                let green = u8::from_str_radix(&s[3..=4], 16)?;
                let blue = u8::from_str_radix(&s[5..=6], 16)?;
                let alpha = u8::from_str_radix(&s[7..=8], 16)?;
                Ok(Self {
                    red,
                    green,
                    blue,
                    alpha,
                })
            }
            len => Err(ColorParseError::InvalidLength(len)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ColorParseError {
    MissingPrefix,
    InvalidLength(usize),
    ParseIntError(num::ParseIntError),
}

impl From<num::ParseIntError> for ColorParseError {
    fn from(error: num::ParseIntError) -> Self {
        ColorParseError::ParseIntError(error)
    }
}

impl fmt::Display for ColorParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::MissingPrefix => f.write_str("Missing leading '#' descriptor"),
            Self::InvalidLength(len) => {
                write!(f, "Expected string of length 6 or 8, but got {}", len)
            }
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
                formatter.write_str("a hex RGB color code, such as #rrggbb or #rrggbbaa.")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_str() {
        assert_eq!(
            Color::from_str("#000000"),
            Ok(Color {
                red: 0x00,
                green: 0x00,
                blue: 0x00,
                alpha: 0xff
            })
        );
        assert_eq!(
            Color::from_str("#ff0000"),
            Ok(Color {
                red: 0xff,
                green: 0x00,
                blue: 0x00,
                alpha: 0xff
            })
        );
        assert_eq!(
            Color::from_str("#00ff00"),
            Ok(Color {
                red: 0x00,
                green: 0xff,
                blue: 0x00,
                alpha: 0xff
            })
        );
        assert_eq!(
            Color::from_str("#0000ff"),
            Ok(Color {
                red: 0x00,
                green: 0x00,
                blue: 0xff,
                alpha: 0xff
            })
        );
        assert_eq!(
            Color::from_str("#ffffff"),
            Ok(Color {
                red: 0xff,
                green: 0xff,
                blue: 0xff,
                alpha: 0xff
            })
        );

        assert_eq!(
            Color::from_str("#00000000"),
            Ok(Color {
                red: 0x00,
                green: 0x00,
                blue: 0x00,
                alpha: 0x00
            })
        );
        assert_eq!(
            Color::from_str("#ff000000"),
            Ok(Color {
                red: 0xff,
                green: 0x00,
                blue: 0x00,
                alpha: 0x00
            })
        );
        assert_eq!(
            Color::from_str("#00ff0000"),
            Ok(Color {
                red: 0x00,
                green: 0xff,
                blue: 0x00,
                alpha: 0x00
            })
        );
        assert_eq!(
            Color::from_str("#0000ff00"),
            Ok(Color {
                red: 0x00,
                green: 0x00,
                blue: 0xff,
                alpha: 0x00
            })
        );
        assert_eq!(
            Color::from_str("#000000ff"),
            Ok(Color {
                red: 0x00,
                green: 0x00,
                blue: 0x00,
                alpha: 0xff
            })
        );
        assert_eq!(
            Color::from_str("#ffffffff"),
            Ok(Color {
                red: 0xff,
                green: 0xff,
                blue: 0xff,
                alpha: 0xff
            })
        );

        assert_eq!(
            Color::from_str("#12345678"),
            Ok(Color {
                red: 0x12,
                green: 0x34,
                blue: 0x56,
                alpha: 0x78
            })
        );
        assert_eq!(
            Color::from_str("#abcdef"),
            Ok(Color {
                red: 0xab,
                green: 0xcd,
                blue: 0xef,
                alpha: 0xff
            })
        );

        assert_eq!(
            Color::from_str("12345678"),
            Err(ColorParseError::MissingPrefix)
        );
        assert_eq!(
            Color::from_str("#12345"),
            Err(ColorParseError::InvalidLength(6))
        );
        assert!(matches!(
            Color::from_str("#abcdefgh"),
            Err(ColorParseError::ParseIntError(_))
        ));
    }
}
