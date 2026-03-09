extern crate alloc;

use alloc::vec::Vec;

use crate::prc_app::form_preview::{FormPreview, FormPreviewObject};
use crate::prc_app::menu_preview::{MenuBarPreview, MenuItemPreview};
use crate::prc_app::ui_component::{FocusChain, UiComponent, UiNavEvent};
use crate::ui::Rect;

#[derive(Clone, Debug, Default)]
pub struct PrcUiController {
    focus_chain: FocusChain,
}

impl PrcUiController {
    pub fn reset(&mut self) {
        self.focus_chain.clear_selection();
    }

    pub fn focused_control_id(&self) -> Option<u16> {
        self.focus_chain.selected_id()
    }

    pub fn sync_with_form(&mut self, form: Option<&FormPreview>) -> bool {
        let previous = self.focus_chain.selected_id();
        let controls = focusable_controls(form);
        let refs: Vec<&dyn UiComponent> = controls.iter().map(|c| c as &dyn UiComponent).collect();
        self.focus_chain.rebuild(&refs);
        self.focus_chain.selected_id() != previous
    }

    pub fn move_focus(&mut self, form: Option<&FormPreview>, delta: i32) -> bool {
        let controls = focusable_controls(form);
        if controls.is_empty() {
            return false;
        }
        let refs: Vec<&dyn UiComponent> = controls.iter().map(|c| c as &dyn UiComponent).collect();
        self.focus_chain.rebuild(&refs);
        let before = self.focus_chain.selected_id();
        if delta < 0 {
            self.focus_chain.move_prev();
        } else if delta > 0 {
            self.focus_chain.move_next();
        }
        self.focus_chain.selected_id() != before
    }
}

#[derive(Clone, Copy, Debug)]
struct ControlComponent {
    id: u16,
    rect: Rect,
}

impl UiComponent for ControlComponent {
    fn id(&self) -> u16 {
        self.id
    }

    fn bounds(&self) -> Rect {
        self.rect
    }

    fn set_bounds(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn draw(&self, _painter: &mut dyn crate::prc_app::ui_component::UiPainter, _ctx: crate::prc_app::ui_component::UiDrawCtx) {}
}

fn focusable_controls(form: Option<&FormPreview>) -> Vec<ControlComponent> {
    let Some(form) = form else {
        return Vec::new();
    };
    form.objects
        .iter()
        .filter_map(|obj| match obj {
            FormPreviewObject::Button { id, x, y, w, h, .. } => Some(ControlComponent {
                id: *id,
                rect: Rect::new(*x as i32, *y as i32, *w as i32, *h as i32),
            }),
            _ => None,
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct PrcMenuController {
    menu_bar: Option<MenuBarPreview>,
    active: bool,
    menu_index: usize,
    item_index: Option<usize>,
}

impl PrcMenuController {
    pub fn reset(&mut self) {
        self.menu_bar = None;
        self.active = false;
        self.menu_index = 0;
        self.item_index = None;
    }

    pub fn set_menu_bar(&mut self, menu_bar: Option<MenuBarPreview>) {
        self.menu_bar = menu_bar;
        self.active = false;
        self.menu_index = 0;
        self.item_index = None;
    }

    pub fn menu_count(&self) -> usize {
        self.menu_bar.as_ref().map(|m| m.menus.len()).unwrap_or(0)
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn open(&mut self) -> bool {
        if self.menu_count() == 0 {
            return false;
        }
        self.active = true;
        self.menu_index = self.menu_index.min(self.menu_count().saturating_sub(1));
        self.item_index = None;
        true
    }

    pub fn close(&mut self) {
        self.active = false;
        self.item_index = None;
    }

    pub fn move_menu(&mut self, delta: i32) -> bool {
        let count = self.menu_count();
        if !self.active || count == 0 {
            return false;
        }
        let next = (self.menu_index as i32 + delta).rem_euclid(count as i32) as usize;
        if next == self.menu_index {
            return false;
        }
        self.menu_index = next;
        self.item_index = None;
        true
    }

    pub fn move_item(&mut self, delta: i32) -> bool {
        if !self.active {
            return false;
        }
        let Some(menu) = self.menu_bar.as_ref().and_then(|b| b.menus.get(self.menu_index)) else {
            return false;
        };
        let len = menu.items.len();
        if len == 0 {
            return false;
        }
        match self.item_index {
            Some(cur) => {
                // Palm behavior: Up from first submenu item returns focus to menu-bar title.
                if cur == 0 && delta < 0 {
                    self.item_index = None;
                    return true;
                }
                let next = (cur as i32 + delta).rem_euclid(len as i32) as usize;
                if self.item_index == Some(next) {
                    return false;
                }
                self.item_index = Some(next);
                true
            }
            None => {
                if delta > 0 {
                    self.item_index = Some(0);
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn selected_item(&self) -> Option<&MenuItemPreview> {
        let menu = self.menu_bar.as_ref()?.menus.get(self.menu_index)?;
        let idx = self.item_index?;
        menu.items.get(idx)
    }

    pub fn overlay(&self) -> Option<(&MenuBarPreview, usize, Option<usize>)> {
        if !self.active {
            return None;
        }
        let bar = self.menu_bar.as_ref()?;
        Some((bar, self.menu_index, self.item_index))
    }

    pub fn on_event(&mut self, event: UiNavEvent) -> MenuAction {
        if !self.active {
            return MenuAction::None;
        }
        match event {
            UiNavEvent::Back => {
                self.close();
                MenuAction::Closed
            }
            UiNavEvent::Left => {
                if self.move_menu(-1) {
                    MenuAction::Redraw
                } else {
                    MenuAction::None
                }
            }
            UiNavEvent::Right => {
                if self.move_menu(1) {
                    MenuAction::Redraw
                } else {
                    MenuAction::None
                }
            }
            UiNavEvent::Up => {
                if self.move_item(-1) {
                    MenuAction::Redraw
                } else {
                    MenuAction::None
                }
            }
            UiNavEvent::Down => {
                if self.move_item(1) {
                    MenuAction::Redraw
                } else {
                    MenuAction::None
                }
            }
            UiNavEvent::Confirm => {
                if let Some(id) = self.selected_item().map(|i| i.id) {
                    self.close();
                    MenuAction::Activate(id)
                } else {
                    self.close();
                    MenuAction::Closed
                }
            }
            UiNavEvent::Tick => MenuAction::None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    None,
    Redraw,
    Closed,
    Activate(u16),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpDialogAction {
    None,
    Scroll(i32),
    Dismiss,
}

#[derive(Clone, Copy, Debug)]
pub struct PrcHelpDialogController {
    pub scroll_step_lines: i32,
}

impl Default for PrcHelpDialogController {
    fn default() -> Self {
        Self {
            scroll_step_lines: 8,
        }
    }
}

impl PrcHelpDialogController {
    pub fn on_event(&self, event: UiNavEvent) -> HelpDialogAction {
        match event {
            UiNavEvent::Up => HelpDialogAction::Scroll(-self.scroll_step_lines),
            UiNavEvent::Down => HelpDialogAction::Scroll(self.scroll_step_lines),
            UiNavEvent::Back | UiNavEvent::Confirm => HelpDialogAction::Dismiss,
            UiNavEvent::Left | UiNavEvent::Right | UiNavEvent::Tick => HelpDialogAction::None,
        }
    }
}
