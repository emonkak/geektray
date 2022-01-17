extern crate anyhow;
extern crate x11rb;

use anyhow::Context as _;
use x11rb::connection::Connection;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

const SYSTEM_TRAY_REQUEST_DOCK: u32 = 0;
const SYSTEM_TRAY_BEGIN_MESSAGE: u32 = 1;

const XEMBED_MAPPED: u32 = 1 << 0;

fn main() -> anyhow::Result<()> {
    let (connection, screen_num) = RustConnection::connect(None).context("connect to X server")?;
    let screen = &connection.setup().roots[screen_num];

    let tray_selection_atom = connection
        .intern_atom(
            false,
            format!("_NET_SYSTEM_TRAY_S{}", screen_num).as_bytes(),
        )?
        .reply()
        .context("intern _NET_SYSTEM_TRAY_S atom")?
        .atom;
    let atoms = Atoms::new(&connection)?.reply().context("intern atoms")?;

    let window = {
        let window = connection.generate_id().context("generate window id")?;

        let event_mask = xproto::EventMask::BUTTON_PRESS
            | xproto::EventMask::BUTTON_RELEASE
            | xproto::EventMask::ENTER_WINDOW
            | xproto::EventMask::EXPOSURE
            | xproto::EventMask::STRUCTURE_NOTIFY;
        let values = xproto::CreateWindowAux::new()
            .event_mask(event_mask)
            .background_pixel(screen.white_pixel)
            .override_redirect(1);

        connection
            .create_window(
                screen.root_depth,
                window,
                screen.root,
                0,
                0,
                24,
                24,
                0,
                xproto::WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                &values,
            )?
            .check()
            .context("create window")?;

        window
    };

    set_window_title(
        &connection,
        window,
        b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua",
        &atoms,
    )?;

    set_xembed_info(&connection, window, &atoms)?;

    let mut tray_window = {
        let selection_window = connection
            .get_selection_owner(tray_selection_atom)?
            .reply()
            .context("get tray selection owner")?
            .owner;
        if selection_window != 0 {
            request_dock(&connection, window, selection_window, &atoms)?;
            Some(window)
        } else {
            connection
                .map_window(window)?
                .check()
                .context("map window")?;
            None
        }
    };

    {
        let values = xproto::ChangeWindowAttributesAux::new()
            .event_mask(xproto::EventMask::STRUCTURE_NOTIFY);
        connection
            .change_window_attributes(screen.root, &values)?
            .check()
            .context("set event_mask for root window")?
    }

    let mut notification_id = 0;

    loop {
        let event = connection.wait_for_event().context("get event")?;

        match event {
            Event::ButtonRelease(_) => {
                notification_id += 1;
                set_window_title(
                    &connection,
                    window,
                    format!("Tray Icon Test #{:?}\0", notification_id).as_bytes(),
                    &atoms,
                )?;
                if let Some(selection_window) = tray_window {
                    send_message(
                        &connection,
                        window,
                        selection_window,
                        3000,
                        &format!("Test Message #{:?}", notification_id),
                        notification_id,
                        &atoms,
                    )?;
                }
            }
            Event::ClientMessage(event) => {
                if event.type_ == atoms.MANAGER {
                    let data = event.data.as_data32();
                    if data[1] == tray_selection_atom {
                        let selection_window = data[2];
                        tray_window = Some(selection_window);
                        request_dock(&connection, window, selection_window, &atoms)?;
                    }
                }
            }
            _ => {}
        }
    }
}

fn set_window_title<C: Connection>(
    connection: &C,
    window: xproto::Window,
    title: &[u8],
    atoms: &Atoms,
) -> anyhow::Result<()> {
    connection
        .change_property8(
            xproto::PropMode::REPLACE,
            window,
            xproto::AtomEnum::WM_NAME,
            xproto::AtomEnum::STRING,
            title,
        )?
        .check()
        .context("set WM_NAME property")?;

    connection
        .change_property8(
            xproto::PropMode::REPLACE,
            window,
            atoms._NET_WM_NAME,
            atoms.UTF8_STRING,
            title,
        )?
        .check()
        .context("set _NET_WM_NAME property")?;

    Ok(())
}

fn set_xembed_info<C: Connection>(
    connection: &C,
    window: xproto::Window,
    atoms: &Atoms,
) -> anyhow::Result<()> {
    let xembed_info: [u32; 2] = [0, XEMBED_MAPPED];

    connection
        .change_property32(
            xproto::PropMode::REPLACE,
            window,
            atoms._XEMBED_INFO,
            atoms._XEMBED_INFO,
            &xembed_info,
        )?
        .check()
        .context("set _XEMBED_INFO property")?;

    Ok(())
}

fn request_dock<C: Connection>(
    connection: &C,
    window: xproto::Window,
    selection_window: xproto::Window,
    atoms: &Atoms,
) -> anyhow::Result<()> {
    let event = xproto::ClientMessageEvent::new(
        32,
        window,
        atoms._NET_SYSTEM_TRAY_OPCODE,
        [x11rb::CURRENT_TIME, SYSTEM_TRAY_REQUEST_DOCK, window, 0, 0],
    );

    connection
        .send_event(false, selection_window, 0xffffffu32, event)?
        .check()
        .context("send request dock")?;

    connection.flush().context("flush request dock")?;

    Ok(())
}

fn send_message<C: Connection>(
    connection: &C,
    window: xproto::Window,
    selection_window: xproto::Window,
    timeout_millis: u32,
    body: &str,
    id: u32,
    atoms: &Atoms,
) -> anyhow::Result<()> {
    let event = xproto::ClientMessageEvent::new(
        32,
        window,
        atoms._NET_SYSTEM_TRAY_OPCODE,
        [
            x11rb::CURRENT_TIME,
            SYSTEM_TRAY_BEGIN_MESSAGE,
            timeout_millis,
            body.len() as u32,
            id,
        ],
    );

    connection
        .send_event(false, selection_window, 0xffffffu32, event)?
        .check()
        .context("send SYSTEM_TRAY_BEGIN_MESSAGE")?;

    send_message_data(connection, window, selection_window, body, atoms)?;

    connection.flush().context("flush send message")?;

    Ok(())
}

fn send_message_data<C: Connection>(
    connection: &C,
    window: xproto::Window,
    selection_window: xproto::Window,
    body: &str,
    atoms: &Atoms,
) -> anyhow::Result<()> {
    for chunk in body.as_bytes().chunks(20) {
        let mut data = [0u8; 20];
        data[0..chunk.len()].copy_from_slice(chunk);

        let event =
            xproto::ClientMessageEvent::new(8, window, atoms._NET_SYSTEM_TRAY_MESSAGE_DATA, data);

        connection
            .send_event(false, selection_window, 0xffffffu32, event)?
            .check()
            .context("send _NET_SYSTEM_TRAY_MESSAGE_DATA")?;
    }

    Ok(())
}

x11rb::atom_manager! {
    Atoms: AtomsCookie {
        MANAGER,
        UTF8_STRING,
        _NET_SYSTEM_TRAY_MESSAGE_DATA,
        _NET_SYSTEM_TRAY_OPCODE,
        _NET_WM_NAME,
        _XEMBED_INFO,
    }
}
