use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use std::borrow::Cow;
use std::error;
use std::ffi::CString;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::os::raw::*;
use std::ptr;

use crate::fontconfig as fc;

#[derive(Debug)]
pub struct FontSet {
    pattern: *mut fc::FcPattern,
    fontset: *mut fc::FcFontSet,
    charsets: Vec<*mut fc::FcCharSet>,
    coverage: *mut fc::FcCharSet,
}

impl FontSet {
    pub fn new(font_descriptor: FontDescriptor) -> Option<FontSet> {
        unsafe {
            let pattern = prepare_pattern(&font_descriptor);

            fc::FcConfigSubstitute(ptr::null_mut(), pattern, fc::FcMatchKind::Pattern);
            fc::FcDefaultSubstitute(pattern);

            let mut result: fc::FcResult = fc::FcResult::NoMatch;
            let fontset = fc::FcFontSort(ptr::null_mut(), pattern, 1, ptr::null_mut(), &mut result);

            if result != fc::FcResult::Match || (*fontset).nfont == 0 {
                return None;
            }

            let mut coverage = fc::FcCharSetNew();
            let mut charsets = Vec::with_capacity((*fontset).nfont as usize);

            for i in 0..(*fontset).nfont {
                let font = *(*fontset).fonts.offset(i as isize);

                let mut charset: *mut fc::FcCharSet = ptr::null_mut();
                let result = fc::FcPatternGetCharSet(
                    font,
                    fc::FC_CHARSET.as_ptr() as *mut c_char,
                    0,
                    &mut charset,
                );

                if result == fc::FcResult::Match {
                    coverage = fc::FcCharSetUnion(coverage, charset);
                }

                charsets.push(charset);
            }

            Some(Self {
                pattern,
                fontset,
                charsets,
                coverage,
            })
        }
    }

    pub fn pattern(&self) -> *mut fc::FcPattern {
        self.pattern
    }

    pub fn default_font(&self) -> *mut fc::FcPattern {
        unsafe { *(*self.fontset).fonts.offset(0) }
    }

    pub fn match_font(&self, c: char) -> Option<*mut fc::FcPattern> {
        unsafe {
            if fc::FcCharSetHasChar(self.coverage, c as u32) == 0 {
                return None;
            }

            for i in 0..(*self.fontset).nfont {
                let font = *(*self.fontset).fonts.offset(i as isize);
                let charset = self.charsets[i as usize];

                if !charset.is_null() && fc::FcCharSetHasChar(charset, c as u32) != 0 {
                    return Some(font);
                }
            }
        }

        None
    }
}

impl Drop for FontSet {
    fn drop(&mut self) {
        unsafe {
            for charset in self.charsets.iter() {
                fc::FcCharSetDestroy(*charset);
            }
            fc::FcCharSetDestroy(self.coverage);
            fc::FcFontSetDestroy(self.fontset);
            fc::FcPatternDestroy(self.pattern);
        }
    }
}

unsafe fn prepare_pattern(descriptor: &FontDescriptor) -> *mut fc::FcPattern {
    let pattern = fc::FcPatternCreate();

    if let Ok(name_str) = CString::new(descriptor.family.0.as_ref()) {
        fc::FcPatternAddString(
            pattern,
            fc::FC_FAMILY.as_ptr() as *mut c_char,
            name_str.as_ptr() as *mut c_uchar,
        );
    }

    fc::FcPatternAddDouble(
        pattern,
        fc::FC_WEIGHT.as_ptr() as *mut c_char,
        descriptor.weight.0 as f64,
    );

    let slant = match descriptor.style {
        FontStyle::Italic => fc::FC_SLANT_ITALIC,
        FontStyle::Normal => fc::FC_SLANT_ROMAN,
        FontStyle::Oblique => fc::FC_SLANT_OBLIQUE,
    };
    fc::FcPatternAddInteger(pattern, fc::FC_SLANT.as_ptr() as *mut c_char, slant);

    let width = match descriptor.stretch {
        FontStretch::UltraCondensed => fc::FC_WIDTH_ULTRACONDENSED,
        FontStretch::ExtraCondensed => fc::FC_WIDTH_EXTRACONDENSED,
        FontStretch::Condensed => fc::FC_WIDTH_CONDENSED,
        FontStretch::SemiCondensed => fc::FC_WIDTH_SEMICONDENSED,
        FontStretch::Normal => fc::FC_WIDTH_NORMAL,
        FontStretch::SemiExpanded => fc::FC_WIDTH_SEMIEXPANDED,
        FontStretch::Expanded => fc::FC_WIDTH_EXPANDED,
        FontStretch::ExtraExpanded => fc::FC_WIDTH_EXTRAEXPANDED,
        FontStretch::UltraExpanded => fc::FC_WIDTH_ULTRAEXPANDED,
    };
    fc::FcPatternAddInteger(pattern, fc::FC_WIDTH.as_ptr() as *mut c_char, width);

    pattern
}

#[derive(Debug)]
pub struct FontId(pub *mut fc::FcPattern);

impl PartialEq for FontId {
    fn eq(&self, other: &Self) -> bool {
        unsafe { fc::FcPatternEqual(self.0, other.0) != 0 }
    }
}

impl Eq for FontId {}

impl Hash for FontId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let hash = unsafe { fc::FcPatternHash(self.0) };
        state.write_u32(hash);
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct FontDescriptor {
    pub family: FontFamily,
    pub style: FontStyle,
    pub weight: FontWeight,
    pub stretch: FontStretch,
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
