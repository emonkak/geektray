use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt;

#[inline]
pub fn emit_click_event<Connection: self::Connection>(
    connection: &Connection,
    screen_num: usize,
    window: xproto::Window,
    button: xproto::ButtonIndex,
    button_mask: xproto::ButtonMask,
    x: i16,
    y: i16,
) -> Result<(), ReplyError> {
    let screen = &connection.setup().roots[screen_num];
    let previous_pointer = connection.query_pointer(screen.root)?.reply()?;

    let absolute_position = connection
        .translate_coordinates(window, screen.root, x, y)?
        .reply()?;

    connection.warp_pointer(
        x11rb::NONE,                    // src_window
        screen.root,                    // dst_window
        0,                              // src_x
        0,                              // src_y
        0,                              // src_width
        0,                              // src_heihgt
        absolute_position.dst_x as i16, // dst_x
        absolute_position.dst_y as i16, // dst_y
    )?;

    send_button_event(
        connection,
        screen_num,
        window,
        button,
        button_mask,
        true,
        x,
        y,
        absolute_position.dst_x,
        absolute_position.dst_y,
    )?;

    send_button_event(
        connection,
        screen_num,
        window,
        button,
        button_mask,
        false,
        x,
        y,
        absolute_position.dst_x,
        absolute_position.dst_y,
    )?;

    connection.warp_pointer(
        x11rb::NONE,                    // src_window
        screen.root,                    // dst_window
        0,                              // src_x
        0,                              // src_y
        0,                              // src_width
        0,                              // src_heihgt
        previous_pointer.root_x as i16, // dst_x
        previous_pointer.root_y as i16, // dst_y
    )?;

    connection.flush()?;

    Ok(())
}

#[inline]
fn send_button_event<Connection: self::Connection>(
    connection: &Connection,
    screen_num: usize,
    window: xproto::Window,
    button: xproto::ButtonIndex,
    button_mask: xproto::ButtonMask,
    is_pressed: bool,
    x: i16,
    y: i16,
    root_x: i16,
    root_y: i16,
) -> Result<(), ReplyError> {
    let screen = &connection.setup().roots[screen_num];

    let event = xproto::ButtonPressEvent {
        response_type: if is_pressed {
            xproto::BUTTON_PRESS_EVENT
        } else {
            xproto::BUTTON_RELEASE_EVENT
        },
        detail: button.into(),
        sequence: 0,
        time: x11rb::CURRENT_TIME,
        root: screen.root,
        event: window,
        child: x11rb::NONE,
        event_x: x,
        event_y: y,
        root_x,
        root_y,
        state: button_mask.into(),
        same_screen: true,
    };

    connection
        .send_event(true, window, xproto::EventMask::NO_EVENT, event)?
        .check()?;

    Ok(())
}
