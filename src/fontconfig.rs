use std::os::raw::*;

#[repr(C)]
pub struct FcCharSet {
    _zero: [u8; 0],
}

#[repr(C)]
pub struct FcConfig {
    _zero: [u8; 0],
}

#[repr(C)]
pub struct FcPattern {
    _zero: [u8; 0],
}

#[repr(C)]
#[derive(Debug)]
pub struct FcFontSet {
    pub nfont: c_int,
    pub sfont: c_int,
    pub fonts: *mut *mut FcPattern,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FcMatchKind {
    Pattern = 0,
    Font = 1,
    Scan = 2,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FcResult {
    Match = 0,
    NoMatch = 1,
    TypeMismatch = 2,
    NoId = 3,
    OutOfMemory = 4,
}

pub type FcChar8 = c_uchar;
pub type FcChar16 = c_ushort;
pub type FcChar32 = c_uint;
pub type FcBool = c_int;

pub const FC_CHARSET: &'static str = "charset\0";
pub const FC_FAMILY: &'static str = "family\0";
pub const FC_FILE: &'static str = "file\0";
pub const FC_INDEX: &'static str = "index\0";
pub const FC_SLANT: &'static str = "slant\0";
pub const FC_WEIGHT: &'static str = "weight\0";
pub const FC_WIDTH: &'static str = "width\0";
pub const FC_PIXEL_SIZE: &'static str = "pixelsize\0";

pub const FC_SLANT_ROMAN: c_int = 0;
pub const FC_SLANT_ITALIC: c_int = 100;
pub const FC_SLANT_OBLIQUE: c_int = 110;

pub const FC_WIDTH_ULTRACONDENSED: c_int = 50;
pub const FC_WIDTH_EXTRACONDENSED: c_int = 63;
pub const FC_WIDTH_CONDENSED: c_int = 75;
pub const FC_WIDTH_SEMICONDENSED: c_int = 87;
pub const FC_WIDTH_NORMAL: c_int = 100;
pub const FC_WIDTH_SEMIEXPANDED: c_int = 113;
pub const FC_WIDTH_EXPANDED: c_int = 125;
pub const FC_WIDTH_EXTRAEXPANDED: c_int = 150;
pub const FC_WIDTH_ULTRAEXPANDED: c_int = 200;

#[link(name = "fontconfig")]
extern "C" {
    // FcPattern
    pub fn FcPatternCreate() -> *mut FcPattern;
    pub fn FcPatternDuplicate(p: *mut FcPattern) -> *mut FcPattern;
    pub fn FcPatternDestroy(p: *mut FcPattern);
    pub fn FcPatternEqual(pa: *const FcPattern, pb: *const FcPattern) -> FcBool;
    pub fn FcPatternHash(p: *const FcPattern) -> FcChar32;
    pub fn FcPatternAddDouble(p: *mut FcPattern, object: *const c_char, d: c_double) -> FcBool;
    pub fn FcPatternAddInteger(p: *mut FcPattern, object: *const c_char, i: c_int) -> FcBool;
    pub fn FcPatternAddString(
        p: *mut FcPattern,
        object: *const c_char,
        s: *const FcChar8,
    ) -> FcBool;
    pub fn FcPatternGetInteger(
        p: *mut FcPattern,
        object: *const c_char,
        n: c_int,
        i: *mut c_int,
    ) -> FcResult;
    pub fn FcPatternGetDouble(
        p: *mut FcPattern,
        object: *const c_char,
        n: c_int,
        i: *mut c_double,
    ) -> FcResult;
    pub fn FcPatternGetString(
        p: *mut FcPattern,
        object: *const c_char,
        n: c_int,
        s: *mut *mut FcChar8,
    ) -> FcResult;
    pub fn FcPatternGetCharSet(
        p: *mut FcPattern,
        object: *const c_char,
        n: c_int,
        c: *mut *mut FcCharSet,
    ) -> FcResult;
    pub fn FcPatternDel(p: *mut FcPattern, object: *const c_char);
    pub fn FcDefaultSubstitute(pattern: *mut FcPattern);

    // FcConfig
    pub fn FcConfigSubstitute(
        config: *mut FcConfig,
        p: *mut FcPattern,
        kind: FcMatchKind,
    ) -> FcBool;

    // FcFontSet
    pub fn FcFontSetDestroy(s: *mut FcFontSet);
    pub fn FcFontRenderPrepare(
        config: *mut FcConfig,
        pat: *mut FcPattern,
        font: *mut FcPattern,
    ) -> *mut FcPattern;
    pub fn FcFontSort(
        config: *mut FcConfig,
        p: *mut FcPattern,
        trim: FcBool,
        csp: *mut *mut FcCharSet,
        result: *mut FcResult,
    ) -> *mut FcFontSet;

    // FcCharSet
    pub fn FcCharSetDestroy(fcs: *mut FcCharSet);
    pub fn FcCharSetUnion(a: *const FcCharSet, b: *const FcCharSet) -> *mut FcCharSet;
    pub fn FcCharSetHasChar(fcs: *const FcCharSet, ucs4: FcChar32) -> FcBool;
    pub fn FcCharSetNew() -> *mut FcCharSet;

    // FcWeight
    pub fn FcWeightFromOpenTypeDouble(ot_weight: c_double) -> c_double;
}
