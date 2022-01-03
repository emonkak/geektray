use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum Command {
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
