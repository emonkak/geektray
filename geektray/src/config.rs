use geektray_shell::event::{Modifiers, MouseButton};
use geektray_shell::graphics::{Color, FontFamily, FontStretch, FontStyle, FontWeight};
use geektray_shell::xkbcommon_sys as xkb;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::str::FromStr as _;

use crate::command::Command;
use crate::hotkey::Hotkey;

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub window: WindowConfig,
    pub ui: UiConfig,
    pub hotkeys: Vec<Hotkey>,
    pub global_hotkeys: Vec<Hotkey>,
    pub log_level: LogLevel,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: LogLevel(log::LevelFilter::Error),
            window: WindowConfig::default(),
            ui: UiConfig::default(),
            hotkeys: vec![
                Hotkey::new(
                    xkb::XKB_KEY_1,
                    Modifiers::NONE,
                    vec![Command::SelectItem { index: 0 }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_2,
                    Modifiers::NONE,
                    vec![Command::SelectItem { index: 1 }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_3,
                    Modifiers::NONE,
                    vec![Command::SelectItem { index: 2 }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_4,
                    Modifiers::NONE,
                    vec![Command::SelectItem { index: 3 }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_5,
                    Modifiers::NONE,
                    vec![Command::SelectItem { index: 4 }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_6,
                    Modifiers::NONE,
                    vec![Command::SelectItem { index: 5 }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_7,
                    Modifiers::NONE,
                    vec![Command::SelectItem { index: 6 }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_8,
                    Modifiers::NONE,
                    vec![Command::SelectItem { index: 7 }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_9,
                    Modifiers::NONE,
                    vec![Command::SelectItem { index: 8 }],
                ),
                Hotkey::new(xkb::XKB_KEY_0, Modifiers::NONE, vec![Command::DeselectItem]),
                Hotkey::new(
                    xkb::XKB_KEY_j,
                    Modifiers::NONE,
                    vec![Command::SelectNextItem],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_Down,
                    Modifiers::NONE,
                    vec![Command::SelectNextItem],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_n,
                    Modifiers::CONTROL,
                    vec![Command::SelectNextItem],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_k,
                    Modifiers::NONE,
                    vec![Command::SelectPreviousItem],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_Up,
                    Modifiers::NONE,
                    vec![Command::SelectPreviousItem],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_p,
                    Modifiers::CONTROL,
                    vec![Command::SelectPreviousItem],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_l,
                    Modifiers::NONE,
                    vec![Command::ClickMouseButton {
                        button: MouseButton::Left,
                    }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_Return,
                    Modifiers::NONE,
                    vec![Command::ClickMouseButton {
                        button: MouseButton::Left,
                    }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_h,
                    Modifiers::NONE,
                    vec![Command::ClickMouseButton {
                        button: MouseButton::Right,
                    }],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_Return,
                    Modifiers::SHIFT,
                    vec![Command::ClickMouseButton {
                        button: MouseButton::Right,
                    }],
                ),
                Hotkey::new(xkb::XKB_KEY_q, Modifiers::NONE, vec![Command::HideWindow]),
                Hotkey::new(
                    xkb::XKB_KEY_Escape,
                    Modifiers::NONE,
                    vec![Command::HideWindow],
                ),
            ],
            global_hotkeys: vec![Hotkey::new(
                xkb::XKB_KEY_grave,
                Modifiers::SUPER,
                vec![Command::ToggleWindow],
            )],
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct WindowConfig {
    pub title: Cow<'static, str>,
    pub instance_name: Cow<'static, str>,
    pub class_name: Cow<'static, str>,
    pub width: f64,
    pub auto_close: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: Cow::Borrowed("GeekTray"),
            instance_name: Cow::Borrowed("GeekTray"),
            class_name: Cow::Borrowed("GeekTray"),
            width: 480.0,
            auto_close: true,
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct UiConfig {
    pub show_number: bool,
    pub icon_size: f64,
    pub text_size: f64,
    pub container_padding: f64,
    pub container_background: Color,
    pub container_foreground: Color,
    pub item_padding: f64,
    pub item_gap: f64,
    pub item_corner_radius: f64,
    pub item_font: FontConfig,
    pub item_background: Color,
    pub item_foreground: Color,
    pub selected_item_font: FontConfig,
    pub selected_item_background: Color,
    pub selected_item_foreground: Color,
}

impl UiConfig {
    pub fn item_height(&self) -> f64 {
        self.icon_size + self.item_padding * 2.0
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_number: true,
            icon_size: 24.0,
            text_size: 12.0,
            container_padding: 8.0,
            container_background: Color::from_rgb(0x21272b),
            container_foreground: Color::from_rgb(0xe8eaeb),
            item_padding: 8.0,
            item_gap: 8.0,
            item_corner_radius: 4.0,
            item_font: FontConfig::default(),
            item_background: Color::from_rgb(0x363f45),
            item_foreground: Color::from_rgb(0xe8eaeb),
            selected_item_font: FontConfig::default(),
            selected_item_background: Color::from_rgb(0x1c95e6),
            selected_item_foreground: Color::from_rgb(0xe8eaeb),
        }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct FontConfig {
    pub family: FontFamily,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub stretch: FontStretch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogLevel(log::LevelFilter);

impl From<LogLevel> for log::LevelFilter {
    fn from(log_level: LogLevel) -> Self {
        log_level.0
    }
}

impl Serialize for LogLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match log::LevelFilter::from_str(&s) {
            Ok(level_filter) => Ok(LogLevel(level_filter)),
            Err(error) => Err(de::Error::custom(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let yaml_string = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config.yml"));
        let config: Config = serde_yaml::from_str(&yaml_string).unwrap();
        pretty_assertions::assert_eq!(config, Config::default(),);
    }
}
