use keytray_shell::event::{Keysym, Modifiers};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::command::Command;

#[derive(Debug)]
pub struct HotkeyInterpreter {
    command_table: HashMap<(Keysym, Modifiers), Vec<Command>>,
}

impl HotkeyInterpreter {
    pub fn new(hotkeys: impl Iterator<Item = Hotkey>) -> Self {
        let mut command_table: HashMap<(Keysym, Modifiers), Vec<Command>> = HashMap::new();
        for hotkey in hotkeys {
            command_table.insert(
                (hotkey.keysym, hotkey.modifiers.without_locks()),
                hotkey.commands,
            );
        }
        Self { command_table }
    }

    pub fn eval(&self, key: Keysym, modifiers: Modifiers) -> &[Command] {
        self.command_table
            .get(&(key, modifiers.without_locks()))
            .map(|commands| commands.as_slice())
            .unwrap_or_default()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Hotkey {
    keysym: Keysym,
    #[serde(default)]
    modifiers: Modifiers,
    commands: Vec<Command>,
}

impl Hotkey {
    pub fn new(keysym: impl Into<Keysym>, modifiers: Modifiers, commands: Vec<Command>) -> Self {
        Self {
            keysym: keysym.into(),
            modifiers,
            commands,
        }
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn keysym(&self) -> Keysym {
        self.keysym
    }
}
