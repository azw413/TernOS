extern crate alloc;

use alloc::{string::String, vec::Vec};

use embedded_graphics::pixelcolor::BinaryColor;

use crate::palm::{runtime::PalmFont, ui_component::UiNavEvent};

use super::{
    chrome::draw_alert_frame_hi,
    form::{draw_form_button_hi, draw_form_field_hi},
    text::{draw_palm_text, palm_text_height, palm_text_width},
    FormResource, ObjectId, ObjectResource, Point, Rect, RenderQueue, UiContext, UiRuntime, View,
};

#[derive(Clone, Debug)]
pub enum ModalWidget {
    Label {
        id: ObjectId,
        bounds: Rect,
        text: String,
        font_id: u8,
    },
    Field {
        id: ObjectId,
        bounds: Rect,
        text: String,
        font_id: u8,
    },
    Button {
        id: ObjectId,
        bounds: Rect,
        text: String,
        font_id: u8,
        style: u8,
        no_frame: bool,
    },
}

impl ModalWidget {
    pub fn id(&self) -> ObjectId {
        match self {
            Self::Label { id, .. } | Self::Field { id, .. } | Self::Button { id, .. } => *id,
        }
    }

    pub fn bounds(&self) -> Rect {
        match self {
            Self::Label { bounds, .. } | Self::Field { bounds, .. } | Self::Button { bounds, .. } => *bounds,
        }
    }

    pub fn is_focusable(&self) -> bool {
        matches!(self, Self::Field { .. } | Self::Button { .. })
    }
}

#[derive(Clone, Debug)]
pub struct ModalFormSpec {
    pub form_id: u16,
    pub bounds: Rect,
    pub title: String,
    pub widgets: Vec<ModalWidget>,
    pub default_focus: Option<ObjectId>,
}

impl ModalFormSpec {
    pub fn as_form_resource(&self) -> FormResource {
        let objects = self
            .widgets
            .iter()
            .map(|widget| match widget {
                ModalWidget::Label { id, bounds, .. } => ObjectResource::Label {
                    id: *id,
                    bounds: *bounds,
                },
                ModalWidget::Field { id, bounds, .. } => ObjectResource::Field {
                    id: *id,
                    bounds: *bounds,
                },
                ModalWidget::Button { id, bounds, .. } => ObjectResource::Button {
                    id: *id,
                    bounds: *bounds,
                },
            })
            .collect();
        FormResource {
            form_id: self.form_id,
            title: Some(self.title.clone()),
            objects,
        }
    }

    fn focusable_ids(&self) -> Vec<ObjectId> {
        self.widgets
            .iter()
            .filter(|widget| widget.is_focusable())
            .map(ModalWidget::id)
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalFormAction {
    None,
    Redraw,
    Activate(ObjectId),
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FocusDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Default)]
pub struct ModalFormController {
    ui_runtime: UiRuntime,
}

impl ModalFormController {
    pub fn reset(&mut self) {
        self.ui_runtime = UiRuntime::default();
    }

    pub fn sync(&mut self, spec: &ModalFormSpec) {
        let form = spec.as_form_resource().into_ui_form();
        self.ui_runtime.upsert_form(form);
        self.ui_runtime.set_active_form(spec.form_id);
        let current = self
            .ui_runtime
            .focus
            .object_id
            .filter(|id| self.ui_runtime.has_object(spec.form_id, *id));
        if current.is_some() {
            self.ui_runtime.set_focus(spec.form_id, current);
            return;
        }
        let target = spec
            .default_focus
            .filter(|id| self.ui_runtime.has_object(spec.form_id, *id))
            .or_else(|| spec.focusable_ids().into_iter().next());
        self.ui_runtime.set_focus(spec.form_id, target);
    }

    pub fn focused_id(&self) -> Option<ObjectId> {
        self.ui_runtime.focus.object_id
    }

