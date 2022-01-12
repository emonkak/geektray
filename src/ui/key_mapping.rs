use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::command::Command;
use crate::ui::{Keysym, Modifiers};

#[derive(Debug)]
pub struct KeyInterpreter {
    command_table: HashMap<(Keysym, Modifiers), Vec<Command>>,
}

impl KeyInterpreter {
    pub fn new(key_mappings: Vec<KeyMapping>) -> Self {
        let mut command_table: HashMap<(Keysym, Modifiers), Vec<Command>> = HashMap::new();
        for key_mapping in key_mappings {
            command_table.insert(
                (key_mapping.keysym, key_mapping.modifiers),
                key_mapping.commands.clone(),
            );
        }
        Self { command_table }
    }

    pub fn eval(&self, key: Keysym, modifiers: Modifiers) -> &[Command] {
        self.command_table
            .get(&(key, modifiers))
            .map(|commands| commands.as_slice())
            .unwrap_or_default()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KeyMapping {
    keysym: Keysym,
    #[serde(default)]
    modifiers: Modifiers,
    commands: Vec<Command>,
}

impl KeyMapping {
    pub fn new(keysym: impl Into<Keysym>, modifiers: Modifiers, commands: Vec<Command>) -> Self {
        Self {
            keysym: keysym.into(),
            modifiers,
            commands,
        }
    }
}
