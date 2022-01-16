extern crate anyhow;
extern crate cairo_sys;
extern crate x11rb;

use anyhow::Context as _;
use cairo_sys as cairo;
use x11rb::connection::Connection as _;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::x11_utils::Serialize as _;
use x11rb::xcb_ffi::XCBConnection;

fn main() -> anyhow::Result<()> {
    let (connection, screen_num) = XCBConnection::connect(None).context("connect to X server")?;

    let mut width = 320;
    let mut height = 240;
    let window = create_window(&connection, screen_num, width, height)?;

    connection
        .map_window(window)?
        .check()
        .context("map window")?;
    connection.flush().context("flush map window")?;

    loop {
        let event = connection.wait_for_event().context("get event")?;
        match event {
            Event::Expose(event) if event.window == window && event.count == 0 => {
                redraw(&connection, screen_num, window, width, height)?;
            }
            Event::ConfigureNotify(event) => {
                width = event.width;
                height = event.height;
            }
            Event::DestroyNotify(event) if event.window == window => {
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

fn create_window(
    connection: &XCBConnection,
    screen_num: usize,
    width: u16,
    height: u16,
) -> anyhow::Result<xproto::Window> {
    let window = connection.generate_id().context("generate window id")?;
    let screen = &connection.setup().roots[screen_num];
    let values = xproto::CreateWindowAux::new()
        .event_mask(xproto::EventMask::EXPOSURE | xproto::EventMask::STRUCTURE_NOTIFY)
        .background_pixel(screen.white_pixel);

    connection
        .create_window(
            screen.root_depth,
            window,
            screen.root,
            0,
            0,
            width,
            height,
            0, // border_width
            xproto::WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &values,
        )
        .context("create window")?;

    Ok(window)
}

fn redraw(
    connection: &XCBConnection,
    screen_num: usize,
    window: xproto::Window,
    width: u16,
    height: u16,
) -> anyhow::Result<()> {
    let cairo_surface = unsafe {
        let screen = &connection.setup().roots[screen_num];
        let visual = screen
            .allowed_depths
            .iter()
            .filter_map(|depth| {
                depth
                    .visuals
                    .iter()
                    .find(|depth| depth.visual_id == screen.root_visual)
            })
            .next()
            .context("get root visual")?
            .serialize();

        cairo::cairo_xcb_surface_create(
            connection.get_raw_xcb_connection().cast(),
            window,
            visual.as_ptr() as *mut cairo::xcb_visualtype_t,
            width as i32,
            height as i32,
        )
    };
    let cairo = unsafe { cairo::cairo_create(cairo_surface) };

    unsafe {
        cairo::cairo_rectangle(cairo, 0.0, 0.0, width as f64, height as f64);
        cairo::cairo_set_source_rgb(cairo, 0.0, 0.0, 1.0);
        cairo::cairo_fill(cairo);

        cairo::cairo_destroy(cairo);
        cairo::cairo_surface_destroy(cairo_surface);
    }

    connection.flush().context("flush draw")?;

    Ok(())
}
