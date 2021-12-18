use std::os::raw::*;
use std::rc::Rc;
use x11::xlib;

use crate::app::RenderContext;
use crate::geometrics::{Point, Rect, Size};
use crate::styles::Styles;
use crate::tray_item::TrayItem;
use crate::widget::{Widget, WidgetPod};

#[derive(Debug)]
pub struct Tray {
    styles: Rc<Styles>,
    selected_icon_index: Option<usize>,
    items: Vec<WidgetPod<TrayItem>>,
}

impl Tray {
    pub fn new(styles: Rc<Styles>) -> Tray {
        Self {
            styles,
            items: Vec::new(),
            selected_icon_index: None,
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
    fn mount(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        _bounds: Rect,
    ) {
        for item in &mut self.items {
            item.mount(display, window);
        }
    }

    fn unmount(&mut self, display: *mut xlib::Display, window: xlib::Window) {
        for item in &mut self.items {
            item.unmount(display, window);
        }
    }

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

            xlib::XFlush(display);
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
}
