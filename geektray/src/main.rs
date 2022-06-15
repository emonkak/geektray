extern crate geektray;
extern crate simple_logger;

use anyhow::Context as _;
use clap::Parser;
use simple_logger::SimpleLogger;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use geektray::{App, Config};

#[derive(Parser, Debug)]
#[clap(
    version,
    help_template = "{before-help}{usage-heading}\n    {usage}\n\n{all-args}{after-help}"
)]
struct Args {
    /// a path to the alternative config file
    #[clap(short, long, value_parser)]
    config: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = match args.config.map(PathBuf::from).or_else(get_config_path) {
        Some(path) => {
            if path.exists() {
                load_config(path)?
            } else {
                save_default_config(path)?;
                Config::default()
            }
        }
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

fn save_default_config(path: impl AsRef<Path>) -> anyhow::Result<()> {
    let default_string = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config.yml"));
    fs::write(path, default_string).context("write config file")?;
    Ok(())
}
