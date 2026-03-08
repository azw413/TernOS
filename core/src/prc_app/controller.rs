extern crate alloc;

use alloc::vec::Vec;

use crate::prc_app::form_preview::{FormPreview, FormPreviewObject};
use crate::prc_app::menu_preview::{MenuBarPreview, MenuItemPreview};

#[derive(Clone, Debug, Default)]
pub struct PrcUiController {
    focused_control_id: Option<u16>,
}

impl PrcUiController {
    pub fn reset(&mut self) {
        self.focused_control_id = None;
    }

    pub fn focused_control_id(&self) -> Option<u16> {
        self.focused_control_id
    }

    pub fn sync_with_form(&mut self, form: Option<&FormPreview>) -> bool {
        let controls = focusable_controls(form);
        if controls.is_empty() {
            if self.focused_control_id.take().is_some() {
                return true;
            }
            return false;
        }
        if controls
            .iter()
            .any(|id| Some(*id) == self.focused_control_id)
        {
            return false;
        }
        self.focused_control_id = controls.first().copied();
        true
    }

    pub fn move_focus(&mut self, form: Option<&FormPreview>, delta: i32) -> bool {
        let controls = focusable_controls(form);
        if controls.is_empty() {
            return false;
        }
        let current_idx = self
            .focused_control_id
            .and_then(|id| controls.iter().position(|x| *x == id))
            .unwrap_or(0);
        let len = controls.len() as i32;
        let next_idx = (current_idx as i32 + delta).rem_euclid(len) as usize;
        let next_id = controls[next_idx];
        if self.focused_control_id == Some(next_id) {
            return false;
        }
        self.focused_control_id = Some(next_id);
        true
    }
}

fn focusable_controls(form: Option<&FormPreview>) -> Vec<u16> {
    let Some(form) = form else {
        return Vec::new();
    };
    form.objects
        .iter()
        .filter_map(|obj| match obj {
            FormPreviewObject::Button { id, .. } => Some(*id),
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
}
