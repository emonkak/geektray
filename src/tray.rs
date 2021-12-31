use std::os::raw::*;
use std::rc::Rc;
use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{Point, Size};
use crate::paint_context::PaintContext;
use crate::styles::Styles;
use crate::tray_item::TrayItem;
use crate::widget::{LayoutResult, SideEffect, Widget};

#[derive(Debug)]
pub struct Tray {
    tray_items: Vec<TrayItem>,
    selected_index: Option<usize>,
    styles: Rc<Styles>,
}

impl Tray {
    pub fn new(styles: Rc<Styles>) -> Tray {
        Self {
            tray_items: Vec::new(),
            selected_index: None,
            styles,
        }
    }

    pub fn tray_items(&self) -> &[TrayItem] {
        &self.tray_items
    }

    pub fn tray_items_mut(&mut self) -> &mut [TrayItem] {
        &mut self.tray_items
    }

    pub fn add_tray_item(&mut self, tray_item: TrayItem) {
        self.tray_items.push(tray_item);
    }

    pub fn remove_tray_item(&mut self, icon_window: xlib::Window) -> Option<TrayItem> {
        if let Some(index) = self
            .tray_items
            .iter()
            .position(|tray_item| tray_item.icon_window() == icon_window)
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
            Some(self.tray_items.remove(index))
        } else {
            None
        }
    }

    pub fn click_selected_icon(
        &mut self,
        display: *mut xlib::Display,
        button: c_uint,
        button_mask: c_uint,
    ) {
        if let Some(index) = self.selected_index {
            let tray_item = &self.tray_items[index];
            tray_item.emit_click(display, button, button_mask);
        }
    }

    pub fn select_next(&mut self) {
        if self.tray_items.len() == 0 {
            return;
        }

        let selected_index = match self.selected_index {
            Some(index) if index < self.tray_items.len() - 1 => Some(index + 1),
            Some(index) if index == self.tray_items.len() - 1 => None,
            _ => Some(0),
        };

        self.update_selected_index(selected_index);
    }

    pub fn select_previous(&mut self) {
        if self.tray_items.len() == 0 {
            return;
        }

        let selected_index = match self.selected_index {
            Some(index) if index > 0 => Some(index - 1),
            Some(index) if index == 0 => None,
            _ => Some(self.tray_items.len() - 1),
        };

        self.update_selected_index(selected_index);
    }

    fn update_selected_index(&mut self, new_index: Option<usize>) {
        if let Some(index) = self.selected_index {
            let current_tray_item = &mut self.tray_items[index];
            current_tray_item.set_selected(false);
        }

        if let Some(index) = new_index {
            let tray_item = &mut self.tray_items[index];
            tray_item.set_selected(true);
        }

        self.selected_index = new_index;
    }
}

impl Widget for Tray {
    fn render(&mut self, _position: Point, layout: &LayoutResult, context: &mut PaintContext) {
        context.clear_viewport(self.styles.normal_background);

        for (tray_item, (child_position, child_layout)) in
            self.tray_items.iter_mut().zip(layout.children.iter())
        {
            tray_item.render(*child_position, child_layout, context);
        }
    }

    fn layout(&mut self, container_size: Size) -> LayoutResult {
        let mut total_height = 0.0;
        let mut child_position = Point { x: 0.0, y: 0.0 };
        let mut children = Vec::with_capacity(self.tray_items.len());

        for tray_item in &mut self.tray_items {
            let child_layout = tray_item.layout(container_size);
            let child_size = child_layout.size;
            children.push((child_position, child_layout));
            child_position.y += child_size.height;
            total_height += child_size.height;
        }

        LayoutResult {
            size: Size {
                width: container_size.width as f32,
                height: total_height.max(self.styles.item_height()),
            },
            children,
        }
    }

    fn on_event(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        event: &X11Event,
        _position: Point,
        layout: &LayoutResult,
    ) -> SideEffect {
        let mut side_effect = SideEffect::None;

        for (tray_item, (position, layout)) in
            self.tray_items.iter_mut().zip(layout.children.iter())
        {
            side_effect =
                side_effect.compose(tray_item.on_event(display, window, event, *position, layout));
        }

        side_effect
    }
}
