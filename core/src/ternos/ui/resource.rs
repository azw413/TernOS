extern crate alloc;

use alloc::{string::String, vec::Vec};

use crate::ternos::ui::{FormId, ObjectId, Rect, UiForm, UiObject, UiTableModel};

#[derive(Clone, Debug)]
pub enum ObjectResource {
    Label { id: ObjectId, bounds: Rect },
    Button { id: ObjectId, bounds: Rect },
    Field { id: ObjectId, bounds: Rect },
    List { id: ObjectId, bounds: Rect },
    Table {
        id: ObjectId,
        bounds: Rect,
        model: UiTableModel,
    },
    Bitmap { id: ObjectId, bounds: Rect },
    Title { id: ObjectId, bounds: Rect },
    Custom { id: ObjectId, bounds: Rect },
}

impl ObjectResource {
    pub fn into_ui_object(self) -> UiObject {
        match self {
            Self::Label { id, bounds } => UiObject::Label { id, bounds },
            Self::Button { id, bounds } => UiObject::Button { id, bounds },
            Self::Field { id, bounds } => UiObject::Field { id, bounds },
            Self::List { id, bounds } => UiObject::List { id, bounds },
            Self::Table { id, bounds, model } => UiObject::Table { id, bounds, model },
            Self::Bitmap { id, bounds } => UiObject::Bitmap { id, bounds },
            Self::Title { id, bounds } => UiObject::Title { id, bounds },
            Self::Custom { id, bounds } => UiObject::Custom { id, bounds },
        }
    }
}

#[derive(Clone, Debug)]
pub struct FormResource {
    pub form_id: FormId,
    pub title: Option<String>,
    pub objects: Vec<ObjectResource>,
}

impl FormResource {
    pub fn into_ui_form(self) -> UiForm {
        UiForm {
            form_id: self.form_id,
            title: self.title,
            objects: self
                .objects
                .into_iter()
                .map(ObjectResource::into_ui_object)
                .collect(),
        }
    }
}
