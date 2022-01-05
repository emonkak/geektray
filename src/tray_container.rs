use std::os::raw::*;
use std::rc::Rc;
use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{Point, Size};
use crate::render_context::RenderContext;
use crate::styles::Styles;
use crate::tray_item::TrayItem;
use crate::widget::{Layout, Widget};
use crate::window::WindowEffcet;

#[derive(Debug)]
pub struct TrayContainer {
    tray_items: Vec<TrayItem>,
    selected_index: Option<usize>,
    styles: Rc<Styles>,
}

impl TrayContainer {
    pub fn new(styles: Rc<Styles>) -> TrayContainer {
        Self {
            tray_items: Vec::new(),
            selected_index: None,
            styles,
        }
    }

    pub fn contains_window(&self, window: xlib::Window) -> bool {
        self.tray_items
            .iter()
            .find(|tray_item| tray_item.window() == window)
            .is_some()
    }

    pub fn add_tray_item(&mut self, window: xlib::Window, title: String) -> WindowEffcet {
        let tray_item = TrayItem::new(window, title, self.styles.clone());
        self.tray_items.push(tray_item);
        WindowEffcet::RequestLayout
    }

    pub fn remove_tray_item(&mut self, window: xlib::Window) -> WindowEffcet {
        if let Some(index) = self
            .tray_items
            .iter()
            .position(|tray_item| tray_item.window() == window)
        {
            match self.selected_index {
                Some(selected_index) if selected_index > index => {
                    self.selected_index = Some(selected_index - 1);
                }
                Some(selected_index) if selected_index == index => {
                    self.selected_index = None;
                }
                _ => {}
            }
            self.tray_items.remove(index);
            WindowEffcet::RequestLayout
        } else {
            WindowEffcet::None
        }
    }

    pub fn change_title(&mut self, window: xlib::Window, title: String) -> WindowEffcet {
        if let Some(tray_item) = self
            .tray_items
            .iter_mut()
            .find(|tray_item| tray_item.window() == window)
        {
            tray_item.change_title(title)
        } else {
            WindowEffcet::None
        }
    }

    pub fn select_item(&mut self, new_index: Option<usize>) -> WindowEffcet {
        let mut result = WindowEffcet::None;

        if let Some(index) = self.selected_index {
            let tray_item = &mut self.tray_items[index];
            result = result + tray_item.deselect_item();
        }

        if let Some(index) = new_index {
            if let Some(tray_item) = self.tray_items.get_mut(index) {
                result = result + tray_item.select_item();
                self.selected_index = Some(index);
            } else {
                self.selected_index = None;
            }
        } else {
            self.selected_index = None;
        }

        result
    }

    pub fn select_next_item(&mut self) -> WindowEffcet {
        if self.tray_items.len() == 0 {
            return WindowEffcet::None;
        }

        let selected_index = match self.selected_index {
            Some(index) if index < self.tray_items.len() - 1 => Some(index + 1),
            Some(index) if index == self.tray_items.len() - 1 => None,
            _ => Some(0),
        };

        self.select_item(selected_index)
    }

    pub fn select_previous_item(&mut self) -> WindowEffcet {
        if self.tray_items.len() == 0 {
            return WindowEffcet::None;
        }

        let selected_index = match self.selected_index {
            Some(index) if index > 0 => Some(index - 1),
            Some(index) if index == 0 => None,
            _ => Some(self.tray_items.len() - 1),
        };

        self.select_item(selected_index)
    }

    pub fn click_selected_item(&mut self, button: c_uint, button_mask: c_uint) -> WindowEffcet {
        if let Some(index) = self.selected_index {
            let tray_item = &mut self.tray_items[index];
            tray_item.click_item(button, button_mask)
        } else {
            WindowEffcet::None
        }
    }
}

impl Widget for TrayContainer {
    fn render(
        &self,
        _position: Point,
        layout: &Layout,
        _index: usize,
        context: &mut RenderContext,
    ) {
        context.clear_viewport(self.styles.window_background);

        for (index, (tray_item, (child_position, child_layout))) in self
            .tray_items
            .iter()
            .zip(layout.children.iter())
            .enumerate()
        {
            tray_item.render(*child_position, child_layout, index, context);
        }
    }

    fn layout(&self, container_size: Size) -> Layout {
        let mut total_height = self.styles.window_padding * 2.0;
        let mut child_position = Point {
            x: self.styles.window_padding,
            y: self.styles.window_padding,
        };
        let mut children = Vec::with_capacity(self.tray_items.len());

        let container_inset = Size {
            width: container_size.width - self.styles.window_padding * 2.0,
            height: container_size.height - self.styles.window_padding * 2.0,
        };

        for (index, tray_item) in self.tray_items.iter().enumerate() {
            let child_layout = tray_item.layout(container_inset);
            let child_size = child_layout.size;
            children.push((child_position, child_layout));
            child_position.y += child_size.height + self.styles.item_gap;
            if index > 0 {
                total_height += child_size.height + self.styles.item_gap;
            } else {
                total_height += child_size.height;
            }
        }

        Layout {
            size: Size {
                width: container_size.width as f64,
                height: total_height.max(self.styles.item_height()),
            },
            children,
        }
    }

    fn on_event(&mut self, event: &X11Event, _position: Point, layout: &Layout) -> WindowEffcet {
        let mut side_effect = WindowEffcet::None;

        for (tray_item, (position, layout)) in
            self.tray_items.iter_mut().zip(layout.children.iter())
        {
            side_effect = side_effect + tray_item.on_event(event, *position, layout);
        }

        side_effect
    }
}
