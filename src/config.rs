use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::str::FromStr as _;

use crate::color::Color;
use crate::event::{Keysym, Modifiers, MouseButton};
use crate::font::{FontDescription, FontFamily, FontStretch, FontStyle, FontWeight};
use crate::xkbcommon_sys as xkb;

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub window: WindowConfig,
    pub ui: UIConfig,
    pub key_bindings: Vec<KeyBinding>,
    pub log_level: LogLevel,
}

impl Config {
    pub fn to_toml(&self) -> String {
        use toml::ser::to_string;
        let mut s = String::new();
        s += "# Log level\n";
        s += "# The following are the levels that may be specified:\n";
        s += "# - Off\n";
        s += "# - Error\n";
        s += "# - Warn\n";
        s += "# - Info\n";
        s += "# - Debug\n";
        s += "# - Trace\n";
        s += &format!("log_level = {}\n", to_string(&self.log_level).unwrap());
        s += "\n";
        s += "[window]\n";
        s += "# Window title\n";
        s += &format!("title = {}\n", to_string(&self.window.title).unwrap());
        s += "\n";
        s
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: LogLevel(log::LevelFilter::Error),
            window: WindowConfig::default(),
            ui: UIConfig::default(),
            key_bindings: vec![
                KeyBinding::new(
                    xkb::XKB_KEY_1,
                    Modifiers::NONE,
                    vec![Action::SelectItem { index: 0 }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_2,
                    Modifiers::NONE,
                    vec![Action::SelectItem { index: 1 }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_3,
                    Modifiers::NONE,
                    vec![Action::SelectItem { index: 2 }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_4,
                    Modifiers::NONE,
                    vec![Action::SelectItem { index: 3 }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_5,
                    Modifiers::NONE,
                    vec![Action::SelectItem { index: 4 }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_6,
                    Modifiers::NONE,
                    vec![Action::SelectItem { index: 5 }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_7,
                    Modifiers::NONE,
                    vec![Action::SelectItem { index: 6 }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_8,
                    Modifiers::NONE,
                    vec![Action::SelectItem { index: 7 }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_9,
                    Modifiers::NONE,
                    vec![Action::SelectItem { index: 8 }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_0,
                    Modifiers::NONE,
                    vec![Action::DeselectItem],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_j,
                    Modifiers::NONE,
                    vec![Action::SelectNextItem],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_Down,
                    Modifiers::NONE,
                    vec![Action::SelectNextItem],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_n,
                    Modifiers::CONTROL,
                    vec![Action::SelectNextItem],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_k,
                    Modifiers::NONE,
                    vec![Action::SelectPreviousItem],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_Up,
                    Modifiers::NONE,
                    vec![Action::SelectPreviousItem],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_p,
                    Modifiers::CONTROL,
                    vec![Action::SelectPreviousItem],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_l,
                    Modifiers::NONE,
                    vec![Action::ClickSelectedItem {
                        button: MouseButton::Left,
                    }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_Return,
                    Modifiers::NONE,
                    vec![Action::ClickSelectedItem {
                        button: MouseButton::Left,
                    }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_h,
                    Modifiers::NONE,
                    vec![Action::ClickSelectedItem {
                        button: MouseButton::Right,
                    }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_Return,
                    Modifiers::SHIFT,
                    vec![Action::ClickSelectedItem {
                        button: MouseButton::Right,
                    }],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_q,
                    Modifiers::NONE,
                    vec![Action::HideWindow],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_Escape,
                    Modifiers::NONE,
                    vec![Action::HideWindow],
                    false,
                ),
                KeyBinding::new(
                    xkb::XKB_KEY_grave,
                    Modifiers::SUPER,
                    vec![Action::ToggleWindow],
                    true,
                ),
            ],
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct WindowConfig {
    pub title: Cow<'static, str>,
    pub instance_name: Cow<'static, str>,
    pub class_name: Cow<'static, str>,
    pub default_width: f64,
    pub auto_hide: bool,
    pub icon_theme_color: Color,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: Cow::Borrowed("GeekTray"),
            instance_name: Cow::Borrowed("GeekTray"),
            class_name: Cow::Borrowed("GeekTray"),
            default_width: 480.0,
            auto_hide: true,
            icon_theme_color: Color::WHITE,
        }
    }
}

#[derive(Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct UIConfig {
    pub show_number: bool,
    pub icon_size: f64,
    pub text_size: f64,
    pub window_padding: f64,
    pub window_background: Color,
    pub window_foreground: Color,
    pub item_padding: f64,
    pub item_gap: f64,
    pub item_corner_radius: f64,
    pub normal_item_font: FontDescription,
    pub normal_item_background: Color,
    pub normal_item_foreground: Color,
    pub selected_item_font: FontDescription,
    pub selected_item_background: Color,
    pub selected_item_foreground: Color,
}

impl UIConfig {
    pub fn item_height(&self) -> f64 {
        self.icon_size + self.item_padding * 2.0
    }
}

impl Default for UIConfig {
    fn default() -> Self {
        Self {
            show_number: true,
            icon_size: 24.0,
            text_size: 12.0,
            window_padding: 8.0,
            window_background: Color::from_rgb(0x22262b),
            window_foreground: Color::from_rgb(0xd1dbe7),
            item_padding: 8.0,
            item_gap: 8.0,
            item_corner_radius: 4.0,
            normal_item_font: FontDescription::new(
                &FontFamily::default(),
                FontStyle::Normal,
                FontWeight::NORMAL,
                FontStretch::Normal,
            ),
            normal_item_background: Color::from_rgb(0x334454),
            normal_item_foreground: Color::from_rgb(0xd1dbe7),
            selected_item_font: FontDescription::new(
                &FontFamily::default(),
                FontStyle::Normal,
                FontWeight::BOLD,
                FontStretch::Normal,
            ),
            selected_item_background: Color::from_rgb(0x5686d7),
            selected_item_foreground: Color::from_rgb(0xd1dbe7),
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

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct KeyBinding {
    keysym: Keysym,
    #[serde(default)]
    modifiers: Modifiers,
    actions: Vec<Action>,
    #[serde(default)]
    global: bool,
}

impl KeyBinding {
    pub fn new(
        keysym: impl Into<Keysym>,
        modifiers: Modifiers,
        actions: Vec<Action>,
        global: bool,
    ) -> Self {
        Self {
            keysym: keysym.into(),
            modifiers,
            actions,
            global,
        }
    }

    pub fn keysym(&self) -> Keysym {
        self.keysym
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub fn global(&self) -> bool {
        self.global
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum Action {
    HideWindow,
    ShowWindow,
    ToggleWindow,
    DeselectItem,
    SelectItem {
        #[serde(rename = "index")]
        index: usize,
    },
    SelectNextItem,
    SelectPreviousItem,
    ClickSelectedItem {
        #[serde(rename = "button")]
        button: MouseButton,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let toml_string = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml"));
        let config: Config = toml::from_str(&toml_string).unwrap();
        pretty_assertions::assert_eq!(config, Config::default());
    }
}
