extern crate geektray;
extern crate simple_logger;

use anyhow::Context as _;
use simple_logger::SimpleLogger;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use geektray::{App, Config};

fn main() -> anyhow::Result<()> {
    let config = match get_config_path() {
        Some(path) => {
            if path.exists() {
                load_config(path)?
            } else {
                let config = Config::default();
                save_config(path, &config)?;
                config
            }
        },
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
        .map(|config_dir| config_dir.join("geektray").join("config.yaml"))
        .ok()
}

fn load_config(path: impl AsRef<Path>) -> anyhow::Result<Config> {
    let yaml_string = fs::read_to_string(path).context("read config file")?;
    let config: Config = serde_yaml::from_str(&yaml_string).context("parse config file")?;
    Ok(config)
}

fn save_config(path: impl AsRef<Path>, config: &Config) -> anyhow::Result<()> {
    let toml_string = serde_yaml::to_string(config).context("serialize config")?;
    fs::write(path, toml_string).context("write config file")?;
    Ok(())
}
