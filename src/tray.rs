use std::mem;
use std::os::raw::*;
use std::rc::Rc;
use x11::xlib;

use crate::app::RenderContext;
use crate::geometrics::{Point, Rect, Size};
use crate::styles::Styles;
use crate::tray_item::TrayItem;
use crate::utils;
use crate::widget::{Widget, WidgetPod};

#[derive(Debug)]
pub struct Tray {
    styles: Rc<Styles>,
    selected_icon_index: Option<usize>,
    items: Vec<WidgetPod<TrayItem>>,
    previous_selection_owner: Option<xlib::Window>,
}

impl Tray {
    pub fn new(styles: Rc<Styles>) -> Tray {
        Self {
            styles,
            items: Vec::new(),
            selected_icon_index: None,
            previous_selection_owner: None,
        }
    }

    pub fn add_item(&mut self, tray_item: WidgetPod<TrayItem>) {
        self.items.push(tray_item);
    }

    pub fn remove_item(&mut self, icon_window: xlib::Window) -> Option<WidgetPod<TrayItem>> {
        if let Some(index) = self
            .items
            .iter()
            .position(|item| item.widget.icon_window() == icon_window)
        {
            match self.selected_icon_index {
                Some(selected_icon_index) if selected_icon_index < index => {
                    self.selected_icon_index = Some(selected_icon_index - 1);
                }
                Some(selected_icon_index) if selected_icon_index == index => {
                    self.selected_icon_index = None;
                }
                _ => {}
            }
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    pub fn find_item_mut(&mut self, icon_window: xlib::Window) -> Option<&mut WidgetPod<TrayItem>> {
        self.items
            .iter_mut()
            .find(|widget_pod| widget_pod.widget.icon_window() == icon_window)
    }

    pub fn click_selected_icon(
        &mut self,
        display: *mut xlib::Display,
        button: c_uint,
        button_mask: c_uint,
    ) {
        if let Some(index) = self.selected_icon_index {
            let tray_item = &self.items[index].widget;
            tray_item.emit_click(display, button, button_mask, 10, 10);
        }
    }

    pub fn select_next(&mut self) {
        if self.items.len() == 0 {
            return;
        }

        let selected_icon_index = match self.selected_icon_index {
            Some(index) if index < self.items.len() - 1 => Some(index + 1),
            Some(index) if index == self.items.len() - 1 => None,
            _ => Some(0),
        };

        self.update_selected_icon_index(selected_icon_index);
    }

    pub fn select_previous(&mut self) {
        if self.items.len() == 0 {
            return;
        }

        let selected_icon_index = match self.selected_icon_index {
            Some(index) if index > 0 => Some(index - 1),
            Some(index) if index == 0 => None,
            _ => Some(self.items.len() - 1),
        };

        self.update_selected_icon_index(selected_icon_index);
    }

    fn update_selected_icon_index(&mut self, new_index: Option<usize>) {
        if let Some(index) = self.selected_icon_index {
            let current_tray_item = &mut self.items[index].widget;
            current_tray_item.set_selected(false);
        }

        if let Some(index) = new_index {
            let tray_item = &mut self.items[index].widget;
            tray_item.set_selected(true);
        }

        self.selected_icon_index = new_index;
    }
}

impl Widget for Tray {
    fn render(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        _bounds: Rect,
        context: &mut RenderContext,
    ) {
        unsafe {
            xlib::XSetWindowBackground(display, window, self.styles.normal_background.pixel());
            xlib::XClearWindow(display, window);

            for item in &mut self.items {
                item.render(display, window, context);
            }
        }
    }

    fn realize(
        &mut self,
        display: *mut xlib::Display,
        _parent_window: xlib::Window,
        bounds: Rect,
    ) -> xlib::Window {
        unsafe {
            let screen = xlib::XDefaultScreenOfDisplay(display);
            let root = xlib::XRootWindowOfScreen(screen);

            let mut attributes: xlib::XSetWindowAttributes =
                mem::MaybeUninit::uninit().assume_init();

            attributes.bit_gravity = xlib::CenterGravity;

            let window = xlib::XCreateWindow(
                display,
                root,
                bounds.x as i32,
                bounds.y as i32,
                bounds.width as u32,
                bounds.height as u32,
                0,
                xlib::CopyFromParent,
                xlib::InputOutput as u32,
                xlib::CopyFromParent as *mut xlib::Visual,
                xlib::CWBitGravity,
                &mut attributes,
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

            xlib::XMapWindow(display, window);

            self.previous_selection_owner = Some(acquire_tray_selection(display, window));

            window
        }
    }

    fn layout(&mut self, container_size: Size) -> Size {
        let mut total_height = 0.0;
        let mut child_position = Point { x: 0.0, y: 0.0 };

        for item in &mut self.items {
            let child_size = item.layout(container_size);
            item.reposition(child_position);
            child_position.y += child_size.height;
            total_height += child_size.height;
        }

        Size {
            width: container_size.width as f32,
            height: total_height.max(self.styles.item_height()),
        }
    }

    fn finalize(&mut self, display: *mut xlib::Display, _window: xlib::Window) {
        if let Some(previous_selection_owner) = self.previous_selection_owner.take() {
            unsafe {
                release_tray_selection(display, previous_selection_owner);
            }
        }

        for item in &mut self.items {
            item.finalize(display);
        }
    }
}

unsafe fn acquire_tray_selection(
    display: *mut xlib::Display,
    tray_window: xlib::Window,
) -> xlib::Window {
    let screen = xlib::XDefaultScreenOfDisplay(display);
    let screen_number = xlib::XScreenNumberOfScreen(screen);
    let root = xlib::XRootWindowOfScreen(screen);
    let manager_atom = utils::new_atom(display, "MANAGER\0");
    let net_system_tray_atom =
        utils::new_atom(display, &format!("_NET_SYSTEM_TRAY_S{}\0", screen_number));

    let previous_selection_owner = xlib::XGetSelectionOwner(display, net_system_tray_atom);
    xlib::XSetSelectionOwner(
        display,
        net_system_tray_atom,
        tray_window,
        xlib::CurrentTime,
    );

    let mut data = xlib::ClientMessageData::new();
    data.set_long(0, xlib::CurrentTime as c_long);
    data.set_long(1, net_system_tray_atom as c_long);
    data.set_long(2, tray_window as c_long);

    utils::send_client_message(display, root, root, manager_atom, data);

    previous_selection_owner
}

unsafe fn release_tray_selection(
    display: *mut xlib::Display,
    previous_selection_owner: xlib::Window,
) {
    let screen = xlib::XDefaultScreenOfDisplay(display);
    let screen_number = xlib::XScreenNumberOfScreen(screen);
    let net_system_tray_atom =
        utils::new_atom(display, &format!("_NET_SYSTEM_TRAY_S{}\0", screen_number));

    xlib::XSetSelectionOwner(
        display,
        net_system_tray_atom,
        previous_selection_owner,
        xlib::CurrentTime,
    );
}
