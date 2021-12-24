extern crate keytray;

use std::env;

use keytray::app::App;
use keytray::config::Config;

fn main() {
    let args = env::args().collect();
    let config = Config::parse(args);
    let mut app = App::new(config).unwrap();

    app.run().unwrap();
}
