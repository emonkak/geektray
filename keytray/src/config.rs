use keytray_shell::event::{Modifiers, MouseButton};
use keytray_shell::graphics::{Color, FontFamily, FontStretch, FontStyle, FontWeight};
use keytray_shell::xkbcommon_sys as xkb;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::str::FromStr as _;

use crate::command::Command;
use crate::hotkey::Hotkey;

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub log_level: LogLevel,
    pub window: WindowConfig,
    pub ui: UiConfig,
    pub hotkeys: Vec<Hotkey>,
    pub global_hotkeys: Vec<Hotkey>,
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
                    vec![Command::SelectItem(0)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_2,
                    Modifiers::NONE,
                    vec![Command::SelectItem(1)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_3,
                    Modifiers::NONE,
                    vec![Command::SelectItem(2)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_4,
                    Modifiers::NONE,
                    vec![Command::SelectItem(3)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_5,
                    Modifiers::NONE,
                    vec![Command::SelectItem(4)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_6,
                    Modifiers::NONE,
                    vec![Command::SelectItem(5)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_7,
                    Modifiers::NONE,
                    vec![Command::SelectItem(6)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_8,
                    Modifiers::NONE,
                    vec![Command::SelectItem(7)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_9,
                    Modifiers::NONE,
                    vec![Command::SelectItem(8)],
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
                    xkb::XKB_KEY_Down,
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
                    Modifiers::CONTROL,
                    vec![Command::ClickMouseButton(MouseButton::Left)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_Return,
                    Modifiers::NONE,
                    vec![Command::ClickMouseButton(MouseButton::Left)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_Return,
                    Modifiers::NONE,
                    vec![Command::ClickMouseButton(MouseButton::Left)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_h,
                    Modifiers::NONE,
                    vec![Command::ClickMouseButton(MouseButton::Right)],
                ),
                Hotkey::new(
                    xkb::XKB_KEY_Return,
                    Modifiers::SHIFT,
                    vec![Command::ClickMouseButton(MouseButton::Right)],
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WindowConfig {
    pub name: Cow<'static, str>,
    pub class: Cow<'static, str>,
    pub width: f64,
    pub override_redirect: bool,
    pub auto_close: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            name: Cow::Borrowed("KeyTray"),
            class: Cow::Borrowed("KeyTray"),
            width: 480.0,
            override_redirect: false,
            auto_close: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct UiConfig {
    pub container_padding: f64,
    pub item_padding: f64,
    pub item_gap: f64,
    pub icon_size: f64,
    pub item_corner_radius: f64,
    pub show_index: bool,
    pub border_size: f64,
    pub border_color: Color,
    pub font_family: FontFamily,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
    pub font_stretch: FontStretch,
    pub font_size: f64,
    pub window_background: Color,
    pub window_foreground: Color,
    pub normal_item_background: Color,
    pub normal_item_foreground: Color,
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
            container_padding: 8.0,
            item_padding: 8.0,
            item_gap: 8.0,
            item_corner_radius: 4.0,
            icon_size: 24.0,
            show_index: true,
            border_size: 2.0,
            border_color: Color::from_rgb(0x1c95e6),
            font_family: FontFamily::default(),
            font_weight: FontWeight::default(),
            font_style: FontStyle::default(),
            font_stretch: FontStretch::default(),
            font_size: 12.0,
            window_background: Color::from_rgb(0x21272b),
            window_foreground: Color::from_rgb(0xe8eaeb),
            normal_item_background: Color::from_rgb(0x363f45),
            normal_item_foreground: Color::from_rgb(0xe8eaeb),
            selected_item_background: Color::from_rgb(0x1c95e6),
            selected_item_foreground: Color::from_rgb(0xe8eaeb),
        }
    }
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
