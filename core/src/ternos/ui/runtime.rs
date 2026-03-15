extern crate alloc;

use alloc::{collections::VecDeque, vec::Vec};

use crate::{
    display::{DamageOverlayKind, DamageOverlayRect, RefreshMode},
    platform::{DisplayCaps, DisplayRotation, LogicalStyle},
    ternos::ui::{event::UiEvent, geom::Rect},
};

pub type FormId = u16;
pub type ObjectId = u16;
pub type ObjectIndex = u16;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiTableColumn {
    pub width: i16,
    pub spacing: i16,
    pub usable: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiTableCell {
    pub text: alloc::string::String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiTableRow {
    pub id: u16,
    pub height: i16,
    pub usable: bool,
    pub selectable: bool,
    pub data: u32,
    pub cells: Vec<UiTableCell>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiTableModel {
    pub cols: u16,
    pub columns: Vec<UiTableColumn>,
    pub rows: Vec<UiTableRow>,
    pub top_row: u16,
    pub selected_row: Option<u16>,
    pub selected_col: Option<u16>,
}

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
    pub damage: DamageFrame,
}

#[derive(Clone, Debug, Default)]
pub struct DamageFrame {
    pub overlay_rects: Vec<DamageOverlayRect>,
    pub presented_rects: Vec<Rect>,
    pub presented_union: Option<Rect>,
    pub presented_refresh: Option<RefreshMode>,
}

impl DamageFrame {
    pub fn clear(&mut self) {
        self.overlay_rects.clear();
        self.clear_presented();
    }

    pub fn clear_presented(&mut self) {
        self.presented_rects.clear();
        self.presented_union = None;
        self.presented_refresh = None;
    }
}

impl InvalidationState {
    pub fn request_full(&mut self, refresh: RefreshMode) {
        self.full_redraw = true;
        self.dirty_rects.clear();
        self.preferred_refresh = Some(refresh);
    }

    pub fn push_rect(&mut self, rect: Rect, refresh: RefreshMode) {
        if !is_valid_rect(rect) {
            return;
        }
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
        self.damage.clear();
    }

    pub fn record_rect(&mut self, kind: DamageOverlayKind, rect: Rect) {
        if !is_valid_rect(rect) {
            return;
        }
        self.damage.overlay_rects.push(DamageOverlayRect { kind, rect });
    }

    pub fn record_old_rect(&mut self, rect: Rect, refresh: RefreshMode) {
        self.record_rect(DamageOverlayKind::Old, rect);
        self.push_rect(rect, refresh);
    }

    pub fn record_new_rect(&mut self, rect: Rect, refresh: RefreshMode) {
        self.record_rect(DamageOverlayKind::New, rect);
        self.push_rect(rect, refresh);
    }

    pub fn record_exposed_rect(&mut self, rect: Rect, refresh: RefreshMode) {
        self.record_rect(DamageOverlayKind::Exposed, rect);
        self.push_rect(rect, refresh);
    }

    pub fn record_transition(
        &mut self,
        old_rect: Option<Rect>,
        new_rect: Option<Rect>,
        refresh: RefreshMode,
    ) {
        if let Some(rect) = old_rect {
            self.record_old_rect(rect, refresh);
        }
        if let Some(rect) = new_rect {
            self.record_new_rect(rect, refresh);
        }
    }

    pub fn record_presented_rect(&mut self, rect: Rect, refresh: RefreshMode) {
        if !is_valid_rect(rect) {
            return;
        }
        self.damage.presented_rects.push(rect);
        self.damage.presented_union = Some(match self.damage.presented_union {
            Some(current) => union_rect(current, rect),
            None => rect,
        });
        self.damage.presented_refresh = Some(match self.damage.presented_refresh {
            Some(current) => max_refresh(current, refresh),
            None => refresh,
        });
        self.damage.overlay_rects.push(DamageOverlayRect {
            kind: DamageOverlayKind::Presented,
            rect,
        });
    }

    pub fn finish_frame(&mut self) {
        self.clear();
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiObject {
    Label { id: ObjectId, bounds: Rect },
    Button { id: ObjectId, bounds: Rect },
    Field { id: ObjectId, bounds: Rect },
    List { id: ObjectId, bounds: Rect },
    Table { id: ObjectId, bounds: Rect, model: UiTableModel },
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
        if self.active_form == Some(form_id) {
            return;
        }
        let old_bounds = self.active_form().and_then(form_bounds);
        self.active_form = Some(form_id);
        if self.form_stack.last().copied() != Some(form_id) {
            self.form_stack.push(form_id);
        }
        let new_bounds = self.active_form().and_then(form_bounds);
        self.invalidation
            .record_transition(old_bounds, new_bounds, RefreshMode::Full);
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
            if existing.title == form.title && existing.objects == form.objects {
                return;
            }
            for old_object in &existing.objects {
                match form.objects.iter().find(|object| object.id() == old_object.id()) {
                    Some(new_object) => {
                        if old_object.bounds() != new_object.bounds() {
                            self.invalidation.record_transition(
                                Some(old_object.bounds()),
                                Some(new_object.bounds()),
                                RefreshMode::Full,
                            );
                        }
                    }
                    None => {
                        self.invalidation
                            .record_old_rect(old_object.bounds(), RefreshMode::Full);
                    }
                }
            }
            for new_object in &form.objects {
                if existing
                    .objects
                    .iter()
                    .all(|object| object.id() != new_object.id())
                {
                    self.invalidation
                        .record_new_rect(new_object.bounds(), RefreshMode::Full);
                }
            }
            *existing = form;
        } else {
            for object in &form.objects {
                self.invalidation
                    .record_new_rect(object.bounds(), RefreshMode::Full);
            }
            self.forms.push(form);
        }
    }

    pub fn set_focus(&mut self, form_id: FormId, object_id: Option<ObjectId>) {
        if self.focus.form_id == Some(form_id) && self.focus.object_id == object_id {
            return;
        }
        let old_bounds = self.focused_object().map(UiObject::bounds);
        let new_bounds = self.object_bounds(form_id, object_id);
        self.focus.form_id = Some(form_id);
        self.focus.object_id = object_id;
        self.invalidation
            .record_transition(old_bounds, new_bounds, RefreshMode::Fast);
    }

    pub fn focused_object(&self) -> Option<&UiObject> {
        let form_id = self.focus.form_id?;
        let object_id = self.focus.object_id?;
        self.forms
            .iter()
            .find(|form| form.form_id == form_id)
            .and_then(|form| form.objects.iter().find(|object| object.id() == object_id))
    }

    pub fn has_object(&self, form_id: FormId, object_id: ObjectId) -> bool {
        self.forms
            .iter()
            .find(|form| form.form_id == form_id)
            .map(|form| form.objects.iter().any(|object| object.id() == object_id))
            .unwrap_or(false)
    }

    pub fn object_bounds(&self, form_id: FormId, object_id: Option<ObjectId>) -> Option<Rect> {
        let object_id = object_id?;
        self.forms
            .iter()
            .find(|form| form.form_id == form_id)
            .and_then(|form| form.objects.iter().find(|object| object.id() == object_id))
            .map(UiObject::bounds)
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

fn union_rect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

fn is_valid_rect(rect: Rect) -> bool {
    rect.w > 0 && rect.h > 0
}

fn form_bounds(form: &UiForm) -> Option<Rect> {
    let mut bounds = None;
    for object in &form.objects {
        let rect = object.bounds();
        if !is_valid_rect(rect) {
            continue;
        }
        bounds = Some(match bounds {
            Some(current) => union_rect(current, rect),
            None => rect,
        });
    }
    bounds
}
