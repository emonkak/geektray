use serde::{Deserialize, Serialize};

use crate::font::{FontFamily, FontStretch, FontStyle, FontWeight};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub ui: UiConfig,
    pub font: FontConfig,
    pub color: ColorConfig,
    pub keys: Vec<KeyConfig>,
    pub print_x11_errors: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig::default(),
            font: FontConfig::default(),
            color: ColorConfig::default(),
            keys: vec![
                KeyConfig {
                    key: String::from("j"),
                    action: KeyAction::SelectNextItem,
                    modifiers: Modifiers::default(),
                },
                KeyConfig {
                    key: String::from("n"),
                    action: KeyAction::SelectNextItem,
                    modifiers: Modifiers {
                        control: true,
                        ..Modifiers::default()
                    },
                },
            ],
            print_x11_errors: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UiConfig {
    pub window_name: String,
    pub window_class: String,
    pub window_padding: f32,
    pub window_width: f32,
    pub item_gap: f32,
    pub icon_size: f32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_name: "KeyTray".to_owned(),
            window_class: "KeyTray".to_owned(),
            window_padding: 8.0,
            window_width: 480.0,
            item_gap: 8.0,
            icon_size: 24.0,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FontConfig {
    pub family: FontFamily,
    pub weight: Either<FontWeight, FontWeightKind>,
    pub style: FontStyle,
    pub stretch: FontStretch,
    pub size: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: FontFamily::default(),
            weight: Either::Left(FontWeight::default()),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum FontWeightKind {
    Thin = 100,
    ExtraLight = 200,
    Light = 300,
    Normal = 400,
    Medium = 500,
    SemiBolda = 600,
    Bold = 700,
    ExtraBold = 800,
    Black = 900,
}

impl Into<FontWeight> for FontWeightKind {
    fn into(self) -> FontWeight {
        FontWeight(self as u16)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KeyConfig {
    key: String,
    action: KeyAction,
    #[serde(default)]
    modifiers: Modifiers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
enum KeyAction {
    HideWindow,
    ShowWindow,
    SelectNextItem,
    SelectPreviousItem,
    ClickLeftButton,
    ClickRightButton,
    ClickMiddleButton,
    ClickX1Button,
    ClickX2Button,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
struct Modifiers {
    #[serde(default = "false_value")]
    control: bool,
    #[serde(default = "false_value")]
    shift: bool,
    #[serde(default = "false_value")]
    alt: bool,
    #[serde(default = "false_value", rename = "super")]
    super_: bool,
}

impl Default for Modifiers {
    fn default() -> Self {
        Self {
            control: false,
            shift: false,
            alt: false,
            super_: false,
        }
    }
}

fn false_value() -> bool {
    false
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    pub fn into<T>(self) -> T
    where
        L: Into<T>,
        R: Into<T>,
    {
        match self {
            Self::Left(value) => value.into(),
            Self::Right(value) => value.into(),
        }
    }
}
