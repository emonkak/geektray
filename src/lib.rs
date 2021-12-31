extern crate libdbus_sys;
extern crate nix;
extern crate x11;

pub mod app;
pub mod config;

mod atoms;
mod color;
mod error_handler;
mod event_loop;
mod geometrics;
mod paint_context;
mod styles;
mod tray;
mod tray_item;
mod utils;
mod widget;

#[allow(dead_code)]
mod dbus;

#[allow(dead_code)]
mod xembed;

#[allow(dead_code)]
mod text;

#[allow(dead_code)]
mod font;

#[allow(dead_code)]
mod fontconfig;
