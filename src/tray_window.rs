use anyhow::{self, Context as _};
use std::ops::ControlFlow;
use std::process;
use std::rc::Rc;
use x11rb::connection::Connection;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::wrapper::ConnectionExt;
use x11rb::{properties, protocol};

use crate::atoms::Atoms;
use crate::config::{UIConfig, WindowConfig};
use crate::event::MouseButton;
use crate::font::FontDescription;
use crate::geometrics::{PhysicalPoint, PhysicalSize, Rect, Size};
use crate::render_context::{HAlign, RenderContext, VAlign};

pub struct TrayWindow<C: Connection> {
    connection: Rc<C>,
    screen_num: usize,
    window: xproto::Window,
    size: PhysicalSize,
    is_mapped: bool,
    tray_items: Vec<TrayItem>,
    selected_index: Option<usize>,
    should_layout: bool,
    should_redraw: bool,
}

impl<C: Connection> TrayWindow<C> {
    pub fn new(
        connection: Rc<C>,
        screen_num: usize,
        atoms: &Atoms,
        config: &WindowConfig,
        size: PhysicalSize,
    ) -> anyhow::Result<Self> {
        let window = connection.generate_id().context("generate window id")?;
        let colormap = connection.generate_id().context("generate colormap id")?;
        let screen = &connection.setup().roots[screen_num];

        connection
            .create_colormap(
                xproto::ColormapAlloc::NONE,
                colormap,
                screen.root,
                screen.root_visual,
            )?
            .check()
            .context("create true color colormap")?;

        let event_mask = xproto::EventMask::BUTTON_PRESS
            | xproto::EventMask::BUTTON_RELEASE
            | xproto::EventMask::ENTER_WINDOW
            | xproto::EventMask::EXPOSURE
            | xproto::EventMask::FOCUS_CHANGE
            | xproto::EventMask::KEY_PRESS
            | xproto::EventMask::KEY_RELEASE
            | xproto::EventMask::LEAVE_WINDOW
            | xproto::EventMask::PROPERTY_CHANGE
            | xproto::EventMask::STRUCTURE_NOTIFY;
        let values = xproto::CreateWindowAux::new()
            .event_mask(event_mask)
            .colormap(colormap)
            .border_pixel(screen.black_pixel);

        connection
            .create_window(
                screen.root_depth,
                window,
                screen.root,
                0,
                0,
                size.width as u16,
                size.height as u16,
                0, // border_width
                xproto::WindowClass::INPUT_OUTPUT,
                screen.root_visual,
                &values,
            )?
            .check()
            .context("create tray window")?;

        set_size_hints(&*connection, window, size)?;

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                atoms.WM_PROTOCOLS,
                xproto::AtomEnum::ATOM,
                &[
                    atoms._NET_WM_PING,
                    atoms._NET_WM_SYNC_REQUEST,
                    atoms.WM_DELETE_WINDOW,
                ],
            )?
            .check()
            .context("set WM_PROTOCOLS")?;

        connection
            .change_property8(
                xproto::PropMode::REPLACE,
                window,
                xproto::AtomEnum::WM_NAME,
                xproto::AtomEnum::STRING,
                config.title.as_bytes(),
            )?
            .check()
            .context("set WM_NAME")?;

        connection
            .change_property8(
                xproto::PropMode::REPLACE,
                window,
                atoms._NET_WM_NAME,
                atoms.UTF8_STRING,
                config.title.as_bytes(),
            )?
            .check()
            .context("set _NET_WM_NAME")?;

