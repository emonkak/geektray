use std::cmp;
use std::collections::HashMap;
use std::ffi::CString;
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::str::CharIndices;
use x11::xft;
use x11::xlib;
use x11::xrender;

use color::Color;
use font::{FontDescriptor, FontFamily, FontStretch, FontStyle};
use fontconfig as fc;
use geometrics::Rectangle;

const SERIF_FAMILY: &'static str = "Serif\0";
const SANS_SERIF_FAMILY: &'static str = "Sans\0";
const MONOSPACE_FAMILY: &'static str = "Monospace\0";

pub struct TextRenderer {
    loaded_fonts: HashMap<fc::FcChar32, Option<*mut xft::XftFont>>,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            loaded_fonts: HashMap::new(),
        }
    }

    pub fn render_single_line(
        &mut self,
        display: *mut xlib::Display,
        draw: *mut xft::XftDraw,
        text: Text,
        bounds: Rectangle,
    ) {
        let origin_x = match text.horizontal_align {
            HorizontalAlign::Left => bounds.x,
            HorizontalAlign::Right => bounds.x + bounds.width,
            HorizontalAlign::Center => unimplemented!(),
        };
        let origin_y = match text.vertical_align {
            VerticalAlign::Top => bounds.y,
            VerticalAlign::Middle => bounds.y + bounds.height / 2.0 - text.font_size / 2.0,
            VerticalAlign::Bottom => bounds.y + bounds.height - text.font_size,
        };

        let mut x_offset = 0;

        for chunk in ChunkIter::new(text.content, &text.font_set) {
            let font = match self.load_font(display, text.font_set.pattern, chunk.font) {
                Some(font) => font,
                _ => continue,
            };

            unsafe {
                let mut extents: xrender::XGlyphInfo = mem::MaybeUninit::uninit().assume_init();
                xft::XftTextExtentsUtf8(
                    display,
                    font,
                    chunk.text.as_ptr(),
                    chunk.text.len() as i32,
                    &mut extents,
                );

                let ascent = (*font).ascent;
                let height = cmp::max(text.font_size as i32, ascent);
                let y_adjustment = (height - ascent) / 2;

                xft::XftDrawStringUtf8(
                    draw,
                    &mut text.color.as_xft_color(),
                    font,
                    origin_x as i32 + x_offset + (extents.x as i32),
                    origin_y as i32 + y_adjustment + (extents.height as i32),
                    chunk.text.as_ptr(),
                    chunk.text.len() as i32,
                );

                x_offset += extents.width as i32;
            }
        }
    }

    fn load_font(
        &mut self,
        display: *mut xlib::Display,
        pattern: *mut fc::FcPattern,
        font: *mut fc::FcPattern,
    ) -> Option<*mut xft::XftFont> {
        *self
            .loaded_fonts
            .entry(unsafe { fc::FcPatternHash(font) })
            .or_insert_with(|| unsafe {
                let pattern = fc::FcFontRenderPrepare(ptr::null_mut(), pattern, font);

                let font = xft::XftFontOpenPattern(display, pattern.cast());
                if font.is_null() {
                    fc::FcPatternDestroy(pattern);
                    return None;
                }

                Some(font)
            })
    }
}

pub struct FontSet {
    pattern: *mut fc::FcPattern,
    fontset: *mut fc::FcFontSet,
    charsets: Vec<*mut fc::FcCharSet>,
    coverage: *mut fc::FcCharSet,
}

impl FontSet {
    pub fn new(font_descriptor: FontDescriptor) -> Option<FontSet> {
        unsafe {
            let pattern = create_pattern(&font_descriptor);

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

    fn default_font(&self) -> *mut fc::FcPattern {
        unsafe { *(*self.fontset).fonts.offset(0) }
    }

    fn match_font(&self, c: char) -> Option<*mut fc::FcPattern> {
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

pub struct Text<'a> {
    pub content: &'a str,
    pub color: &'a Color,
    pub font_size: f32,
    pub font_set: &'a FontSet,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
}

pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

pub enum HorizontalAlign {
    Left,
    Center,
    Right,
}

struct Chunk<'a> {
    text: &'a str,
    font: *mut fc::FcPattern,
}

struct ChunkIter<'a> {
    fontset: &'a FontSet,
    current_font: Option<*mut fc::FcPattern>,
    current_index: usize,
    inner_iter: CharIndices<'a>,
    source: &'a str,
}

impl<'a> ChunkIter<'a> {
    fn new(source: &'a str, fontset: &'a FontSet) -> Self {
        Self {
            fontset,
            current_font: None,
            current_index: 0,
            inner_iter: source.char_indices(),
            source,
        }
    }
}

impl<'a> Iterator for ChunkIter<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((i, c)) = self.inner_iter.next() {
            let matched_font = self.fontset.match_font(c);
            if i == 0 {
                self.current_font = matched_font;
            } else if self.current_font != matched_font {
                let result = Some(Chunk {
                    text: &self.source[self.current_index..i],
                    font: self.current_font.unwrap_or(self.fontset.default_font()),
                });
                self.current_font = matched_font;
                self.current_index = i;
                return result;
            }
        }

        if self.current_index < self.source.len() {
            let result = Some(Chunk {
                text: &self.source[self.current_index..],
                font: self.current_font.unwrap_or(self.fontset.default_font()),
            });
            self.current_font = None;
            self.current_index = self.source.len();
            return result;
        }

        None
    }
}

unsafe fn create_pattern(descriptor: &FontDescriptor) -> *mut fc::FcPattern {
    let pattern = fc::FcPatternCreate();

    match &descriptor.family {
        FontFamily::Name(name) => {
            if let Ok(name_str) = CString::new(name.as_str()) {
                fc::FcPatternAddString(
                    pattern,
                    fc::FC_FAMILY.as_ptr() as *mut c_char,
                    name_str.as_ptr() as *mut c_uchar,
                );
            }
        }
        FontFamily::Serif => {
            fc::FcPatternAddString(
                pattern,
                fc::FC_FAMILY.as_ptr() as *mut c_char,
                SERIF_FAMILY.as_ptr(),
            );
        }
        FontFamily::SansSerif => {
            fc::FcPatternAddString(
                pattern,
                fc::FC_FAMILY.as_ptr() as *mut c_char,
                SANS_SERIF_FAMILY.as_ptr(),
            );
        }
        FontFamily::Monospace => {
            fc::FcPatternAddString(
                pattern,
                fc::FC_FAMILY.as_ptr() as *mut c_char,
                MONOSPACE_FAMILY.as_ptr(),
            );
        }
    };

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
