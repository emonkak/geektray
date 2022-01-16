use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::command::Command;
use crate::graphics::{Color, FontFamily, FontStretch, FontStyle, FontWeight};
use crate::ui::xkbcommon_sys as xkb;
use crate::ui::{KeyMapping, Modifiers, MouseButton};

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub window: WindowConfig,
    pub ui: UiConfig,
    pub keys: Vec<KeyMapping>,
    pub global_keys: Vec<KeyMapping>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window: WindowConfig::default(),
            ui: UiConfig::default(),
            keys: vec![
                KeyMapping::new(
                    xkb::XKB_KEY_1,
                    Modifiers::NONE,
                    vec![Command::SelectItem(0)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_2,
                    Modifiers::NONE,
                    vec![Command::SelectItem(1)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_3,
                    Modifiers::NONE,
                    vec![Command::SelectItem(2)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_4,
                    Modifiers::NONE,
                    vec![Command::SelectItem(3)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_5,
                    Modifiers::NONE,
                    vec![Command::SelectItem(4)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_6,
                    Modifiers::NONE,
                    vec![Command::SelectItem(5)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_7,
                    Modifiers::NONE,
                    vec![Command::SelectItem(6)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_8,
                    Modifiers::NONE,
                    vec![Command::SelectItem(7)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_9,
                    Modifiers::NONE,
                    vec![Command::SelectItem(8)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_j,
                    Modifiers::NONE,
                    vec![Command::SelectNextItem],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_Down,
                    Modifiers::NONE,
                    vec![Command::SelectNextItem],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_n,
                    Modifiers::CONTROL,
                    vec![Command::SelectNextItem],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_k,
                    Modifiers::NONE,
                    vec![Command::SelectPreviousItem],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_Down,
                    Modifiers::NONE,
                    vec![Command::SelectPreviousItem],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_p,
                    Modifiers::CONTROL,
                    vec![Command::SelectPreviousItem],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_l,
                    Modifiers::CONTROL,
                    vec![Command::ClickMouseButton(MouseButton::Left)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_Return,
                    Modifiers::NONE,
                    vec![Command::ClickMouseButton(MouseButton::Left)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_Return,
                    Modifiers::NONE,
                    vec![Command::ClickMouseButton(MouseButton::Left)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_h,
                    Modifiers::NONE,
                    vec![Command::ClickMouseButton(MouseButton::Right)],
                ),
                KeyMapping::new(
                    xkb::XKB_KEY_Return,
                    Modifiers::SHIFT,
                    vec![Command::ClickMouseButton(MouseButton::Right)],
                ),
                KeyMapping::new(xkb::XKB_KEY_q, Modifiers::NONE, vec![Command::HideWindow]),
                KeyMapping::new(
                    xkb::XKB_KEY_Escape,
                    Modifiers::NONE,
                    vec![Command::HideWindow],
                ),
            ],
            global_keys: vec![KeyMapping::new(
                xkb::XKB_KEY_Escape,
                Modifiers::SUPER,
                vec![Command::ShowWindow],
            )],
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct WindowConfig {
    pub name: Cow<'static, str>,
    pub class: Cow<'static, str>,
    pub initial_width: f64,
    pub sticky: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            name: Cow::Borrowed("KeyTray"),
            class: Cow::Borrowed("KeyTray"),
            initial_width: 480.0,
            sticky: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct UiConfig {
    pub window_name: Cow<'static, str>,
    pub window_class: Cow<'static, str>,
    pub window_width: f64,
    pub container_padding: f64,
    pub item_padding: f64,
    pub item_gap: f64,
    pub icon_size: f64,
    pub item_corner_radius: f64,
    pub font: FontConfig,
    pub color: ColorConfig,
    pub show_index: bool,
}

impl UiConfig {
    pub fn item_height(&self) -> f64 {
        self.icon_size + self.item_padding * 2.0
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_name: Cow::Borrowed("KeyTray"),
            window_class: Cow::Borrowed("KeyTray"),
            window_width: 480.0,
            container_padding: 8.0,
            item_padding: 0.0,
            item_gap: 8.0,
            item_corner_radius: 4.0,
            icon_size: 24.0,
            font: FontConfig::default(),
            color: ColorConfig::default(),
            show_index: true,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct FontConfig {
    pub family: FontFamily,
    pub weight: FontWeight,
    pub style: FontStyle,
    pub stretch: FontStretch,
    pub size: f64,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: FontFamily::default(),
            weight: FontWeight::default(),
            style: FontStyle::default(),
            stretch: FontStretch::default(),
            size: 12.0,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ColorConfig {
    pub window_background: Color,
    pub normal_item_background: Color,
    pub normal_item_foreground: Color,
    pub selected_item_background: Color,
    pub selected_item_foreground: Color,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            window_background: Color::from_rgb(0x21272b),
            normal_item_background: Color::from_rgb(0x363f45),
            normal_item_foreground: Color::from_rgb(0xe8eaeb),
            selected_item_background: Color::from_rgb(0x1c95e6),
            selected_item_foreground: Color::from_rgb(0xe8eaeb),
        }
    }
}
