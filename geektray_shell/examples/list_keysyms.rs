extern crate anyhow;
extern crate geektray_shell;
extern crate x11rb;

use anyhow::{anyhow, Context as _};
use x11rb::protocol::xkb::ConnectionExt as _;
use x11rb::xcb_ffi::XCBConnection;

use geektray_shell::xkb;

fn main() -> anyhow::Result<()> {
    let (connection, _screen_num) = XCBConnection::connect(None).context("connect to X server")?;

    let reply = connection
        .xkb_use_extension(1, 0)?
        .reply()
        .context("init xkb extension")?;
    if !reply.supported {
        anyhow!("xkb extension not supported.");
    }

    let context = xkb::Context::new();
    let device_id =
        xkb::DeviceId::core_keyboard(&connection).context("get the core keyboard device ID")?;
    let keymap = xkb::Keymap::from_device(context, &connection, device_id)
        .context("create a keymap from a device")?;
    let state = xkb::State::from_keymap(keymap.clone());

    println!("keycode\tkeysym\tname");
    for keycode in keymap.all_keycodes() {
        let keysym = state.get_keysym(keycode);
        println!(
            "0x{:<04x}\t0x{:<04x}\t{}",
            keycode,
            u32::from(keysym),
            keysym
        );
    }

    Ok(())
}