    pub fn hit_test(&mut self, spec: &ModalFormSpec, point: Point) -> Option<ObjectId> {
        self.sync(spec);
        spec.widgets
            .iter()
            .filter(|widget| widget.is_focusable())
            .rev()
            .find(|widget| {
                let bounds = widget.bounds();
                point.x >= bounds.x
                    && point.x < bounds.x + bounds.w
                    && point.y >= bounds.y
                    && point.y < bounds.y + bounds.h
            })
            .map(ModalWidget::id)
    }

    pub fn select_id(&mut self, spec: &ModalFormSpec, id: ObjectId) -> bool {
        self.sync(spec);
        let before = self.focused_id();
        self.ui_runtime.set_focus(spec.form_id, Some(id));
        self.focused_id() != before
    }

    pub fn activate_id(&mut self, spec: &ModalFormSpec, id: ObjectId) -> ModalFormAction {
        self.sync(spec);
        if self.focused_id() != Some(id) {
            self.ui_runtime.set_focus(spec.form_id, Some(id));
        }
        ModalFormAction::Activate(id)
    }

    pub fn on_event(&mut self, spec: &ModalFormSpec, event: UiNavEvent) -> ModalFormAction {
        self.sync(spec);
        match event {
            UiNavEvent::Back => ModalFormAction::Closed,
            UiNavEvent::Confirm => self
                .focused_id()
                .map(ModalFormAction::Activate)
                .unwrap_or(ModalFormAction::None),
            UiNavEvent::Up => self.move_focus(spec, FocusDirection::Up),
            UiNavEvent::Down => self.move_focus(spec, FocusDirection::Down),
            UiNavEvent::Left => self.move_focus(spec, FocusDirection::Left),
            UiNavEvent::Right => self.move_focus(spec, FocusDirection::Right),
            UiNavEvent::Tick => ModalFormAction::None,
        }
    }

    fn move_focus(&mut self, spec: &ModalFormSpec, direction: FocusDirection) -> ModalFormAction {
        let focusable: Vec<&ModalWidget> = spec.widgets.iter().filter(|w| w.is_focusable()).collect();
        if focusable.is_empty() {
            return ModalFormAction::None;
        }
        let current_id = self
            .focused_id()
            .filter(|id| focusable.iter().any(|w| w.id() == *id))
            .unwrap_or_else(|| focusable[0].id());
        let next = directional_target(&focusable, current_id, direction)
            .or_else(|| linear_fallback_target(&focusable, current_id, direction));
        if let Some(next_id) = next {
            let before = self.focused_id();
            self.ui_runtime.set_focus(spec.form_id, Some(next_id));
            if self.focused_id() != before {
                return ModalFormAction::Redraw;
            }
        }
        ModalFormAction::None
    }
}

pub struct ModalFormView<'a> {
    pub spec: &'a ModalFormSpec,
    pub fonts: &'a [PalmFont],
    pub focused_id: Option<ObjectId>,
}

