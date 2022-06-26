use geekytray_shell::event::MouseButton;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "params")]
pub enum Command {
    HideWindow,
    ShowWindow,
    ToggleWindow,
    DeselectItem,
    SelectItem { index: usize },
    SelectNextItem,
    SelectPreviousItem,
    ClickMouseButton { button: MouseButton },
}
