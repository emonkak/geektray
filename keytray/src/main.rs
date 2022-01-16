extern crate keytray;

use std::env;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use keytray::app::App;
use keytray::config::Config;

fn main() {
    let config = match get_config_path() {
        Some(config_path) if config_path.exists() => load_config(config_path).unwrap(),
        _ => Config::default(),
    };
    let mut app = App::new(config).unwrap();
    app.run().unwrap();
}

fn get_config_path() -> Option<PathBuf> {
    env::var("XDG_CONFIG_HOME")
        .map(|config_dir| Path::new(&config_dir).to_path_buf())
        .or_else(|_| env::var("HOME").map(|home_dir| Path::new(&home_dir).join(".config")))
        .map(|config_dir| config_dir.join("keytray").join("config.json"))
        .ok()
}

fn load_config(path: impl AsRef<Path>) -> Result<Config, Box<dyn Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let config = serde_json::from_reader(reader)?;
    Ok(config)
}
