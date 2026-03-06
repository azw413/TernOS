extern crate alloc;

use alloc::vec::Vec;

use crate::prc_app::form_preview::{FormPreview, FormPreviewObject};

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
