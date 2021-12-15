use std::cmp;
use std::slice;

pub struct Layout<T: Layoutable> {
    items: Vec<T>,
    container_width: u32,
    item_height: u32,
}

pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl<T: Layoutable> Layout<T> {
    pub fn new(container_width: u32, item_height: u32) -> Self {
        Layout {
            items: Vec::new(),
            container_width,
            item_height,
        }
    }

    pub fn width(&self) -> u32 {
        self.container_width
    }

    pub fn height(&self) -> u32 {
        cmp::max(self.item_height, self.item_height * self.items.len() as u32)
    }

    pub fn iter(&self) -> slice::Iter<T> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn get_unchecked(&mut self, index: usize) -> &T {
        &self.items[index]
    }

    pub fn get_unchecked_mut(&mut self, index: usize) -> &mut T {
        unsafe { self.items.get_unchecked_mut(index) }
    }

    pub fn next_item_rectange(&self) -> Rectangle {
        let y = if self.items.len() > 0 {
            self.item_height as i32 * self.items.len() as i32
        } else {
            0
        };
        Rectangle {
            x: 0,
            y,
            width: self.container_width,
            height: self.item_height,
        }
    }

    pub fn add(&mut self, item: T) {
        self.items.push(item)
    }

    pub fn remove_unchecked(&mut self, index: usize) -> T {
        self.items.remove(index)
    }

    pub fn clear(&mut self) {
        self.items.clear()
    }

    pub fn update(&mut self) {
        let mut y = 0;

        for item in self.items.iter_mut() {
            item.update_layout(
                0,
                y,
                self.container_width,
                self.item_height
            );

            y += self.item_height as i32;
        }
    }
}

pub trait Layoutable {
    fn update_layout(&mut self, x: i32, y: i32, width: u32, height: u32);
}
