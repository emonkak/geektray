extern crate cairo_sys;
extern crate libdbus_sys;
extern crate nix;
extern crate serde;
extern crate serde_json;
extern crate x11rb;

pub mod app;
pub mod config;

mod atoms;
mod color;
mod command;
mod event_loop;
mod font;
mod geometrics;
mod hotkey;
mod keyboard;
mod mouse;
mod render_context;
mod text;
mod tray_container;
mod tray_item;
mod tray_manager;
mod utils;
mod widget;
mod window;
mod xembed;
mod xkbcommon_sys;

#[allow(dead_code)]
mod dbus;
