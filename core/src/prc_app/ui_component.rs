extern crate alloc;

use alloc::vec::Vec;

use crate::ui::Rect;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiNavEvent {
    Left,
    Right,
    Up,
    Down,
    Confirm,
    Back,
    Tick,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiAction {
    None,
    Redraw,
    Activate(u16),
    Dismiss,
    ScrollBy(i16),
    SelectById(u16),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiDrawCtx {
    pub selected: bool,
    pub pressed: bool,
    pub enabled: bool,
}

impl Default for UiDrawCtx {
    fn default() -> Self {
        Self {
            selected: false,
            pressed: false,
            enabled: true,
        }
    }
}

pub trait UiPainter {
    fn stroke_rect(&mut self, rect: Rect, thick: i32);
    fn fill_rect(&mut self, rect: Rect);
    fn draw_text(&mut self, x: i32, y: i32, text: &str, font_id: u8, invert: bool);
}

pub trait UiComponent {
    fn id(&self) -> u16;
    fn bounds(&self) -> Rect;
    fn set_bounds(&mut self, rect: Rect);

    fn is_focusable(&self) -> bool {
        false
    }

    fn draw(&self, painter: &mut dyn UiPainter, ctx: UiDrawCtx);

    fn on_event(&mut self, _event: UiNavEvent) -> UiAction {
        UiAction::None
    }
}

#[derive(Default, Clone, Debug)]
pub struct FocusChain {
    order: Vec<u16>,
    selected: Option<usize>,
}

impl FocusChain {
    pub fn from_components(components: &[&dyn UiComponent]) -> Self {
        let mut out = Self::default();
        out.rebuild(components);
        out
    }

    pub fn rebuild(&mut self, components: &[&dyn UiComponent]) {
        self.order.clear();
        for c in components {
            if c.is_focusable() {
                self.order.push(c.id());
            }
        }
        if self.order.is_empty() {
            self.selected = None;
        } else if let Some(idx) = self.selected {
            if idx >= self.order.len() {
                self.selected = Some(0);
            }
        } else {
            self.selected = Some(0);
        }
    }

    pub fn selected_id(&self) -> Option<u16> {
        self.selected.and_then(|idx| self.order.get(idx).copied())
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    pub fn select_id(&mut self, id: u16) -> bool {
        if let Some(idx) = self.order.iter().position(|v| *v == id) {
            self.selected = Some(idx);
            return true;
        }
        false
    }

    pub fn move_next(&mut self) -> Option<u16> {
        if self.order.is_empty() {
            self.selected = None;
            return None;
        }
        let next = match self.selected {
            Some(idx) => (idx + 1) % self.order.len(),
            None => 0,
        };
        self.selected = Some(next);
        self.selected_id()
    }

    pub fn move_prev(&mut self) -> Option<u16> {
        if self.order.is_empty() {
            self.selected = None;
            return None;
        }
        let prev = match self.selected {
            Some(0) | None => self.order.len() - 1,
            Some(idx) => idx - 1,
        };
        self.selected = Some(prev);
        self.selected_id()
    }
}
