use std::mem;
use std::ops::{Add, Div, Rem};
use std::ptr;
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

#[inline]
pub fn get_fixed_property<Connection: self::Connection, T: Sized, const N: usize>(
    connection: &Connection,
    window: xproto::Window,
    property_atom: xproto::Atom,
) -> Result<Option<[T; N]>, ReplyError> {
    let format = match mem::size_of::<T>() {
        4 => 32,
        2 => 16,
        1 => 8,
        _ => unreachable!(),
    };

    let mut reply = connection
        .get_property(
            false,
            window,
            property_atom,
            xproto::AtomEnum::ANY,
            0,
            ceiling_div(mem::size_of::<T>() * N, 4) as u32,
        )?
        .reply()?;

    if reply.format != format as u8
        || reply.bytes_after != 0
        || reply.value_len != N as u32
        || reply.value.len() != mem::size_of::<T>() * N
    {
        return Ok(None);
    }

    unsafe {
        let value_ptr = reply.value.as_ptr() as *const [T; N];
        reply.value.set_len(0); // leak value
        Ok(Some(ptr::read_unaligned(value_ptr)))
    }
}

#[inline]
pub fn get_variable_property<Connection: self::Connection>(
    connection: &Connection,
    window: xproto::Window,
    property_atom: xproto::Atom,
    property_type: xproto::AtomEnum,
    property_buffer: u64,
) -> Result<Option<Vec<u8>>, ReplyError> {
    let reply = connection
        .get_property(
            false,
            window,
            property_atom,
            property_type,
            0,
            ceiling_div(property_buffer, 4) as u32,
        )?
        .reply()?;

    if reply.type_ != u32::from(property_type)
        || reply.format != 8
        || reply.value_len == 0
        || reply.value.len() == 0
    {
        return Ok(None);
    }

    let mut data = reply.value;
    data.reserve(reply.bytes_after as usize);

    if reply.bytes_after > 0 {
        let reply = connection
            .get_property(
                false,
                window,
                property_atom,
                property_type,
                ceiling_div(reply.value_len, 4) as u32,
                ceiling_div(reply.bytes_after, 4) as u32,
            )?
            .reply()?;

        if reply.type_ != u32::from(property_type)
            || reply.format != 8
            || reply.value_len == 0
            || reply.value.len() == 0
        {
            return Ok(None);
        }

        data.extend(reply.value);
    }

    Ok(Some(data))
}

#[inline]
fn ceiling_div<T>(n: T, divisor: T) -> T
where
    T: Copy + Add<Output = T> + Div<Output = T> + Rem<Output = T>,
{
    (n + n % divisor) / divisor
}
