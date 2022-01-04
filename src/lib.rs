extern crate libdbus_sys;
extern crate nix;
extern crate serde;
extern crate serde_json;
extern crate x11;

pub mod app;
pub mod config;

mod atoms;
mod color;
mod command;
mod effect;
mod error_handler;
mod event_loop;
mod font;
mod geometrics;
mod key_mapping;
mod render_context;
mod styles;
mod text;
mod tray_container;
mod tray_item;
mod tray_manager;
mod utils;
mod widget;
mod xembed;

#[allow(dead_code)]
mod dbus;

#[allow(dead_code)]
mod fontconfig;
