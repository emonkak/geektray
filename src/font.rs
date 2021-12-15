use fontconfig::fontconfig as fc;
use std::cmp;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::ffi::CString;
use std::hash::Hash;
use std::hash::Hasher;
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::str::CharIndices;
use x11::xft;
use x11::xlib;
use x11::xrender;

static FC_CHARSET: &'static [u8] = b"charset\0";
static FC_FAMILY: &'static [u8] = b"family\0";
static FC_PIXEL_SIZE: &'static [u8] = b"pixelsize\0";
static FC_SLANT: &'static [u8] = b"slant\0";
static FC_WEIGHT: &'static [u8] = b"weight\0";

pub struct FontSet {
    font_descriptor: FontDescriptor,
    pattern: *mut fc::FcPattern,
    fontset: *mut fc::FcFontSet,
    charsets: Vec<*mut fc::FcCharSet>,
    coverage: *mut fc::FcCharSet,
}

impl FontSet {
    pub fn new(font_descriptor: FontDescriptor) -> Option<FontSet> {
        unsafe {
            let pattern = font_descriptor.to_pattern();

            fc::FcConfigSubstitute(ptr::null_mut(), pattern, fc::FcMatchPattern);
            fc::FcDefaultSubstitute(pattern);

            let mut result: fc::FcResult = fc::FcResultNoMatch;
            let fontset = fc::FcFontSort(
                ptr::null_mut(),
                pattern,
                1,
                ptr::null_mut(),
                &mut result
            );

            if result != fc::FcResultMatch || (*fontset).nfont == 0 {
                return None;
            }

            let mut coverage = fc::FcCharSetNew();
            let mut charsets = Vec::with_capacity((*fontset).nfont as usize);

            for i in 0..(*fontset).nfont {
                let font = *(*fontset).fonts.offset(i as isize);

                let mut charset: *mut fc::FcCharSet = ptr::null_mut();
                let result = fc::FcPatternGetCharSet(
                    font,
                    FC_CHARSET.as_ptr() as *mut c_char,
                    0,
                    &mut charset
                );

                if result == fc::FcResultMatch {
                    coverage = fc::FcCharSetUnion(coverage, charset);
                }

                charsets.push(charset);
            }

            Some(FontSet {
                font_descriptor,
                pattern,
                fontset,
                charsets,
                coverage,
            })
        }
    }

    pub fn font_descriptor(&self) -> &FontDescriptor {
        &self.font_descriptor
    }

    fn default_font(&self) -> *mut fc::FcPattern {
        unsafe {
            *(*self.fontset).fonts.offset(0)
        }
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

pub struct FontRenderer {
    loaded_fonts: HashMap<fc::FcChar32, Option<*mut xft::XftFont>>,
}

impl FontRenderer {
    pub fn new() -> Self {
        Self {
            loaded_fonts: HashMap::new(),
        }
    }

    pub fn render_line_text(
        &mut self,
        display: *mut xlib::Display,
        draw: *mut xft::XftDraw,
        color: *mut xft::XftColor,
        fontset: &FontSet,
        x: i32,
        y: i32,
        text: &str
    ) {
        let mut x_position = 0;

        for chunk in ChunkIter::new(text, &fontset) {
            let font = match self.load_font(display, fontset.pattern, chunk.font) {
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
                    &mut extents
                );

                let ascent = (*font).ascent;
                let height = cmp::max(fontset.font_descriptor.pixel_size as i32, ascent);
                let y_adjustment = (height - ascent) / 2;

                xft::XftDrawStringUtf8(
                    draw,
                    color,
                    font,
                    x + x_position + (extents.x as i32),
                    y + y_adjustment + (extents.height as i32),
                    chunk.text.as_ptr(),
                    chunk.text.len() as i32
                );

                x_position += extents.width as i32;
            }
        }
    }

    fn load_font(&mut self, display: *mut xlib::Display, pattern: *mut fc::FcPattern, font: *mut fc::FcPattern) -> Option<*mut xft::XftFont> {
        *self.loaded_fonts
            .entry(unsafe { fc::FcPatternHash(font) })
            .or_insert_with(|| {
                unsafe {
                    let pattern = fc::FcFontRenderPrepare(
                        ptr::null_mut(),
                        pattern,
                        font
                    );

                    let font = xft::XftFontOpenPattern(display, pattern.cast());
                    if font.is_null() {
                        fc::FcPatternDestroy(pattern);
                        return None;
                    }

                    Some(font)
                }
            })
    }
}

#[derive(Debug, Hash)]
pub struct FontDescriptor {
    pub family_name: String,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub pixel_size: u64,
}

impl FontDescriptor {
    pub fn to_pattern(&self) -> *mut fc::FcPattern {
        unsafe {
            let pattern = fc::FcPatternCreate();

            if let Ok(ref family_name) = CString::new(self.family_name.as_str()) {
                fc::FcPatternAddString(
                    pattern,
                    FC_FAMILY.as_ptr() as *mut c_char,
                    family_name.as_ptr() as *const u8
                );
            }

            fc::FcPatternAddInteger(
                pattern,
                FC_WEIGHT.as_ptr() as *mut c_char,
                self.weight as i32
            );

            fc::FcPatternAddInteger(
                pattern,
                FC_SLANT.as_ptr() as *mut c_char,
                self.style as i32
            );

            fc::FcPatternAddDouble(
                pattern,
                FC_PIXEL_SIZE.as_ptr() as *mut c_char,
                self.pixel_size as f64
            );

            pattern
        }
    }

    pub fn key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

#[derive(Clone, Copy, Debug, Hash)]
pub enum FontWeight {
    Thin       = fc::FC_WEIGHT_THIN as isize,
    Extralight = fc::FC_WEIGHT_EXTRALIGHT as isize,
    Light      = fc::FC_WEIGHT_LIGHT as isize,
    Book       = fc::FC_WEIGHT_BOOK as isize,
    Regular    = fc::FC_WEIGHT_REGULAR as isize,
    Medium     = fc::FC_WEIGHT_MEDIUM as isize,
    Demibold   = fc::FC_WEIGHT_DEMIBOLD as isize,
    Bold       = fc::FC_WEIGHT_BOLD as isize,
    Extrabold  = fc::FC_WEIGHT_EXTRABOLD as isize,
    Black      = fc::FC_WEIGHT_BLACK as isize,
    Extrablack = fc::FC_WEIGHT_EXTRABLACK as isize,
}

#[derive(Clone, Copy, Debug, Hash)]
pub enum FontStyle {
  Normal = fc::FC_SLANT_ROMAN as isize,
  Italic = fc::FC_SLANT_ITALIC as isize,
  Oblique = fc::FC_SLANT_OBLIQUE as isize,
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
