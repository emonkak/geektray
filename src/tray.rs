use std::os::raw::*;
use std::rc::Rc;
use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{Point, Rect, Size};
use crate::render_context::RenderContext;
use crate::styles::Styles;
use crate::tray_item::TrayItem;
use crate::widget::{SideEffect, Widget, WidgetPod};

#[derive(Debug)]
pub struct Tray {
    styles: Rc<Styles>,
    selected_icon_index: Option<usize>,
    tray_items: Vec<WidgetPod<TrayItem>>,
}

impl Tray {
    pub fn new(styles: Rc<Styles>) -> Tray {
        Self {
            styles,
            tray_items: Vec::new(),
            selected_icon_index: None,
        }
    }

    pub fn tray_items(&self) -> &[WidgetPod<TrayItem>] {
        &self.tray_items
    }

    pub fn add_tray_item(&mut self, tray_item: WidgetPod<TrayItem>) {
        self.tray_items.push(tray_item);
    }

    pub fn remove_tray_item(&mut self, icon_window: xlib::Window) -> Option<WidgetPod<TrayItem>> {
        if let Some(index) = self
            .tray_items
            .iter()
            .position(|tray_item| tray_item.widget.icon_window() == icon_window)
        {
            match self.selected_icon_index {
                Some(selected_icon_index) if selected_icon_index > index => {
                    self.selected_icon_index = Some(selected_icon_index - 1);
                }
                Some(selected_icon_index) if selected_icon_index == index => {
                    self.selected_icon_index = None;
                }
                _ => {}
            }
            Some(self.tray_items.remove(index))
        } else {
            None
        }
    }

    pub fn find_tray_item_mut(
        &mut self,
        icon_window: xlib::Window,
    ) -> Option<&mut WidgetPod<TrayItem>> {
        self.tray_items
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
            let tray_item = &self.tray_items[index].widget;
            tray_item.emit_click(display, button, button_mask);
        }
    }

    pub fn select_next(&mut self) {
        if self.tray_items.len() == 0 {
            return;
        }

        let selected_icon_index = match self.selected_icon_index {
            Some(index) if index < self.tray_items.len() - 1 => Some(index + 1),
            Some(index) if index == self.tray_items.len() - 1 => None,
            _ => Some(0),
        };

        self.update_selected_icon_index(selected_icon_index);
    }

    pub fn select_previous(&mut self) {
        if self.tray_items.len() == 0 {
            return;
        }

        let selected_icon_index = match self.selected_icon_index {
            Some(index) if index > 0 => Some(index - 1),
            Some(index) if index == 0 => None,
            _ => Some(self.tray_items.len() - 1),
        };

        self.update_selected_icon_index(selected_icon_index);
    }

    fn update_selected_icon_index(&mut self, new_index: Option<usize>) {
        if let Some(index) = self.selected_icon_index {
            let current_tray_item = &mut self.tray_items[index].widget;
            current_tray_item.set_selected(false);
        }

        if let Some(index) = new_index {
            let tray_item = &mut self.tray_items[index].widget;
            tray_item.set_selected(true);
        }

        self.selected_icon_index = new_index;
    }
}

impl Widget for Tray {
    fn render(&mut self, _bounds: Rect, context: &mut RenderContext) {
        context.clear_viewport(self.styles.normal_background);

        for tray_item in &mut self.tray_items {
            tray_item.render(context);
        }
    }

    fn layout(&mut self, container_size: Size) -> Size {
        let mut total_height = 0.0;
        let mut child_position = Point { x: 0.0, y: 0.0 };

        for tray_item in &mut self.tray_items {
            let child_size = tray_item.layout(container_size);
            tray_item.reposition(child_position);
            child_position.y += child_size.height;
            total_height += child_size.height;
        }

        Size {
            width: container_size.width as f32,
            height: total_height.max(self.styles.item_height()),
        }
    }

    fn on_event(
        &mut self,
        display: *mut xlib::Display,
        window: xlib::Window,
        event: &X11Event,
        _bounds: Rect,
    ) -> SideEffect {
        let mut side_effect = SideEffect::None;

        for tray_item in &mut self.tray_items {
            side_effect = side_effect.compose(tray_item.on_event(display, window, event));
        }

        side_effect
    }
}
