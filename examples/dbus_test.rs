extern crate anyhow;
extern crate keytray;

use anyhow::Context as _;
use keytray::dbus;
use std::ffi::CStr;

fn main() {
    run().unwrap()
}

fn run() -> anyhow::Result<()> {
    let connection = dbus::Connection::new_session(unsafe {
        CStr::from_bytes_with_nul_unchecked(b"com.example\0")
    })
    .context("dbus new session")?;

    while connection.read_write(None) {
        while let Some(message) = connection.pop_message() {
            let reader = dbus::reader::Reader::from_message(&message);
            println!("{:?}", message);
            println!("{:?}", reader.collect::<Vec<_>>());
        }
    }

    Ok(())
}
