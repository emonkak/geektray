extern crate geektray;

use anyhow::Context as _;
use geektray::{App, Config};
use simple_logger::SimpleLogger;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const HELP: &'static str = "\
USAGE:
  geektray [OPTIONS]

OPTIONS:
  -c, --config <CONFIG>  a path to the alternative config file [Default: $XDG_CONFIG_HOME/geektray/config.yml]
  -h, --help             Print help information
  -V, --version          Print version information
";

#[derive(Debug)]
struct Args {
    config: Option<String>,
}

impl Args {
    fn parse_from_env() -> Result<Self, pico_args::Error> {
        let mut pargs = pico_args::Arguments::from_env();

        if pargs.contains(["-h", "--help"]) {
            print!("{}", HELP);
            std::process::exit(0);
        }

        if pargs.contains(["-V", "--version"]) {
            println!("geektray v{}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }

        Ok(Self {
            config: pargs.opt_value_from_str(["-c", "--config"])?,
        })
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse_from_env().context("parse args")?;

    let config = match args.config.map(PathBuf::from).or_else(get_config_dir) {
        Some(config_dir) => {
            let config_path = config_dir.join("config.toml");
            if config_path.exists() {
                load_config(config_path)?
            } else {
                if !config_dir.exists() {
                    fs::create_dir_all(config_dir).context("create config dir")?;
                }
                save_default_config(config_path)?;
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

fn get_config_dir() -> Option<PathBuf> {
    env::var("XDG_CONFIG_HOME")
        .map(|config_dir| Path::new(&config_dir).to_path_buf())
        .or_else(|_| env::var("HOME").map(|home_dir| Path::new(&home_dir).join(".config")))
        .map(|config_dir| config_dir.join("geektray"))
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
