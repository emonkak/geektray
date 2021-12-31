use std::path::Path;

use crate::font::{FontFamily, FontStretch, FontStyle, FontWeight};

pub struct Config {
    pub program_name: String,
    pub window_width: f32,
    pub icon_size: f32,
    pub padding: f32,
    pub font_size: f32,
    pub font_family: FontFamily,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub font_stretch: FontStretch,
    pub normal_background: String,
    pub normal_foreground: String,
    pub selected_background: String,
    pub selected_foreground: String,
    pub is_debugging: bool,
}

impl Config {
    pub fn parse(args: Vec<String>) -> Self {
        let program_name = args
            .first()
            .and_then(|arg| Path::new(arg).file_name())
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or("keytray".to_owned());
        Self {
            program_name,
            window_width: 480.0,
            icon_size: 24.0,
            padding: 8.0,
            font_size: 12.0,
            font_family: FontFamily::SansSerif,
            font_weight: FontWeight::NORMAL,
            font_style: FontStyle::Normal,
            font_stretch: FontStretch::Normal,
            normal_background: "#21272b".to_owned(),
            normal_foreground: "#e8eaeb".to_owned(),
            selected_background: "#1c95e6".to_owned(),
            selected_foreground: "#e8eaeb".to_owned(),
            is_debugging: true,
        }
    }
}