        {
            let class_string = format!(
                "{}\0{}",
                config.instance_name.as_ref(),
                config.class_name.as_ref()
            );
            connection
                .change_property8(
                    xproto::PropMode::REPLACE,
                    window,
                    xproto::AtomEnum::WM_CLASS,
                    xproto::AtomEnum::STRING,
                    class_string.as_bytes(),
                )?
                .check()
                .context("set WM_CLASS")?;
        }

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                atoms._NET_WM_PID,
                xproto::AtomEnum::CARDINAL,
                &[process::id()],
            )?
            .check()
            .context("set _NET_WM_PID")?;

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                atoms._NET_WM_WINDOW_TYPE,
                xproto::AtomEnum::ATOM,
                &[atoms._NET_WM_WINDOW_TYPE_NORMAL],
            )?
            .check()
            .context("set _NET_WM_WINDOW_TYPE")?;

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                atoms._NET_WM_STATE,
                xproto::AtomEnum::ATOM,
                &[
                    atoms._NET_WM_STATE_ABOVE,
                    atoms._NET_WM_STATE_STAYS_ON_TOP,
                    atoms._NET_WM_STATE_STICKY,
                ],
            )?
            .check()
            .context("set _NET_WM_STATE")?;

        connection
            .change_property32(
                xproto::PropMode::REPLACE,
                window,
                atoms._NET_WM_DESKTOP,
                xproto::AtomEnum::CARDINAL,
                &[0xffffffff],
            )?
            .check()
            .context("set _NET_WM_DESKTOP")?;

        Ok(Self {
            connection,
            screen_num,
            window,
            size,
            is_mapped: false,
            tray_items: Vec::new(),
            selected_index: None,
            should_layout: true,
            should_redraw: true,
        })
    }

    pub fn add_icon(&mut self, icon: xproto::Window, title: String, is_embdded: bool) {
        let tray_item = TrayItem::new(icon, title, is_embdded);
        self.tray_items.push(tray_item);
        self.should_layout = true;
    }

    pub fn change_title(&mut self, icon: xproto::Window, title: String) {
        if let Some(tray_item) = self
            .tray_items
            .iter_mut()
            .find(|tray_item| tray_item.icon == icon)
        {
            tray_item.title = title;
            self.should_redraw = true;
        }
    }

    pub fn change_visibility(&mut self, icon: xproto::Window, is_embdded: bool) {
        if let Some(tray_item) = self
            .tray_items
            .iter_mut()
            .find(|tray_item| tray_item.icon == icon)
        {
            tray_item.is_embdded = is_embdded;
            self.should_redraw = true;
        }
    }

    pub fn clear_icons(&mut self) {
        self.tray_items.clear();
        self.should_layout = true;
    }

    pub fn click_selected_item(&mut self, button: MouseButton) -> anyhow::Result<()> {
        if let Some(selected_item) = self
            .selected_index
            .and_then(|index| self.tray_items.get(index))
        {
            let (button_index, button_mask) = match button {
                MouseButton::Left => (xproto::ButtonIndex::M1, xproto::ButtonMask::M1),
                MouseButton::Right => (xproto::ButtonIndex::M3, xproto::ButtonMask::M3),
                MouseButton::Middle => (xproto::ButtonIndex::M2, xproto::ButtonMask::M2),
                MouseButton::X1 => (xproto::ButtonIndex::M4, xproto::ButtonMask::M4),
                MouseButton::X2 => (xproto::ButtonIndex::M5, xproto::ButtonMask::M5),
            };
            click_window(
                &*self.connection,
                self.screen_num,
                selected_item.icon,
                button_index,
                button_mask,
            )?;
        }
        Ok(())
    }

    pub fn deselect_item(&mut self) {
        self.selected_index = None;
        self.should_redraw = true;
    }

    pub fn draw(
        &mut self,
        layout_changed: bool,
        ui_config: &UIConfig,
        context: &RenderContext,
    ) -> anyhow::Result<()> {
        log::debug!("draw tray window");

        let size = context.size().unsnap();
        let font_instances = FontInstances::new(ui_config);

        context.draw_rect(
            Rect {
                x: 0.0,
                y: 0.0,
                width: size.width,
                height: size.height,
            },
            ui_config.window_background,
        );

        if self.tray_items.len() > 0 {
            for (index, tray_item) in self.tray_items.iter_mut().enumerate() {
                let is_selected = self.selected_index.map_or(false, |i| i == index);

                tray_item.draw(index, is_selected, &font_instances, ui_config, context);
            }
        } else {
            context.draw_text(
                "No tray items found",
                &font_instances.item_font,
                ui_config.text_size,
                HAlign::Center,
                VAlign::Middle,
                Rect {
                    x: ui_config.window_padding,
                    y: 0.0,
                    width: size.width - (ui_config.window_padding * 2.0),
                    height: size.height,
                },
                ui_config.window_foreground,
            );
        }

        context.flush()?;

        for tray_item in &self.tray_items {
            if !tray_item.is_embdded {
                continue;
            }

            if layout_changed {
                let values = xproto::ConfigureWindowAux::new()
                    .x((tray_item.bounds.x + ui_config.item_padding) as i32)
                    .y((tray_item.bounds.y + ui_config.item_padding) as i32)
                    .width(ui_config.icon_size as u32)
                    .height(ui_config.icon_size as u32);
                self.connection
                    .configure_window(tray_item.icon, &values)?
                    .check()
                    .context("move and resize tray icon")?;
            }

            if tray_item.is_mapped {
                self.connection
                    .clear_area(
                        true,
                        tray_item.icon,
                        0,
                        0,
                        ui_config.icon_size as u16,
                        ui_config.icon_size as u16,
                    )?
                    .check()
                    .context("request redraw tray icon")?;
            } else {
                self.connection
                    .map_window(tray_item.icon)?
                    .check()
                    .context("map tray icon")?;
            }
        }

        self.connection
            .flush()
            .context("flush after draw tray window")?;

        self.should_redraw = false;

        Ok(())
    }

    pub fn handle_x11_event(
        &mut self,
        event: &protocol::Event,
        control_flow: &mut ControlFlow<()>,
    ) -> anyhow::Result<()> {
        use protocol::Event::*;

        match event {
            Expose(event) => {
                if event.window == self.window && event.count == 0 {
                    self.should_redraw = true;
                }
            }
            ButtonPress(event) if event.event == self.window => {
                let cursor = PhysicalPoint {
                    x: event.event_x as _,
                    y: event.event_y as _,
                };
                for tray_item in &mut self.tray_items {
                    if tray_item.bounds.snap().contains_pos(cursor) {
                        tray_item.is_pressed = true;
                    }
                }
            }
            ButtonRelease(event) if event.event == self.window => {
                let cursor = PhysicalPoint {
                    x: event.event_x as _,
                    y: event.event_y as _,
                };
                for tray_item in &mut self.tray_items {
                    if !tray_item.is_pressed {
                        continue;
                    }
                    if tray_item.bounds.snap().contains_pos(cursor) {
                        let button = u8::from(event.detail).into();
                        let button_mask = u16::from(event.state).into();
                        click_window(
                            &*self.connection,
                            self.screen_num,
                            tray_item.icon,
                            button,
                            button_mask,
                        )?;
                    }
                    tray_item.is_pressed = false;
                }
            }
            ConfigureNotify(event)
                if event.window == event.event && event.window == self.window =>
            {
                let new_size = PhysicalSize {
                    width: event.width as u32,
                    height: event.height as u32,
                };
                if self.size != new_size {
                    self.size = new_size;
                    self.should_layout = true;
                }
            }
            DestroyNotify(event) if event.window == event.event && event.window == self.window => {
                *control_flow = ControlFlow::Break(());
            }
            LeaveNotify(event) if event.event == self.window => {
                for tray_item in &mut self.tray_items {
                    tray_item.is_pressed = false;
                }
            }
            MapNotify(event) if event.window == event.event && event.window == self.window => {
                self.is_mapped = true;
            }
            MapNotify(event) if event.window == event.event => {
                for tray_item in &mut self.tray_items {
                    if tray_item.icon == event.window {
                        tray_item.is_mapped = true;
                    }
                }
            }
            UnmapNotify(event) if event.window == event.event && event.window == self.window => {
                self.is_mapped = false;
            }
            UnmapNotify(event) if event.window == event.event => {
                for tray_item in &mut self.tray_items {
                    if tray_item.icon == event.window {
                        tray_item.is_mapped = false;
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub fn hide(&mut self) -> anyhow::Result<()> {
        self.selected_index = None;
        self.connection
            .unmap_window(self.window)?
            .check()
            .context("unmap tray window")?;
        self.connection
            .flush()
            .context("flush after unmap tray window")?;
        Ok(())
    }

    pub fn is_mapped(&self) -> bool {
        self.is_mapped
    }

    pub fn layout(&mut self, ui_config: &UIConfig) -> anyhow::Result<PhysicalSize> {
        log::debug!("layout tray window");

        let window_size = self.size.unsnap();
        let item_height =
            ui_config.icon_size.max(ui_config.text_size) + ui_config.item_padding * 2.0;
        let mut v_offset = ui_config.window_padding;
        let mut total_height = ui_config.window_padding * 2.0;

        if self.tray_items.len() > 0 {
            for (i, tray_item) in self.tray_items.iter_mut().enumerate() {
                let bounds = Rect {
                    x: ui_config.window_padding,
                    y: v_offset,
                    width: window_size.width - ui_config.item_padding * 2.0,
                    height: item_height,
                };

                v_offset += bounds.height + ui_config.item_gap;
                total_height += bounds.height;

                if i > 0 {
                    total_height += ui_config.item_gap;
                }

                tray_item.bounds = bounds;
            }
        } else {
            total_height += item_height;
        }

        let size = Size {
            width: window_size.width,
            height: total_height,
        }
        .snap();

        set_size_hints(&*self.connection, self.window, size)?;
        resize_window(&*self.connection, self.screen_num, self.window, size)?;

        self.should_layout = false;

        Ok(size)
    }

    pub fn remove_icon(&mut self, icon: xproto::Window) {
        if let Some(i) = self
            .tray_items
            .iter()
            .position(|tray_item| tray_item.icon == icon)
        {
            self.tray_items.remove(i);
            self.should_layout = true;
        }
    }

    pub fn request_redraw(&mut self) {
        self.should_redraw = true
    }

    pub fn select_item(&mut self, index: usize) {
        self.selected_index = Some(index);
        self.should_redraw = true;
    }

    pub fn select_next_item(&mut self) {
        self.selected_index = match self.selected_index {
            Some(index) if index + 1 < self.tray_items.len() => Some(index + 1),
            Some(_) => None,
            _ => {
                if self.tray_items.len() > 0 {
                    Some(0)
                } else {
                    None
                }
            }
        };
        self.should_redraw = true;
    }

    pub fn select_previous_item(&mut self) {
        self.selected_index = match self.selected_index {
            Some(index) if index > 0 => Some(index - 1),
            Some(_) => None,
            _ => {
                if self.tray_items.len() > 0 {
                    Some(self.tray_items.len() - 1)
                } else {
                    None
                }
            }
        };
        self.should_redraw = true;
    }

    pub fn should_layout(&self) -> bool {
        self.should_layout
    }

    pub fn should_redraw(&self) -> bool {
        self.should_redraw
    }

    pub fn show(&self) -> anyhow::Result<()> {
        {
            let screen = &self.connection.setup().roots[self.screen_num];
            let values = xproto::ConfigureWindowAux::new()
                .x(((screen.width_in_pixels as f64 - self.size.width as f64) / 2.0) as i32)
                .y(((screen.height_in_pixels as f64 - self.size.height as f64) / 2.0) as i32)
                .stack_mode(xproto::StackMode::ABOVE);
            self.connection
                .configure_window(self.window, &values)?
                .check()
                .context("move tray window in screen center")?;
        }
        self.connection
            .map_window(self.window)?
            .check()
            .context("map tray window")?;
        self.connection
            .flush()
            .context("flush after map tray window")?;
        Ok(())
    }

    pub fn window(&self) -> xproto::Window {
        self.window
    }
}

impl<C: Connection> Drop for TrayWindow<C> {
    fn drop(&mut self) {
        self.connection.destroy_window(self.window).ok();
    }
}

#[derive(Debug)]
struct TrayItem {
    icon: xproto::Window,
    title: String,
    is_embdded: bool,
    is_mapped: bool,
    is_pressed: bool,
    bounds: Rect,
}

impl TrayItem {
    fn new(icon: xproto::Window, title: String, is_embdded: bool) -> Self {
        Self {
            icon,
            title,
            is_embdded,
            is_mapped: false,
            is_pressed: false,
            bounds: Rect::ZERO,
        }
    }

    fn draw(
        &self,
        index: usize,
        is_selected: bool,
        font_instances: &FontInstances,
        ui_config: &UIConfig,
        context: &RenderContext,
    ) {
        let (background, foreground, font) = if is_selected {
            (
                ui_config.selected_item_background,
                ui_config.selected_item_foreground,
                &font_instances.selected_item_font,
            )
        } else {
            (
                ui_config.normal_item_background,
                ui_config.normal_item_foreground,
                &font_instances.item_font,
            )
        };

        if ui_config.item_corner_radius > 0.0 {
            context.draw_rounded_rect(
                self.bounds,
                background,
                Size {
                    width: ui_config.item_corner_radius,
                    height: ui_config.item_corner_radius,
                },
            )
        } else {
            context.draw_rect(self.bounds, background);
        }

        let text_bounds = Rect {
            x: self.bounds.x + (ui_config.icon_size + ui_config.item_padding * 2.0),
            y: self.bounds.y,
            width: self.bounds.width - (ui_config.icon_size + ui_config.item_padding * 3.0),
            height: self.bounds.height,
        };
        let text_content = if ui_config.show_number {
            format!("{}. {}", index + 1, &self.title)
        } else {
            format!("{}", self.title)
        };

        context.draw_text(
            &text_content,
            font,
            ui_config.text_size,
            HAlign::Left,
            VAlign::Middle,
            text_bounds,
            foreground,
        );
    }
}

struct FontInstances {
    item_font: FontDescription,
    selected_item_font: FontDescription,
}

impl FontInstances {
    fn new(ui_config: &UIConfig) -> Self {
        Self {
            item_font: ui_config.normal_item_font.to_font_description(),
            selected_item_font: ui_config.selected_item_font.to_font_description(),
        }
    }
}

fn click_window(
    connection: &impl Connection,
    screen_num: usize,
    window: xproto::Window,
    button: xproto::ButtonIndex,
    button_mask: xproto::ButtonMask,
) -> anyhow::Result<()> {
    let screen = &connection.setup().roots[screen_num];
    let saved_pointer = connection.query_pointer(screen.root)?.reply()?;
    let absolute_position = connection
        .translate_coordinates(window, screen.root, 0, 0)?
        .reply()?;

    connection
        .warp_pointer(
            x11rb::NONE,                    // src_window
            screen.root,                    // dst_window
            0,                              // src_x
            0,                              // src_y
            0,                              // src_width
            0,                              // src_heihgt
            absolute_position.dst_x as i16, // dst_x
            absolute_position.dst_y as i16, // dst_y
        )?
        .check()
        .context("move cursor to icon")?;

    send_button_event(
        connection,
        screen_num,
        window,
        button,
        button_mask,
        xproto::BUTTON_PRESS_EVENT,
        0,
        0,
        absolute_position.dst_x,
        absolute_position.dst_y,
    )?;

    send_button_event(
        connection,
        screen_num,
        window,
        button,
        button_mask,
        xproto::BUTTON_RELEASE_EVENT,
        0,
        0,
        absolute_position.dst_x,
        absolute_position.dst_y,
    )?;

    connection
        .warp_pointer(
            x11rb::NONE,                 // src_window
            screen.root,                 // dst_window
            0,                           // src_x
            0,                           // src_y
            0,                           // src_width
            0,                           // src_heihgt
            saved_pointer.root_x as i16, // dst_x
            saved_pointer.root_y as i16, // dst_y
        )?
        .check()
        .context("restore cursor position")?;

    connection.flush().context("flush after click icon")?;

    Ok(())
}

fn resize_window(
    connection: &impl Connection,
    screen_num: usize,
    window: xproto::Window,
    size: PhysicalSize,
) -> anyhow::Result<()> {
    let screen = &connection.setup().roots[screen_num];
    let values = xproto::ConfigureWindowAux::new()
        .x(((screen.width_in_pixels as u32 - size.width) / 2) as i32)
        .y(((screen.height_in_pixels as u32 - size.height) / 2) as i32)
        .height(size.height)
        .width(size.width)
        .stack_mode(xproto::StackMode::ABOVE);

    connection
        .configure_window(window, &values)?
        .check()
        .context("resize tray window")?;

    Ok(())
}

fn send_button_event(
    connection: &impl Connection,
    screen_num: usize,
    window: xproto::Window,
    button: xproto::ButtonIndex,
    button_mask: xproto::ButtonMask,
    event_type: u8,
    x: i16,
    y: i16,
    root_x: i16,
    root_y: i16,
) -> anyhow::Result<()> {
    let screen = &connection.setup().roots[screen_num];

    let event = xproto::ButtonPressEvent {
        response_type: event_type,
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
        state: u16::from(button_mask).into(),
        same_screen: true,
    };

    connection
        .send_event(true, window, xproto::EventMask::NO_EVENT, event)?
        .check()
        .context("send ButtonPressEvent")?;

    Ok(())
}

fn set_size_hints(
    connection: &impl Connection,
    window: xproto::Window,
    size: PhysicalSize,
) -> anyhow::Result<()> {
    let mut size_hints = properties::WmSizeHints::new();
    size_hints.min_size = Some((size.width as i32, size.height as i32));
    size_hints.max_size = Some((size.width as i32, size.height as i32));
    size_hints
        .set_normal_hints(connection, window)?
        .check()
        .context("set tray window size hints")?;

    Ok(())
}
