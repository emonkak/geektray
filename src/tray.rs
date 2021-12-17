use std::mem;
use std::os::raw::*;
use std::rc::Rc;
use x11::xlib;

use app::{Atoms, RenderContext, Styles};
use geometrics::{Point, Size};
use tray_item::TrayItem;

pub struct Tray {
    display: *mut xlib::Display,
    window: xlib::Window,
    styles: Rc<Styles>,
    selected_icon_index: Option<usize>,
    position: Point,
    size: Size,
    items: Vec<TrayItem>,
}

impl Tray {
    pub fn new(display: *mut xlib::Display, atoms: &Atoms, styles: Rc<Styles>) -> Tray {
        unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(display);
            let root = xlib::XRootWindowOfScreen(screen);

            let mut attributes: xlib::XSetWindowAttributes =
                mem::MaybeUninit::uninit().assume_init();

            let window = xlib::XCreateWindow(
                display,
                root,
                0,
                0,
                1,
                1,
                0,
                xlib::CopyFromParent,
                xlib::InputOutput as u32,
                xlib::CopyFromParent as *mut xlib::Visual,
                0,
                &mut attributes,
            );

            let mut protocol_atoms = [atoms.WM_DELETE_WINDOW, atoms.WM_TAKE_FOCUS, atoms.WM_PING];

            xlib::XSetWMProtocols(
                display,
                window,
                protocol_atoms.as_mut_ptr(),
                protocol_atoms.len() as i32,
            );

            xlib::XSelectInput(
                display,
                window,
                xlib::KeyPressMask
                    | xlib::KeyReleaseMask
                    | xlib::StructureNotifyMask
                    | xlib::FocusChangeMask
                    | xlib::PropertyChangeMask
                    | xlib::ExposureMask,
            );

            Self {
                display,
                window,
                styles,
                items: Vec::new(),
                position: Point::ZERO,
                size: Size::ZERO,
                selected_icon_index: None,
            }
        }
    }

    pub fn render(&mut self, context: &mut RenderContext) {
        unsafe {
            xlib::XSetWindowBackground(
                self.display,
                self.window,
                self.styles.normal_background.pixel(),
            );
            xlib::XClearWindow(self.display, self.window);
        }

        for item in &mut self.items {
            item.render(context)
        }
    }

    pub fn layout(&mut self) -> Size {
        let mut total_height = 0.0;

        {
            let mut child_position = Point::ZERO;

            for item in &mut self.items {
                let child_size = item.layout(child_position);
                child_position.y += child_size.height;
                total_height += child_size.height;
            }
        }

        let size = Size {
            width: self.styles.window_width,
            height: total_height.max(self.styles.icon_size + self.styles.padding * 2.0),
        };
        let position = unsafe {
            let screen_number = xlib::XDefaultScreen(self.display);
            let display_width = xlib::XDisplayWidth(self.display, screen_number) as f32;
            let display_height = xlib::XDisplayHeight(self.display, screen_number) as f32;
            Point {
                x: display_width / 2.0 - size.width / 2.0,
                y: display_height / 2.0 - size.height / 2.0,
            }
        };

        if self.position != position || self.size != size {
            unsafe {
                let mut size_hints: xlib::XSizeHints = mem::MaybeUninit::zeroed().assume_init();
                size_hints.flags = xlib::PSize | xlib::PMinSize | xlib::PMaxSize;
                size_hints.width = size.width as i32;
                size_hints.height = size.height as i32;

                xlib::XSetWMNormalHints(self.display, self.window, &mut size_hints);
                xlib::XMoveResizeWindow(
                    self.display,
                    self.window,
                    position.x as _,
                    position.y as _,
                    size.width as _,
                    size.height as _,
                );
            }

            self.position = position;
            self.size = size;
        }

        size
    }

    pub fn show_window(&self) {
        unsafe {
            xlib::XMapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }

    pub fn hide_window(&self) {
        unsafe {
            xlib::XUnmapWindow(self.display, self.window);
            xlib::XFlush(self.display);
        }
    }

    pub fn window(&self) -> xlib::Window {
        self.window
    }

    pub fn find_item(&self, icon_window: xlib::Window) -> Option<&TrayItem> {
        self.items
            .iter()
            .find(|icon| icon.icon_window() == icon_window)
    }

    pub fn find_item_mut(&mut self, icon_window: xlib::Window) -> Option<&mut TrayItem> {
        self.items
            .iter_mut()
            .find(|icon| icon.icon_window() == icon_window)
    }

    pub fn add_item(&mut self, tray_item: TrayItem) {
        self.items.push(tray_item);
    }

    pub fn remove_item(&mut self, icon_window: xlib::Window) -> Option<TrayItem> {
        if let Some(index) = self
            .items
            .iter()
            .position(|icon| icon.icon_window() == icon_window)
        {
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    pub fn click_selected_icon(&mut self, button: c_uint, button_mask: c_uint) {
        match self.selected_icon_index {
            Some(index) => {
                let tray_item = &self.items[index];
                tray_item.emit_click(button, button_mask, 10, 10);
            }
            _ => (),
        }
    }

    pub fn select_next_icon(&mut self) {
        if self.items.len() == 0 {
            return;
        }

        let selected_icon_index = match self.selected_icon_index {
            Some(index) if index < self.items.len() - 1 => index + 1,
            _ => 0,
        };

        self.update_selected_icon_index(selected_icon_index);
    }

    pub fn select_previous_icon(&mut self) {
        if self.items.len() == 0 {
            return;
        }

        let selected_icon_index = match self.selected_icon_index {
            Some(index) if index > 0 => index - 1,
            _ => self.items.len() - 1,
        };

        self.update_selected_icon_index(selected_icon_index);
    }

    fn update_selected_icon_index(&mut self, new_index: usize) {
        if let Some(index) = self.selected_icon_index {
            let current_tray_item = &mut self.items[index];
            current_tray_item.set_selected(false);
        }

        let tray_item = &mut self.items[new_index];
        tray_item.set_selected(true);

        self.selected_icon_index = Some(new_index);
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.items.clear();
        unsafe {
            xlib::XDestroyWindow(self.display, self.window);
        }
    }
}
