use pango_sys as pango;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryFrom;
use std::error;
use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::*;

#[derive(Debug)]
pub struct FontDescription(*mut pango::PangoFontDescription);

impl FontDescription {
    pub fn new(
        family: &FontFamily,
        style: FontStyle,
        weight: FontWeight,
        stretch: FontStretch,
    ) -> Self {
        unsafe {
            let description = pango::pango_font_description_new();

            if let Ok(family_c_str) = CString::new(family.0.as_str()) {
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

            Self(description)
        }
    }

    pub fn as_mut_ptr(&self) -> *mut pango::PangoFontDescription {
        self.0
    }

    pub fn font_family(&self) -> FontFamily {
        let family =
            unsafe { CStr::from_ptr(pango::pango_font_description_get_family(self.0) as *mut _) };
        FontFamily(family.to_string_lossy().into_owned())
    }

    pub fn font_stretch(&self) -> FontStretch {
        let stretch = unsafe { pango::pango_font_description_get_stretch(self.0) };
        if stretch == pango::PANGO_STRETCH_ULTRA_CONDENSED {
            FontStretch::UltraCondensed
        } else if stretch == pango::PANGO_STRETCH_EXTRA_CONDENSED {
            FontStretch::ExtraCondensed
        } else if stretch == pango::PANGO_STRETCH_CONDENSED {
            FontStretch::Condensed
        } else if stretch == pango::PANGO_STRETCH_SEMI_CONDENSED {
            FontStretch::SemiCondensed
        } else if stretch == pango::PANGO_STRETCH_NORMAL {
            FontStretch::Normal
        } else if stretch == pango::PANGO_STRETCH_SEMI_EXPANDED {
            FontStretch::SemiExpanded
        } else if stretch == pango::PANGO_STRETCH_EXPANDED {
            FontStretch::Expanded
        } else if stretch == pango::PANGO_STRETCH_EXTRA_EXPANDED {
            FontStretch::ExtraExpanded
        } else if stretch == pango::PANGO_STRETCH_ULTRA_EXPANDED {
            FontStretch::UltraExpanded
        } else {
            unreachable!("unexpected stretch: {}", stretch);
        }
    }

    pub fn font_style(&self) -> FontStyle {
        let style = unsafe { pango::pango_font_description_get_style(self.0) };
        if style == pango::PANGO_STYLE_NORMAL {
            FontStyle::Normal
        } else if style == pango::PANGO_STYLE_OBLIQUE {
            FontStyle::Oblique
        } else if style == pango::PANGO_STYLE_ITALIC {
            FontStyle::Italic
        } else {
            unreachable!("unexpected style: {}", style);
        }
    }

    pub fn font_weight(&self) -> FontWeight {
        let weight = unsafe { pango::pango_font_description_get_weight(self.0) };
        FontWeight::try_from(weight as u16).expect("unexpected weight")
    }

    pub fn set_font_size(&mut self, font_size: f64) {
        unsafe {
            pango::pango_font_description_set_absolute_size(self.0, font_size);
        }
    }
}

impl Clone for FontDescription {
    fn clone(&self) -> Self {
        unsafe { Self(pango::pango_font_description_copy(self.0)) }
    }
}

impl Drop for FontDescription {
    fn drop(&mut self) {
        unsafe {
            pango::pango_font_description_free(self.0);
        }
    }
}

impl From<SerializableFontDescription> for FontDescription {
    fn from(serializable: SerializableFontDescription) -> FontDescription {
        FontDescription::new(
            &serializable.family,
            serializable.style,
            serializable.weight,
            serializable.stretch,
        )
    }
}

impl PartialEq for FontDescription {
    fn eq(&self, other: &Self) -> bool {
        unsafe { pango::pango_font_description_equal(self.0, other.0) != 0 }
    }
}

impl<'de> Deserialize<'de> for FontDescription {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let font_description: FontDescription =
            SerializableFontDescription::deserialize(deserializer)?.into();
        Ok(font_description)
    }
}

impl Serialize for FontDescription {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let serializable = SerializableFontDescription::from(self);
        serializable.serialize(serializer)
    }
}

#[derive(Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default)]
struct SerializableFontDescription {
    pub family: FontFamily,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub stretch: FontStretch,
}

impl From<&FontDescription> for SerializableFontDescription {
    fn from(font_description: &FontDescription) -> Self {
        Self {
            family: font_description.font_family(),
            style: font_description.font_style(),
            weight: font_description.font_weight(),
            stretch: font_description.font_stretch(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
pub struct FontFamily(String);

impl Default for FontFamily {
    fn default() -> Self {
        Self("Sans".to_owned())
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
pub struct FontWeight(u16);

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
}

impl TryFrom<u16> for FontWeight {
    type Error = InvalidWeight;

    fn try_from(weight: u16) -> Result<Self, Self::Error> {
        if 1 <= weight && weight <= 1000 {
            Ok(Self(weight))
        } else {
            Err(InvalidWeight)
        }
    }
}

impl From<FontWeight> for u16 {
    fn from(font_weight: FontWeight) -> Self {
        font_weight.0
    }
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidWeight;

impl fmt::Display for InvalidWeight {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("the weight value is invalid, it must be in the range 1 to 1000")
    }
}

impl error::Error for InvalidWeight {}
