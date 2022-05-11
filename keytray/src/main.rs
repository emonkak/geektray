extern crate keytray;
extern crate simple_logger;

use anyhow::Context as _;
use simple_logger::SimpleLogger;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use keytray::{App, Config};

fn main() -> anyhow::Result<()> {
    let config = match get_config_path() {
        Some(config_path) if config_path.exists() => load_config(config_path)?,
        _ => Config::default(),
    };
    SimpleLogger::new()
        .with_level(config.log_level.into())
        .init()
        .context("init logger")?;
    let mut app = App::new(config)?;
    app.run()?;
    Ok(())
}

fn get_config_path() -> Option<PathBuf> {
    env::var("XDG_CONFIG_HOME")
        .map(|config_dir| Path::new(&config_dir).to_path_buf())
        .or_else(|_| env::var("HOME").map(|home_dir| Path::new(&home_dir).join(".config")))
        .map(|config_dir| config_dir.join("keytray").join("config.toml"))
        .ok()
}

fn load_config(path: impl AsRef<Path>) -> anyhow::Result<Config> {
    let toml_string = fs::read_to_string(path).context("read config file")?;
    let config: Config = toml::from_str(&toml_string).context("parse toml string")?;
    Ok(config)
}
