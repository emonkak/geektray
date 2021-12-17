extern crate nix;
extern crate x11;

pub mod app;
pub mod config;

mod color;
mod error_handler;
mod event_loop;
mod font;
mod geometrics;
mod styles;
mod text_renderer;
mod tray;
mod tray_item;
mod utils;
mod xembed;

#[allow(dead_code)]
mod fontconfig;
