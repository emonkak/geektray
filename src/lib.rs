extern crate cairo_sys;
extern crate libdbus_sys;
extern crate nix;
extern crate serde;
extern crate serde_json;
extern crate x11rb;

pub mod app;
pub mod config;

mod command;
mod event_loop;
mod graphics;
mod tray_container;
mod tray_item;
mod tray_manager;
mod ui;
mod utils;

#[allow(dead_code)]
mod dbus;