impl View for ModalFormView<'_> {
    fn render(&mut self, ctx: &mut UiContext<'_>, rect: Rect, _rq: &mut RenderQueue) {
        draw_alert_frame_hi(
            ctx.buffers,
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            34,
        );

        let title_w = palm_text_width(&self.spec.title, 1, self.fonts, 1);
        let title_h = palm_text_height(1, self.fonts, 1);
        draw_palm_text(
            ctx.buffers,
            &self.spec.title,
            rect.x + ((rect.w - title_w) / 2).max(0),
            rect.y + ((34 - title_h) / 2).max(2) - 1,
            1,
            self.fonts,
            1,
            BinaryColor::On,
        );

        for widget in &self.spec.widgets {
            let bounds = widget.bounds();
            let focused = self.focused_id == Some(widget.id());
            match widget {
                ModalWidget::Label {
                    text, font_id, ..
                } => {
                    draw_palm_text(
                        ctx.buffers,
                        text,
                        bounds.x,
                        bounds.y,
                        *font_id,
                        self.fonts,
                        1,
                        BinaryColor::Off,
                    );
                }
                ModalWidget::Field {
                    text, font_id, ..
                } => {
                    draw_form_field_hi(ctx.buffers, bounds.x, bounds.y, bounds.w, bounds.h, focused);
                    draw_palm_text(
                        ctx.buffers,
                        text,
                        bounds.x + 4,
                        bounds.y + ((bounds.h - palm_text_height(*font_id, self.fonts, 1)) / 2).max(1),
                        *font_id,
                        self.fonts,
                        1,
                        BinaryColor::Off,
                    );
                }
                ModalWidget::Button {
                    text,
                    font_id,
                    style,
                    no_frame,
                    ..
                } => {
                    draw_form_button_hi(
                        ctx.buffers,
                        self.fonts,
                        bounds.x,
                        bounds.y,
                        bounds.w,
                        bounds.h,
                        *font_id,
                        *style,
                        *no_frame,
                        text,
                        focused,
                    );
                }
            }
        }
    }
}

fn directional_target(
    controls: &[&ModalWidget],
    current_id: ObjectId,
    direction: FocusDirection,
) -> Option<ObjectId> {
    let current = controls.iter().find(|c| c.id() == current_id)?;
    let current = current.bounds();
    let cur_cx = current.x + current.w / 2;
    let cur_cy = current.y + current.h / 2;
    let mut best: Option<(u8, i32, i32, i32, i32, ObjectId)> = None;
    let mut best_id = None;

    for control in controls {
        if control.id() == current_id {
            continue;
        }
        let rect = control.bounds();
        let cx = rect.x + rect.w / 2;
        let cy = rect.y + rect.h / 2;
        let (in_dir, primary, secondary, overlap) = match direction {
            FocusDirection::Up => (
                cy < cur_cy,
                cur_cy - cy,
                (cx - cur_cx).abs(),
                axis_overlap(current.x, current.x + current.w, rect.x, rect.x + rect.w),
            ),
            FocusDirection::Down => (
                cy > cur_cy,
                cy - cur_cy,
                (cx - cur_cx).abs(),
                axis_overlap(current.x, current.x + current.w, rect.x, rect.x + rect.w),
            ),
            FocusDirection::Left => (
                cx < cur_cx,
                cur_cx - cx,
                (cy - cur_cy).abs(),
                axis_overlap(current.y, current.y + current.h, rect.y, rect.y + rect.h),
            ),
            FocusDirection::Right => (
                cx > cur_cx,
                cx - cur_cx,
                (cy - cur_cy).abs(),
                axis_overlap(current.y, current.y + current.h, rect.y, rect.y + rect.h),
            ),
        };
        if !in_dir {
            continue;
        }
        let rank = if overlap > 0 { 0 } else { 1 };
        let candidate = (rank, primary, secondary, -overlap, rect.y, control.id());
        if best.map(|cur| candidate < cur).unwrap_or(true) {
            best = Some(candidate);
            best_id = Some(control.id());
        }
    }
    best_id
}

fn linear_fallback_target(
    controls: &[&ModalWidget],
    current_id: ObjectId,
    direction: FocusDirection,
) -> Option<ObjectId> {
    let mut ids: Vec<(ObjectId, Rect)> = controls.iter().map(|c| (c.id(), c.bounds())).collect();
    ids.sort_by_key(|(id, rect)| (rect.y, rect.x, *id));
    let index = ids.iter().position(|(id, _)| *id == current_id)?;
    match direction {
        FocusDirection::Up | FocusDirection::Left => index.checked_sub(1).map(|i| ids[i].0),
        FocusDirection::Down | FocusDirection::Right => ids.get(index + 1).map(|entry| entry.0),
    }
}

fn axis_overlap(a0: i32, a1: i32, b0: i32, b1: i32) -> i32 {
    (a1.min(b1) - a0.max(b0)).max(0)
}
