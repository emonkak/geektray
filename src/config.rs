use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use x11::keysym;
use x11::xlib;

use crate::command::{Command, MouseButton};
use crate::font::{FontFamily, FontStretch, FontStyle, FontWeight};
use crate::key_mapping::{KeyMapping, Keysym, Modifiers};

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub ui: UiConfig,
    pub keys: Vec<KeyMapping>,
    pub print_x11_errors: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig::default(),
            keys: vec![
                KeyMapping::new(
                    Keysym(keysym::XK_1 as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectItem(0)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_2 as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectItem(1)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_3 as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectItem(2)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_4 as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectItem(3)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_5 as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectItem(4)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_6 as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectItem(5)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_7 as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectItem(6)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_8 as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectItem(7)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_9 as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectItem(8)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_j as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectNextItem],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_Down as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectNextItem],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_n as xlib::KeySym),
                    Modifiers::control(),
                    vec![Command::SelectNextItem],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_k as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectPreviousItem],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_Down as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::SelectPreviousItem],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_p as xlib::KeySym),
                    Modifiers::control(),
                    vec![Command::SelectPreviousItem],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_l as xlib::KeySym),
                    Modifiers::control(),
                    vec![Command::ClickMouseButton(MouseButton::Left)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_Return as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::ClickMouseButton(MouseButton::Left)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_Return as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::ClickMouseButton(MouseButton::Left)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_h as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::ClickMouseButton(MouseButton::Right)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_Return as xlib::KeySym),
                    Modifiers::shift(),
                    vec![Command::ClickMouseButton(MouseButton::Right)],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_q as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::HideWindow],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_Escape as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::HideWindow],
                ),
            ],
            print_x11_errors: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct UiConfig {
    pub window_name: Cow<'static, str>,
    pub window_class: Cow<'static, str>,
    pub window_padding: f32,
    pub window_width: f32,
    pub item_padding: f32,
    pub item_gap: f32,
    pub icon_size: f32,
    pub font: FontConfig,
    pub color: ColorConfig,
    pub show_index: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_name: Cow::Borrowed("KeyTray"),
            window_class: Cow::Borrowed("KeyTray"),
            window_padding: 8.0,
            window_width: 480.0,
            item_padding: 0.0,
            item_gap: 8.0,
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
    pub size: f32,
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
    pub window_background: String,
    pub normal_item_background: String,
    pub normal_item_foreground: String,
    pub selected_item_background: String,
    pub selected_item_foreground: String,
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            window_background: "#21272b".to_owned(),
            normal_item_background: "#21272b".to_owned(),
            normal_item_foreground: "#e8eaeb".to_owned(),
            selected_item_background: "#1c95e6".to_owned(),
            selected_item_foreground: "#e8eaeb".to_owned(),
        }
    }
}
