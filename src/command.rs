use serde::de;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum Command {
    HideWindow,
    ShowWindow,
    ToggleWindow,
    SelectNextItem,
    SelectPreviousItem,
    ClickLeftButton,
    ClickRightButton,
    ClickMiddleButton,
    ClickX1Button,
    ClickX2Button,
}

impl FromStr for Command {
    type Err = de::value::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use serde::de::IntoDeserializer;
        Self::deserialize(s.into_deserializer())
    }
}
