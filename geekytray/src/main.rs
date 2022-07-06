extern crate geekytray;
extern crate simple_logger;

use anyhow::Context as _;
use simple_logger::SimpleLogger;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use geekytray::{App, Config};

const HELP: &'static str = "\
USAGE:
  geekytray [OPTIONS]

OPTIONS:
  -c, --config <CONFIG>  a path to the alternative config file [Default: $XDG_CONFIG_HOME/geekytray/config.yml]
  -h, --help             Print help information
  -V, --version          Print version information
";

#[derive(Debug)]
struct Args {
    config: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = parse_args().context("parse args")?;

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

fn parse_args() -> Result<Args, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    if pargs.contains(["-V", "--version"]) {
        println!("geekytray v{}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    Ok(Args {
        config: pargs.opt_value_from_str(["-c", "--config"])?,
    })
}

fn get_config_path() -> Option<PathBuf> {
    env::var("XDG_CONFIG_HOME")
        .map(|config_dir| Path::new(&config_dir).to_path_buf())
        .or_else(|_| env::var("HOME").map(|home_dir| Path::new(&home_dir).join(".config")))
        .map(|config_dir| config_dir.join("geekytray").join("config.toml"))
        .ok()
}

fn load_config(path: impl AsRef<Path>) -> anyhow::Result<Config> {
    let toml_string = fs::read_to_string(path).context("read config file")?;
    let config: Config = toml::from_str(&toml_string).context("parse config file")?;
    Ok(config)
}

fn save_default_config(path: impl AsRef<Path>) -> anyhow::Result<()> {
    let default_string = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml"));
    fs::write(path, default_string).context("write config file")?;
    Ok(())
}
