extern crate keytray_shell;
extern crate nix;
extern crate serde;
extern crate serde_json;
extern crate x11rb;

pub mod app;
pub mod config;

mod command;
mod event_loop;
mod hotkey;
mod tray_container;
mod tray_item;
mod tray_manager;
mod utils;
mod xembed;
