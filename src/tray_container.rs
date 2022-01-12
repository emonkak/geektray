use std::rc::Rc;
use x11rb::properties;
use x11rb::protocol;
use x11rb::protocol::xproto;
use x11rb::protocol::xproto::ConnectionExt as _;

use crate::config::UiConfig;
use crate::graphics::{FontDescription, PhysicalPoint, PhysicalSize, Point, RenderContext, Size};
use crate::tray_item::TrayItem;
use crate::ui::{Effect, Layout, MouseButton, Widget};

#[derive(Debug)]
pub struct TrayContainer {
    tray_items: Vec<TrayItem>,
    selected_index: Option<usize>,
    config: Rc<UiConfig>,
    font: Rc<FontDescription>,
}

impl TrayContainer {
    pub fn new(config: Rc<UiConfig>, font: Rc<FontDescription>) -> TrayContainer {
        Self {
            tray_items: Vec::new(),
            selected_index: None,
            config,
            font,
        }
    }

    pub fn contains_window(&self, window: xproto::Window) -> bool {
        self.tray_items
            .iter()
            .find(|tray_item| tray_item.window() == window)
            .is_some()
    }

    pub fn add_tray_item(&mut self, window: xproto::Window, title: String) -> Effect {
        let tray_item = TrayItem::new(window, title, self.config.clone(), self.font.clone());
        self.tray_items.push(tray_item);
        Effect::RequestLayout
    }

    pub fn remove_tray_item(&mut self, window: xproto::Window) -> Effect {
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

    pub fn change_title(&mut self, window: xproto::Window, title: String) -> Effect {
        if let Some(tray_item) = self
            .tray_items
            .iter_mut()
            .find(|tray_item| tray_item.window() == window)
        {
            tray_item.change_title(title)
        } else {
            Effect::None
        }
    }

    pub fn select_item(&mut self, new_index: Option<usize>) -> Effect {
        let mut result = Effect::None;

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

    pub fn select_next_item(&mut self) -> Effect {
        if self.tray_items.len() == 0 {
            return Effect::None;
        }

        let selected_index = match self.selected_index {
            Some(index) if index < self.tray_items.len() - 1 => Some(index + 1),
            Some(index) if index == self.tray_items.len() - 1 => None,
            _ => Some(0),
        };

        self.select_item(selected_index)
    }

    pub fn select_previous_item(&mut self) -> Effect {
        if self.tray_items.len() == 0 {
            return Effect::None;
        }

        let selected_index = match self.selected_index {
            Some(index) if index > 0 => Some(index - 1),
            Some(index) if index == 0 => None,
            _ => Some(self.tray_items.len() - 1),
        };

        self.select_item(selected_index)
    }

    pub fn click_selected_item(&mut self, button: MouseButton) -> Effect {
        if let Some(index) = self.selected_index {
            let tray_item = &mut self.tray_items[index];
            tray_item.click_item(button)
        } else {
            Effect::None
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
        context.clear(self.config.color.window_background);

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
        let mut total_height = self.config.container_padding * 2.0;
        let mut child_position = Point {
            x: self.config.container_padding,
            y: self.config.container_padding,
        };
        let mut children = Vec::with_capacity(self.tray_items.len());

        let container_inset = Size {
            width: container_size.width - self.config.container_padding * 2.0,
            height: container_size.height - self.config.container_padding * 2.0,
        };

        for (index, tray_item) in self.tray_items.iter().enumerate() {
            let child_layout = tray_item.layout(container_inset);
            let child_size = child_layout.size;
            children.push((child_position, child_layout));
            child_position.y += child_size.height + self.config.item_gap;
            if index > 0 {
                total_height += child_size.height + self.config.item_gap;
            } else {
                total_height += child_size.height;
            }
        }

        Layout {
            size: Size {
                width: container_size.width as f64,
                height: total_height.max(self.config.item_height()),
            },
            children,
        }
    }

    fn on_change_layout(
        &mut self,
        position: PhysicalPoint,
        old_size: PhysicalSize,
        new_size: PhysicalSize,
    ) -> Effect {
        Effect::Action(Box::new(move |connection, _, window| {
            {
                let mut size_hints = properties::WmSizeHints::new();
                size_hints.min_size = Some((0, new_size.height as i32));
                size_hints.max_size = Some((0, new_size.height as i32));

                size_hints.set_normal_hints(connection, window)?.check()?;
            }

            {
                let values = xproto::ConfigureWindowAux::new()
                    .x(position.x)
                    .y(position.y
                        - (((old_size.height as i32 - new_size.height as i32) / 2) as i32))
                    .height(new_size.height)
                    .width(new_size.width);

                connection.configure_window(window, &values)?.check()?;
            }

            Ok(())
        }))
    }

    fn on_event(&mut self, event: &protocol::Event, _position: Point, layout: &Layout) -> Effect {
        let mut side_effect = Effect::None;

        for (tray_item, (position, layout)) in
            self.tray_items.iter_mut().zip(layout.children.iter())
        {
            side_effect = side_effect + tray_item.on_event(event, *position, layout);
        }

        side_effect
    }
}
