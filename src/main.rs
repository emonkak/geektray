extern crate keytray;
extern crate env_logger;

use std::env;

use keytray::app::App;
use keytray::config::Config;

fn main() {
    env_logger::init();

    let args = env::args().collect();
    let config = Config::parse(args);
    let mut app = App::new(config).unwrap();
    app.run().unwrap();
}
