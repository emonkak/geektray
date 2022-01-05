use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use std::borrow::Cow;
use std::error;
use std::ffi::CString;
use std::fmt;
use std::os::raw::*;

use pango_sys as pango;

#[derive(Debug)]
pub struct FontDescription {
    description: *mut pango::PangoFontDescription,
}

impl FontDescription {
    pub fn new(
        family: FontFamily,
        style: FontStyle,
        weight: FontWeight,
        stretch: FontStretch,
    ) -> Self {
        unsafe {
            let description = pango::pango_font_description_new();

            if let Ok(family_c_str) = CString::new(family.0.as_ref()) {
                pango::pango_font_description_set_family(
                    description,
                    family_c_str.as_ptr() as *const c_char,
                );
            }

            pango::pango_font_description_set_weight(description, weight.0 as i32);

            let style = match style {
                FontStyle::Italic => pango::PANGO_STYLE_ITALIC,
                FontStyle::Normal => pango::PANGO_STYLE_NORMAL,
                FontStyle::Oblique => pango::PANGO_STYLE_OBLIQUE,
            };
            pango::pango_font_description_set_style(description, style);

            let stretch = match stretch {
                FontStretch::UltraCondensed => pango::PANGO_STRETCH_ULTRA_CONDENSED,
                FontStretch::ExtraCondensed => pango::PANGO_STRETCH_EXTRA_CONDENSED,
                FontStretch::Condensed => pango::PANGO_STRETCH_CONDENSED,
                FontStretch::SemiCondensed => pango::PANGO_STRETCH_SEMI_CONDENSED,
                FontStretch::Normal => pango::PANGO_STRETCH_NORMAL,
                FontStretch::SemiExpanded => pango::PANGO_STRETCH_SEMI_EXPANDED,
                FontStretch::Expanded => pango::PANGO_STRETCH_EXPANDED,
                FontStretch::ExtraExpanded => pango::PANGO_STRETCH_EXTRA_EXPANDED,
                FontStretch::UltraExpanded => pango::PANGO_STRETCH_ULTRA_EXPANDED,
            };
            pango::pango_font_description_set_stretch(description, stretch);

            Self { description }
        }
    }

    pub fn as_ptr(&self) -> *mut pango::PangoFontDescription {
        self.description
    }

    pub fn set_font_size(&mut self, font_size: f64) {
        unsafe {
            pango::pango_font_description_set_absolute_size(self.description, font_size);
        }
    }
}

impl Clone for FontDescription {
    fn clone(&self) -> Self {
        unsafe {
            let description = pango::pango_font_description_copy(self.description);
            Self { description }
        }
    }
}

impl Drop for FontDescription {
    fn drop(&mut self) {
        unsafe {
            pango::pango_font_description_free(self.description);
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub struct FontFamily(pub Cow<'static, str>);

impl Default for FontFamily {
    fn default() -> Self {
        Self(Cow::Borrowed("Sans"))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub enum FontStretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

impl Default for FontStretch {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

impl Default for FontStyle {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub struct FontWeight(#[serde(deserialize_with = "deserialize_font_weight")] pub u16);

impl FontWeight {
    pub const THIN: Self = Self(100);
    pub const EXTRA_LIGHT: Self = Self(200);
    pub const LIGHT: Self = Self(300);
    pub const NORMAL: Self = Self(400);
    pub const MEDIUM: Self = Self(500);
    pub const SEMI_BOLD: Self = Self(600);
    pub const BOLD: Self = Self(700);
    pub const EXTRA_BOLD: Self = Self(800);
    pub const BLACK: Self = Self(900);

    pub fn new(weight: u16) -> Result<Self, InvalidWeight> {
        if 1 <= weight && weight <= 1000 {
            Ok(Self(weight))
        } else {
            Err(InvalidWeight)
        }
    }
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

fn deserialize_font_weight<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;

    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = u16;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(
                formatter,
                "an integer value from 1 to 1000 or string representing a font weight."
            )
        }

        fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<u16, E> {
            match s {
                "Thin" => Ok(FontWeight::THIN.0),
                "ExtraLight" => Ok(FontWeight::EXTRA_LIGHT.0),
                "Light" => Ok(FontWeight::LIGHT.0),
                "Normal" => Ok(FontWeight::NORMAL.0),
                "Medium" => Ok(FontWeight::MEDIUM.0),
                "SemiBold" => Ok(FontWeight::SEMI_BOLD.0),
                "Bold" => Ok(FontWeight::BOLD.0),
                "ExtraBold" => Ok(FontWeight::EXTRA_BOLD.0),
                "Black" => Ok(FontWeight::BLACK.0),
                other => Err(de::Error::unknown_variant(
                    other,
                    &[
                        "Thin",
                        "ExtraLight",
                        "Light",
                        "Normal",
                        "Medium",
                        "SemiBold",
                        "Bold",
                        "ExtraBold",
                        "Black",
                    ],
                )),
            }
        }

        fn visit_u16<E: serde::de::Error>(self, n: u16) -> Result<u16, E> {
            if 1 <= n && n <= 1000 {
                Ok(n)
            } else {
                Err(de::Error::invalid_value(
                    de::Unexpected::Unsigned(n as u64),
                    &"The value must be in the range 1 to 1000.",
                ))
            }
        }
    }

    deserializer.deserialize_any(Visitor)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InvalidWeight;

impl fmt::Display for InvalidWeight {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("InvalidWeight: The weight value must be in the range 1 to 1000.")
    }
}

impl error::Error for InvalidWeight {}
