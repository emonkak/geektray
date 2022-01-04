use std::collections::hash_map;
use std::collections::HashMap;
use std::mem;
use std::os::raw::*;
use std::ptr;
use std::str::CharIndices;
use x11::xft;
use x11::xlib;
use x11::xrender;

use crate::color::Color;
use crate::font::{FontId, FontSet};
use crate::fontconfig as fc;
use crate::geometrics::{Rect, Size};

#[derive(Debug)]
pub struct TextRenderer {
    font_caches: HashMap<FontId, *mut xft::XftFont>,
}

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            font_caches: HashMap::new(),
        }
    }

    pub fn render_single_line(
        &mut self,
        display: *mut xlib::Display,
        draw: *mut xft::XftDraw,
        color: Color,
        text: Text,
        bounds: Rect,
    ) {
        let origin_x = match text.horizontal_align {
            HorizontalAlign::Left => bounds.x,
            HorizontalAlign::Right => bounds.x + bounds.width,
            HorizontalAlign::Center => {
                (bounds.x + bounds.width / 2.0)
                    - (self.measure_single_line(display, text).width / 2.0)
            }
        };
        let origin_y = match text.vertical_align {
            VerticalAlign::Top => bounds.y,
            VerticalAlign::Middle => bounds.y + bounds.height / 2.0 - text.font_size / 2.0,
            VerticalAlign::Bottom => bounds.y + bounds.height - text.font_size,
        };

        let mut x_offset = 0.0;

        for text_chunk in TextChunkIter::new(text.content, &text.font_set) {
            let font = if let Some(font) = self.open_font(
                display,
                text_chunk.font,
                text.font_size,
                text.font_set.pattern(),
            ) {
                font
            } else {
                continue;
            };

            let extents = unsafe {
                let mut extents = mem::MaybeUninit::<xrender::XGlyphInfo>::uninit();
                xft::XftTextExtentsUtf8(
                    display,
                    font,
                    text_chunk.content.as_ptr(),
                    text_chunk.content.len() as i32,
                    extents.as_mut_ptr(),
                );
                extents.assume_init()
            };

            let ascent = unsafe { (*font).ascent } as f32;
            let y_adjustment = if text.font_size > ascent {
                (text.font_size - ascent) / 2.0
            } else {
                0.0
            };

            unsafe {
                xft::XftDrawStringUtf8(
                    draw,
                    &mut color.as_xft_color(),
                    font,
                    (origin_x + x_offset + extents.x as f32) as i32,
                    (origin_y + y_adjustment + extents.height as f32) as i32,
                    text_chunk.content.as_ptr(),
                    text_chunk.content.len() as i32,
                );
            }

            x_offset += extents.width as f32;
        }
    }

    pub fn measure_single_line(&mut self, display: *mut xlib::Display, text: Text) -> Size {
        let mut measured_size = Size {
            width: 0.0,
            height: 0.0,
        };

        for text_chunk in TextChunkIter::new(text.content, &text.font_set) {
            let font = if let Some(font) = self.open_font(
                display,
                text_chunk.font,
                text.font_size,
                text.font_set.pattern(),
            ) {
                font
            } else {
                continue;
            };

            let extents = unsafe {
                let mut extents = mem::MaybeUninit::<xrender::XGlyphInfo>::uninit();
                xft::XftTextExtentsUtf8(
                    display,
                    font,
                    text_chunk.content.as_ptr(),
                    text_chunk.content.len() as i32,
                    extents.as_mut_ptr(),
                );
                extents.assume_init()
            };

            measured_size.width += extents.width as f32;
            measured_size.height += measured_size.height.max(extents.height as f32);
        }

        measured_size
    }

    pub fn clear_caches(&mut self, display: *mut xlib::Display) {
        for (id, font) in self.font_caches.drain() {
            unsafe {
                xft::XftFontClose(display, font);
                fc::FcPatternDestroy(id.0);
            }
        }
    }

    fn open_font(
        &mut self,
        display: *mut xlib::Display,
        font: *mut fc::FcPattern,
        font_size: f32,
        fontset_pattern: *mut fc::FcPattern,
    ) -> Option<*mut xft::XftFont> {
        unsafe {
            let pattern = fc::FcFontRenderPrepare(ptr::null_mut(), fontset_pattern, font);

            fc::FcPatternDel(pattern, fc::FC_PIXEL_SIZE.as_ptr() as *mut c_char);
            fc::FcPatternAddDouble(
                pattern,
                fc::FC_PIXEL_SIZE.as_ptr() as *mut c_char,
                font_size as f64,
            );

            match self.font_caches.entry(FontId(pattern)) {
                hash_map::Entry::Occupied(entry) => {
                    fc::FcPatternDestroy(pattern);
                    Some(*entry.get())
                }
                hash_map::Entry::Vacant(entry) => {
                    let font = xft::XftFontOpenPattern(display, pattern.cast());
                    if font.is_null() {
                        fc::FcPatternDestroy(pattern);
                        return None;
                    }
                    entry.insert(font);
                    Some(font)
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Text<'a> {
    pub content: &'a str,
    pub font_size: f32,
    pub font_set: &'a FontSet,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum HorizontalAlign {
    Left,
    Center,
    Right,
}

struct TextChunk<'a> {
    content: &'a str,
    font: *mut fc::FcPattern,
}

struct TextChunkIter<'a> {
    fontset: &'a FontSet,
    current_font: Option<*mut fc::FcPattern>,
    current_index: usize,
    char_indices: CharIndices<'a>,
    source: &'a str,
}

impl<'a> TextChunkIter<'a> {
    fn new(source: &'a str, fontset: &'a FontSet) -> Self {
        Self {
            fontset,
            current_font: None,
            current_index: 0,
            char_indices: source.char_indices(),
            source,
        }
    }
}

impl<'a> Iterator for TextChunkIter<'a> {
    type Item = TextChunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((i, c)) = self.char_indices.next() {
            let matched_font = self.fontset.match_font(c);
            if i == 0 {
                self.current_font = matched_font;
            } else if self.current_font != matched_font {
                let result = Some(TextChunk {
                    content: &self.source[self.current_index..i],
                    font: self.current_font.unwrap_or(self.fontset.default_font()),
                });
                self.current_font = matched_font;
                self.current_index = i;
                return result;
            }
        }

        if self.current_index < self.source.len() {
            let result = Some(TextChunk {
                content: &self.source[self.current_index..],
                font: self.current_font.unwrap_or(self.fontset.default_font()),
            });
            self.current_font = None;
            self.current_index = self.source.len();
            return result;
        }

        None
    }
}
