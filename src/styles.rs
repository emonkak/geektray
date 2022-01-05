use crate::color::Color;
use crate::config::UiConfig;
use crate::font::{FontDescription};

#[derive(Debug)]
pub struct Styles {
    pub window_padding: f64,
    pub icon_size: f64,
    pub item_padding: f64,
    pub item_gap: f64,
    pub item_corner_radius: f64,
    pub show_index: bool,
    pub font_description: FontDescription,
    pub font_size: f64,
    pub window_background: Color,
    pub normal_item_background: Color,
    pub normal_item_foreground: Color,
    pub selected_item_background: Color,
    pub selected_item_foreground: Color,
}

impl Styles {
    pub fn new(config: &UiConfig) -> Self {
        Self {
            icon_size: config.icon_size,
            window_padding: config.window_padding,
            item_padding: config.item_padding,
            item_gap: config.item_gap,
            item_corner_radius: config.item_corner_radius,
            show_index: config.show_index,
            font_description: FontDescription::new(
                config.font.family.clone(),
                config.font.style,
                config.font.weight.into(),
                config.font.stretch,
            ),
            font_size: config.font.size,
            window_background: config.color.window_background,
            normal_item_background: config.color.normal_item_background,
            normal_item_foreground: config.color.normal_item_foreground,
            selected_item_background: config.color.selected_item_background,
            selected_item_foreground: config.color.selected_item_foreground,
        }
    }
}
