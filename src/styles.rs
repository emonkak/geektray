use x11::xlib;

use crate::color::Color;
use crate::config::UiConfig;
use crate::font::{FontDescriptor, FontSet};

#[derive(Debug)]
pub struct Styles {
    pub window_padding: f32,
    pub icon_size: f32,
    pub item_padding: f32,
    pub item_gap: f32,
    pub show_index: bool,
    pub font_size: f32,
    pub font_set: FontSet,
    pub window_background: Color,
    pub normal_item_background: Color,
    pub normal_item_foreground: Color,
    pub selected_item_background: Color,
    pub selected_item_foreground: Color,
}

impl Styles {
    pub fn new(display: *mut xlib::Display, config: &UiConfig) -> Result<Self, String> {
        Ok(Self {
            icon_size: config.icon_size,
            window_padding: config.window_padding,
            item_padding: config.item_padding,
            item_gap: config.item_gap,
            show_index: config.show_index,
            font_size: config.font.size,
            font_set: FontSet::new(FontDescriptor {
                family: config.font.family.clone(),
                style: config.font.style,
                weight: config.font.weight.into(),
                stretch: config.font.stretch,
            })
            .ok_or(format!(
                "Failed to initialize `font_set`: {:?}",
                config.font.family
            ))?,
            window_background: Color::parse(display, &config.color.window_background).ok_or(
                format!(
                    "Failed to parse `window_background`: {:?}",
                    config.color.window_background
                ),
            )?,
            normal_item_background: Color::parse(display, &config.color.normal_item_background)
                .ok_or(format!(
                    "Failed to parse `normal_item_background`: {:?}",
                    config.color.normal_item_background
                ))?,
            normal_item_foreground: Color::parse(display, &config.color.normal_item_foreground)
                .ok_or(format!(
                    "Failed to parse `normal_item_foreground`: {:?}",
                    config.color.normal_item_foreground
                ))?,
            selected_item_background: Color::parse(display, &config.color.selected_item_background)
                .ok_or(format!(
                    "Failed to parse `selected_item_background`: {:?}",
                    config.color.selected_item_background
                ))?,
            selected_item_foreground: Color::parse(display, &config.color.selected_item_foreground)
                .ok_or(format!(
                    "Failed to parse `selected_item_foreground`: {:?}",
                    config.color.selected_item_foreground
                ))?,
        })
    }

    pub fn item_height(&self) -> f32 {
        self.icon_size + self.item_padding * 2.0
    }
}
