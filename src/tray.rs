use std::os::raw::*;
use std::rc::Rc;
use x11::xlib;

use crate::event_loop::X11Event;
use crate::geometrics::{Point, Size};
use crate::render_context::RenderContext;
use crate::styles::Styles;
use crate::tray_item::{TrayItem, TrayItemMessage};
use crate::widget::{Effect, LayoutResult, Widget};

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

    fn add_tray_item(&mut self, window: xlib::Window, title: String) -> Effect {
        let tray_item = TrayItem::new(window, title, self.styles.clone());
        self.tray_items.push(tray_item);
        Effect::RequestLayout
    }

    fn remove_tray_item(&mut self, window: xlib::Window) -> Effect {
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
            Effect::RequestLayout
        } else {
            Effect::None
        }
    }

    fn change_title(&mut self, window: xlib::Window, title: String) -> Effect {
        if let Some(tray_item) = self
            .tray_items
            .iter_mut()
            .find(|tray_item| tray_item.window() == window)
        {
            tray_item.on_message(TrayItemMessage::ChangeTitle { title })
        } else {
            Effect::None
        }
    }

    fn deselect_item(&mut self) -> Effect {
        if self.tray_items.len() == 0 {
            return Effect::None;
        }

        if let Some(selected_index) = self.selected_index {
            let tray_item = &mut self.tray_items[selected_index];
            tray_item.on_message(TrayItemMessage::DeselectItem)
        } else {
            return Effect::None;
        }
    }

    fn select_next(&mut self) -> Effect {
        if self.tray_items.len() == 0 {
            return Effect::None;
        }

        let selected_index = match self.selected_index {
            Some(index) if index < self.tray_items.len() - 1 => Some(index + 1),
            Some(index) if index == self.tray_items.len() - 1 => None,
            _ => Some(0),
        };

        self.update_selected_index(selected_index)
    }

    fn select_previous(&mut self) -> Effect {
        if self.tray_items.len() == 0 {
            return Effect::None;
        }

        let selected_index = match self.selected_index {
            Some(index) if index > 0 => Some(index - 1),
            Some(index) if index == 0 => None,
            _ => Some(self.tray_items.len() - 1),
        };

        self.update_selected_index(selected_index)
    }

    fn update_selected_index(&mut self, new_index: Option<usize>) -> Effect {
        let mut result = Effect::None;

        if let Some(index) = self.selected_index {
            let tray_item = &mut self.tray_items[index];
            result = result + tray_item.on_message(TrayItemMessage::DeselectItem);
        }

        if let Some(index) = new_index {
            let tray_item = &mut self.tray_items[index];
            result = result + tray_item.on_message(TrayItemMessage::SelectItem);
        }

        self.selected_index = new_index;

        result
    }

    fn click_selected_item(&mut self, button: c_uint, button_mask: c_uint) -> Effect {
        if let Some(index) = self.selected_index {
            let tray_item = &mut self.tray_items[index];
            tray_item.on_message(TrayItemMessage::ClickItem {
                button,
                button_mask,
            })
        } else {
            Effect::None
        }
    }
}

impl Widget<TrayMessage> for Tray {
    fn render(&mut self, _position: Point, layout: &LayoutResult, context: &mut RenderContext) {
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

    fn on_event(&mut self, event: &X11Event, _position: Point, layout: &LayoutResult) -> Effect {
        let mut side_effect = Effect::None;

        for (tray_item, (position, layout)) in
            self.tray_items.iter_mut().zip(layout.children.iter())
        {
            side_effect = side_effect + tray_item.on_event(event, *position, layout);
        }

        side_effect
    }

    fn on_message(&mut self, message: TrayMessage) -> Effect {
        match message {
            TrayMessage::AddTrayIcon { window, title } => self.add_tray_item(window, title),
            TrayMessage::RemoveTrayIcon { window } => self.remove_tray_item(window),
            TrayMessage::ChangeTitle { window, title } => self.change_title(window, title),
            TrayMessage::DeselectItem => self.deselect_item(),
            TrayMessage::SelectNextItem => self.select_next(),
            TrayMessage::SelectPreviousItem => self.select_previous(),
            TrayMessage::ClickSelectedItem {
                button,
                button_mask,
            } => self.click_selected_item(button, button_mask),
        }
    }
}

#[derive(Debug)]
pub enum TrayMessage {
    AddTrayIcon { window: xlib::Window, title: String },
    RemoveTrayIcon { window: xlib::Window },
    ChangeTitle { window: xlib::Window, title: String },
    DeselectItem,
    SelectNextItem,
    SelectPreviousItem,
    ClickSelectedItem { button: c_uint, button_mask: c_uint },
}
