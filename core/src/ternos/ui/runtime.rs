extern crate alloc;

use alloc::{collections::VecDeque, vec::Vec};

use crate::{
    display::RefreshMode,
    platform::{DisplayCaps, DisplayRotation, LogicalStyle},
    ternos::ui::{event::UiEvent, geom::Rect},
};

pub type FormId = u16;
pub type ObjectId = u16;
pub type ObjectIndex = u16;

#[derive(Clone, Debug, Default)]
pub struct FocusState {
    pub form_id: Option<FormId>,
    pub object_id: Option<ObjectId>,
}

#[derive(Clone, Debug, Default)]
pub struct MenuState {
    pub active: bool,
    pub selected_item: Option<u16>,
}

#[derive(Clone, Debug, Default)]
pub struct HelpDialogState {
    pub title: alloc::string::String,
    pub message: alloc::string::String,
    pub scroll: i16,
}

#[derive(Clone, Debug, Default)]
pub struct InvalidationState {
    pub full_redraw: bool,
    pub dirty_rects: Vec<Rect>,
    pub preferred_refresh: Option<RefreshMode>,
}

impl InvalidationState {
    pub fn request_full(&mut self, refresh: RefreshMode) {
        self.full_redraw = true;
        self.dirty_rects.clear();
        self.preferred_refresh = Some(refresh);
    }

    pub fn push_rect(&mut self, rect: Rect, refresh: RefreshMode) {
        if !self.full_redraw {
            self.dirty_rects.push(rect);
        }
        self.preferred_refresh = Some(match self.preferred_refresh {
            Some(current) => max_refresh(current, refresh),
            None => refresh,
        });
    }

    pub fn clear(&mut self) {
        self.full_redraw = false;
        self.dirty_rects.clear();
        self.preferred_refresh = None;
    }
}

#[derive(Clone, Debug, Default)]
pub struct EventQueue {
    events: VecDeque<UiEvent>,
}

impl EventQueue {
    pub fn push(&mut self, event: UiEvent) {
        self.events.push_back(event);
    }

    pub fn pop(&mut self) -> Option<UiEvent> {
        self.events.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[derive(Clone, Debug)]
pub enum UiObject {
    Label { id: ObjectId, bounds: Rect },
    Button { id: ObjectId, bounds: Rect },
    Field { id: ObjectId, bounds: Rect },
    List { id: ObjectId, bounds: Rect },
    Table { id: ObjectId, bounds: Rect },
    Bitmap { id: ObjectId, bounds: Rect },
    Title { id: ObjectId, bounds: Rect },
    Custom { id: ObjectId, bounds: Rect },
}

impl UiObject {
    pub fn id(&self) -> ObjectId {
        match *self {
            Self::Label { id, .. }
            | Self::Button { id, .. }
            | Self::Field { id, .. }
            | Self::List { id, .. }
            | Self::Table { id, .. }
            | Self::Bitmap { id, .. }
            | Self::Title { id, .. }
            | Self::Custom { id, .. } => id,
        }
    }

    pub fn bounds(&self) -> Rect {
        match *self {
            Self::Label { bounds, .. }
            | Self::Button { bounds, .. }
            | Self::Field { bounds, .. }
            | Self::List { bounds, .. }
            | Self::Table { bounds, .. }
            | Self::Bitmap { bounds, .. }
            | Self::Title { bounds, .. }
            | Self::Custom { bounds, .. } => bounds,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct UiForm {
    pub form_id: FormId,
    pub title: Option<alloc::string::String>,
    pub objects: Vec<UiObject>,
}

#[derive(Clone, Debug, Default)]
pub struct UiRuntime {
    pub forms: Vec<UiForm>,
    pub active_form: Option<FormId>,
    pub form_stack: Vec<FormId>,
    pub focus: FocusState,
    pub menu: MenuState,
    pub help: Option<HelpDialogState>,
    pub queue: EventQueue,
    pub invalidation: InvalidationState,
}

impl UiRuntime {
    pub fn set_active_form(&mut self, form_id: FormId) {
        self.active_form = Some(form_id);
        if self.form_stack.last().copied() != Some(form_id) {
            self.form_stack.push(form_id);
        }
        self.invalidation.request_full(RefreshMode::Full);
    }

    pub fn active_form(&self) -> Option<&UiForm> {
        let form_id = self.active_form?;
        self.forms.iter().find(|form| form.form_id == form_id)
    }

    pub fn active_form_mut(&mut self) -> Option<&mut UiForm> {
        let form_id = self.active_form?;
        self.forms.iter_mut().find(|form| form.form_id == form_id)
    }

    pub fn upsert_form(&mut self, form: UiForm) {
        if let Some(existing) = self.forms.iter_mut().find(|entry| entry.form_id == form.form_id) {
            *existing = form;
        } else {
            self.forms.push(form);
        }
        self.invalidation.request_full(RefreshMode::Full);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayProfile {
    pub surface_width: u32,
    pub surface_height: u32,
    pub native_rotation: DisplayRotation,
    pub gray_levels: u8,
    pub bits_per_pixel: u8,
    pub has_partial_refresh: bool,
    pub logical_style: LogicalStyle,
}

impl DisplayProfile {
    pub fn from_display_caps(caps: DisplayCaps) -> Self {
        Self {
            surface_width: crate::display::WIDTH as u32,
            surface_height: crate::display::HEIGHT as u32,
            native_rotation: caps.rotation,
            gray_levels: caps.gray_levels,
            bits_per_pixel: caps.bits_per_pixel,
            has_partial_refresh: caps.partial_refresh,
            logical_style: caps.logical_style,
        }
    }
}

fn max_refresh(a: RefreshMode, b: RefreshMode) -> RefreshMode {
    use RefreshMode::*;
    match (a, b) {
        (Full, _) | (_, Full) => Full,
        (Half, _) | (_, Half) => Half,
        _ => Fast,
    }
}
