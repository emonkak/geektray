use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}
