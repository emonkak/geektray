use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use x11::keysym;
use x11::xlib;

use crate::command::Command;
use crate::font::{FontFamily, FontStretch, FontStyle, FontWeight};
use crate::key_mapping::{KeyMapping, Keysym, Modifiers};

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub window_name: Cow<'static, str>,
    pub window_class: Cow<'static, str>,
    pub ui: UiConfig,
    pub font: FontConfig,
    pub color: ColorConfig,
    pub keys: Vec<KeyMapping>,
    pub print_x11_errors: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window_name: Cow::Borrowed("KeyTray"),
            window_class: Cow::Borrowed("KeyTray"),
            ui: UiConfig::default(),
            font: FontConfig::default(),
            color: ColorConfig::default(),
            keys: vec![
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
                    vec![Command::ClickLeftButton],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_Return as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::ClickLeftButton],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_Return as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::ClickLeftButton],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_h as xlib::KeySym),
                    Modifiers::none(),
                    vec![Command::ClickRightButton],
                ),
                KeyMapping::new(
                    Keysym(keysym::XK_Return as xlib::KeySym),
                    Modifiers::shift(),
                    vec![Command::ClickRightButton],
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
    pub window_padding: f32,
    pub window_width: f32,
    pub item_gap: f32,
    pub icon_size: f32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_padding: 8.0,
            window_width: 480.0,
            item_gap: 8.0,
            icon_size: 24.0,
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
