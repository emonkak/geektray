use font::FontWeight;
use font::FontStyle;

pub struct Config {
    pub icon_size: u32,
    pub window_width: u32,
    pub padding: u32,
    pub font_family: String,
    pub font_size: u64,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub normal_background: String,
    pub normal_foreground: String,
    pub selected_background: String,
    pub selected_foreground: String,
}

impl Config {
    pub fn parse(_args: Vec<String>) -> Self {
        Self::default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            icon_size: 24,
            window_width: 480,
            padding: 8,
            font_family: "Monospace".to_string(),
            font_size: 12,
            font_weight: FontWeight::Regular,
            font_style: FontStyle::Normal,
            normal_background: "#21272b".to_string(),
            normal_foreground: "#e8eaeb".to_string(),
            selected_background: "#1c95e6".to_string(),
            selected_foreground: "#e8eaeb".to_string(),
        }
    }
}
