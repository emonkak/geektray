use serde::de;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::ui::MouseButton;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "args")]
pub enum Command {
    HideWindow,
    ShowWindow,
    ToggleWindow,
    SelectItem(usize),
    SelectNextItem,
    SelectPreviousItem,
    ClickMouseButton(MouseButton),
}

impl FromStr for Command {
    type Err = de::value::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use serde::de::IntoDeserializer;
        Self::deserialize(s.into_deserializer())
    }
}
