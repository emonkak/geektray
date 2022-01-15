extern crate anyhow;
extern crate keytray;
extern crate serde;

use anyhow::Context as _;
use keytray::dbus;
use serde::Deserialize;
use std::ffi::CStr;

fn main() {
    run().unwrap()
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct OneTwoThree {
    one: i32,
    two: i32,
    three: i32,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Either<L, R> {
    Left(L),
    Right(R),
}

fn run() -> anyhow::Result<()> {
    let connection = dbus::Connection::new_session(unsafe {
        CStr::from_bytes_with_nul_unchecked(b"com.example\0")
    })
    .context("dbus new session")?;

    while connection.read_write(None) {
        while let Some(message) = connection.pop_message() {
            if message.member() == Some("ExampleMethod") {
                let mut reader = dbus::reader::MessageReader::from_message(&message);

                println!("{:?}", reader.consume::<i32>());
                println!("{:?}", reader.consume::<&CStr>());
                println!("{:?}", reader.consume::<f64>());
                println!("{:?}", reader.consume::<Vec<&CStr>>());
                println!("{:?}", reader.consume::<Vec<(&CStr, i32)>>());
                println!("{:?}", reader.consume::<dbus::Variant<i32>>());
            }
        }
    }

    Ok(())
}
