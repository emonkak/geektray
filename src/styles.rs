use x11::xlib;

use crate::color::Color;
use crate::config::Config;
use crate::font::FontDescriptor;
use crate::text_renderer::FontSet;

#[derive(Debug)]
pub struct Styles {
    pub icon_size: f32,
    pub padding: f32,
    pub font_size: f32,
    pub font_set: FontSet,
    pub normal_background: Color,
    pub normal_foreground: Color,
    pub hover_background: Color,
    pub hover_foreground: Color,
    pub selected_background: Color,
    pub selected_foreground: Color,
}

impl Styles {
    pub fn new(display: *mut xlib::Display, config: &Config) -> Result<Self, String> {
        Ok(Self {
            icon_size: config.icon_size,
            padding: config.padding,
            font_size: config.font_size,
            font_set: FontSet::new(FontDescriptor {
                family: config.font_family.clone(),
                style: config.font_style,
                weight: config.font_weight,
                stretch: config.font_stretch,
            })
            .ok_or(format!(
                "Failed to initialize `font_set`: {:?}",
                config.font_family
            ))?,
            normal_background: Color::parse(display, &config.normal_background).ok_or(format!(
                "Failed to parse `normal_background`: {:?}",
                config.normal_background
            ))?,
            normal_foreground: Color::parse(display, &config.normal_foreground).ok_or(format!(
                "Failed to parse `normal_foreground`: {:?}",
                config.normal_foreground
            ))?,
            hover_background: Color::parse(display, &config.normal_background).ok_or(format!(
                "Failed to parse `hover_background`: {:?}",
                config.hover_background
            ))?,
            hover_foreground: Color::parse(display, &config.normal_foreground).ok_or(format!(
                "Failed to parse `hover_foreground`: {:?}",
                config.hover_foreground
            ))?,
            selected_background: Color::parse(display, &config.selected_background).ok_or(
                format!(
                    "Failed to parse `selected_background`: {:?}",
                    config.selected_background
                ),
            )?,
            selected_foreground: Color::parse(display, &config.selected_foreground).ok_or(
                format!(
                    "Failed to parse `selected_foreground`: {:?}",
                    config.selected_foreground
                ),
            )?,
        })
    }

    pub fn item_width(&self) -> f32 {
        self.icon_size + self.padding * 2.0
    }

    pub fn item_height(&self) -> f32 {
        self.icon_size + self.padding * 2.0
    }
}
