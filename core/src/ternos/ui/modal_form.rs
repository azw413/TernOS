extern crate alloc;

use alloc::{string::String, vec::Vec};

use embedded_graphics::pixelcolor::BinaryColor;

use crate::display::RefreshMode;
use crate::palm::{runtime::PalmFont, ui_component::UiNavEvent};

use super::{
    chrome::draw_alert_frame_hi,
    form::{draw_form_button_hi, draw_form_field_hi},
    runtime::UiTableModel,
    table_view::{
        PalmWrappedTextCellRenderer, TableInteraction, TableScrollBarHit, TableScrollBarView, TableView,
    },
    text::{draw_palm_text, palm_text_height, palm_text_width},
    FormResource, ObjectId, ObjectResource, Point, Rect, RenderQueue, UiContext, UiRuntime, View,
};

#[derive(Clone, Debug)]
pub enum ModalTableCellStyle {
    Default,
    PalmWrappedText {
        font_id: u8,
        padding_x: i32,
        padding_y: i32,
        line_spacing: i32,
    },
}

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
        focusable: bool,
    },
    Button {
        id: ObjectId,
        bounds: Rect,
        text: String,
        font_id: u8,
        style: u8,
        no_frame: bool,
    },
    Table {
        id: ObjectId,
        bounds: Rect,
        model: UiTableModel,
        cell_style: ModalTableCellStyle,
    },
    ScrollBar {
        id: ObjectId,
        bounds: Rect,
        table_id: ObjectId,
    },
}

impl ModalWidget {
    pub fn id(&self) -> ObjectId {
        match self {
            Self::Label { id, .. }
            | Self::Field { id, .. }
            | Self::Button { id, .. }
            | Self::Table { id, .. }
            | Self::ScrollBar { id, .. } => *id,
        }
    }

    pub fn bounds(&self) -> Rect {
        match self {
            Self::Label { bounds, .. }
            | Self::Field { bounds, .. }
            | Self::Button { bounds, .. }
            | Self::Table { bounds, .. }
            | Self::ScrollBar { bounds, .. } => *bounds,
        }
    }

    pub fn is_focusable(&self) -> bool {
        match self {
            Self::Field { focusable, .. } => *focusable,
            Self::Button { .. } | Self::Table { .. } => true,
            Self::Label { .. } | Self::ScrollBar { .. } => false,
        }
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
                ModalWidget::Table {
                    id,
                    bounds,
                    model,
                    ..
                } => ObjectResource::Table {
                    id: *id,
                    bounds: *bounds,
                    model: model.clone(),
                },
                ModalWidget::ScrollBar { id, bounds, .. } => ObjectResource::Custom {
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
    TableChanged {
        id: ObjectId,
        selected_row: Option<usize>,
        selected_col: Option<usize>,
        top_row: usize,
        activated: bool,
    },
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModalHit {
    Widget(ObjectId),
    TableRow { table_id: ObjectId, row: usize },
    TableScroll {
        table_id: ObjectId,
        hit: TableScrollBarHit,
    },
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

    pub fn hit_test(&mut self, spec: &ModalFormSpec, point: Point, fonts: &[PalmFont]) -> Option<ModalHit> {
        self.sync(spec);
        for widget in spec.widgets.iter().rev() {
            let bounds = widget.bounds();
            if !(point.x >= bounds.x
                && point.x < bounds.x + bounds.w
                && point.y >= bounds.y
                && point.y < bounds.y + bounds.h)
            {
                continue;
            }
            match widget {
                ModalWidget::Table {
                    id,
                    bounds,
                    model,
                    cell_style,
                } => {
                    if let Some(hit) = self.table_hit(*bounds, model, cell_style, point, fonts) {
                        return Some(match hit {
                            ModalHit::TableRow { row, .. } => ModalHit::TableRow {
                                table_id: *id,
                                row,
                            },
                            ModalHit::TableScroll { hit, .. } => ModalHit::TableScroll {
                                table_id: *id,
                                hit,
                            },
                            ModalHit::Widget(_) => ModalHit::Widget(*id),
                        });
                    }
                    if widget.is_focusable() {
                        return Some(ModalHit::Widget(*id));
                    }
                }
                ModalWidget::ScrollBar {
                    table_id,
                    bounds,
                    ..
                } => {
                    let Some(ModalWidget::Table {
                        model,
                        bounds: table_bounds,
                        cell_style,
                        ..
                    }) = spec.widgets.iter().find(|entry| entry.id() == *table_id)
                    else {
                        continue;
                    };
                    let scrollbar_hit = self.with_table_view(model, cell_style, fonts, |table| {
                        let visible_rows = table.visible_row_count(*table_bounds);
                        let scrollbar = TableScrollBarView::new(
                            model.top_row as usize,
                            visible_rows,
                            model.rows.len(),
                        );
                        scrollbar.hit_test(*bounds, point)
                    });
                    if let Some(hit) = scrollbar_hit {
                        return Some(ModalHit::TableScroll {
                            table_id: *table_id,
                            hit,
                        });
                    }
                }
                ModalWidget::Label { .. } => {}
                _ if widget.is_focusable() => return Some(ModalHit::Widget(widget.id())),
                _ => {}
            }
        }
        None
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

    pub fn select_hit(&mut self, spec: &ModalFormSpec, hit: ModalHit, fonts: &[PalmFont]) -> bool {
        self.sync(spec);
        match hit {
            ModalHit::Widget(id) => self.select_id(spec, id),
            ModalHit::TableRow { table_id, row } => {
                let Some(ModalWidget::Table {
                    bounds,
                    model,
                    cell_style,
                    ..
                }) = spec.widgets.iter().find(|widget| widget.id() == table_id)
                else {
                    return false;
                };
                let before = self.focused_id();
                self.ui_runtime.set_focus(spec.form_id, Some(table_id));
                let interaction = self.with_table_view(model, cell_style, fonts, |table| {
                    table.select_row(*bounds, scrollbar_bounds(spec, table_id), row, false)
                });
                if let Some(ref interaction) = interaction {
                    for rect in &interaction.dirty_rects {
                        self.ui_runtime.invalidation.push_rect(*rect, RefreshMode::Fast);
                    }
                }
                self.focused_id() != before || interaction_changed(model, interaction.as_ref())
            }
            ModalHit::TableScroll { table_id, hit } => {
                let Some(ModalWidget::Table {
                    bounds,
                    model,
                    cell_style,
                    ..
                }) = spec.widgets.iter().find(|widget| widget.id() == table_id)
                else {
                    return false;
                };
                let interaction = self.with_table_view(model, cell_style, fonts, |table| {
                    scrollbar_bounds(spec, table_id)
                        .and_then(|scrollbar_rect| table.apply_scrollbar_hit(*bounds, scrollbar_rect, hit))
                });
                if let Some(interaction) = interaction {
                    for rect in interaction.dirty_rects {
                        self.ui_runtime.invalidation.push_rect(rect, RefreshMode::Fast);
                    }
                    return true;
                }
                false
            }
        }
    }

    pub fn activate_hit(&mut self, spec: &ModalFormSpec, hit: ModalHit, fonts: &[PalmFont]) -> ModalFormAction {
        self.sync(spec);
        match hit {
            ModalHit::Widget(id) => self.activate_id(spec, id),
            ModalHit::TableRow { table_id, row } => {
                let Some(ModalWidget::Table {
                    bounds,
                    model,
                    cell_style,
                    ..
                }) = spec.widgets.iter().find(|widget| widget.id() == table_id)
                else {
                    return ModalFormAction::None;
                };
                self.ui_runtime.set_focus(spec.form_id, Some(table_id));
                let interaction = self.with_table_view(model, cell_style, fonts, |table| {
                    table.select_row(*bounds, scrollbar_bounds(spec, table_id), row, true)
                });
                if let Some(interaction) = interaction {
                    for rect in interaction.dirty_rects.clone() {
                        self.ui_runtime.invalidation.push_rect(rect, RefreshMode::Fast);
                    }
                    return ModalFormAction::TableChanged {
                        id: table_id,
                        selected_row: interaction.selected_row,
                        selected_col: interaction.selected_col,
                        top_row: interaction.top_row,
                        activated: interaction.activated,
                    };
                }
                ModalFormAction::None
            }
            ModalHit::TableScroll { table_id, hit } => {
                let Some(ModalWidget::Table {
                    bounds,
                    model,
                    cell_style,
                    ..
                }) = spec.widgets.iter().find(|widget| widget.id() == table_id)
                else {
                    return ModalFormAction::None;
                };
                let interaction = self.with_table_view(model, cell_style, fonts, |table| {
                    scrollbar_bounds(spec, table_id)
                        .and_then(|scrollbar_rect| table.apply_scrollbar_hit(*bounds, scrollbar_rect, hit))
                });
                if let Some(interaction) = interaction {
                    for rect in interaction.dirty_rects.clone() {
                        self.ui_runtime.invalidation.push_rect(rect, RefreshMode::Fast);
                    }
                    return ModalFormAction::TableChanged {
                        id: table_id,
                        selected_row: interaction.selected_row,
                        selected_col: interaction.selected_col,
                        top_row: interaction.top_row,
                        activated: false,
                    };
                }
                ModalFormAction::None
            }
        }
    }

    pub fn take_dirty_rects(&mut self, fallback: Rect) -> Vec<Rect> {
        let rects = if self.ui_runtime.invalidation.full_redraw {
            alloc::vec![fallback]
        } else {
            self.ui_runtime.invalidation.dirty_rects.clone()
        };
        self.ui_runtime.invalidation.finish_frame();
        if rects.is_empty() {
            alloc::vec![fallback]
        } else {
            rects
        }
    }

    pub fn on_event(&mut self, spec: &ModalFormSpec, event: UiNavEvent, fonts: &[PalmFont]) -> ModalFormAction {
        self.sync(spec);
        match event {
            UiNavEvent::Back => ModalFormAction::Closed,
            UiNavEvent::Confirm => self
                .focused_id()
                .and_then(|id| self.focused_table_action(spec, id, event, fonts))
                .or_else(|| self.focused_id().map(ModalFormAction::Activate))
                .unwrap_or(ModalFormAction::None),
            UiNavEvent::Up => self.table_or_focus(spec, event, FocusDirection::Up, fonts),
            UiNavEvent::Down => self.table_or_focus(spec, event, FocusDirection::Down, fonts),
            UiNavEvent::Left => self.move_focus(spec, FocusDirection::Left),
            UiNavEvent::Right => self.move_focus(spec, FocusDirection::Right),
            UiNavEvent::Tick => ModalFormAction::None,
        }
    }

    fn table_or_focus(
        &mut self,
        spec: &ModalFormSpec,
        event: UiNavEvent,
        direction: FocusDirection,
        fonts: &[PalmFont],
    ) -> ModalFormAction {
        if let Some(id) = self.focused_id() {
            if let Some(action) = self.focused_table_action(spec, id, event, fonts) {
                return action;
            }
        }
        self.move_focus(spec, direction)
    }

    fn focused_table_action(
        &mut self,
        spec: &ModalFormSpec,
        focused_id: ObjectId,
        event: UiNavEvent,
        fonts: &[PalmFont],
    ) -> Option<ModalFormAction> {
        let ModalWidget::Table {
            bounds,
            model,
            cell_style,
            ..
        } = spec.widgets.iter().find(|widget| widget.id() == focused_id)?
        else {
            return None;
        };
        let interaction = match event {
            UiNavEvent::Up => self.with_table_view(model, cell_style, fonts, |table| {
                table.move_selection(*bounds, -1, scrollbar_bounds(spec, focused_id))
            }),
            UiNavEvent::Down => self.with_table_view(model, cell_style, fonts, |table| {
                table.move_selection(*bounds, 1, scrollbar_bounds(spec, focused_id))
            }),
            UiNavEvent::Confirm => self.with_table_view(model, cell_style, fonts, |table| {
                table.activate_selection(*bounds, scrollbar_bounds(spec, focused_id))
            }),
            _ => None,
        }?;
        for rect in interaction.dirty_rects.clone() {
            self.ui_runtime.invalidation.push_rect(rect, RefreshMode::Fast);
        }
        Some(ModalFormAction::TableChanged {
            id: focused_id,
            selected_row: interaction.selected_row,
            selected_col: interaction.selected_col,
            top_row: interaction.top_row,
            activated: interaction.activated,
        })
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

    fn with_table_view<T>(
        &self,
        model: &UiTableModel,
        style: &ModalTableCellStyle,
        fonts: &[PalmFont],
        f: impl FnOnce(&TableView<'_>) -> T,
    ) -> T {
        match style {
            ModalTableCellStyle::Default => {
                let table = TableView::new(model);
                f(&table)
            }
            ModalTableCellStyle::PalmWrappedText {
                font_id,
                padding_x,
                padding_y,
                line_spacing,
            } => {
                let renderer = PalmWrappedTextCellRenderer {
                    fonts,
                    font_id: *font_id,
                    padding_x: *padding_x,
                    padding_y: *padding_y,
                    line_spacing: *line_spacing,
                };
                let mut table = TableView::new(model);
                table.renderer = Some(&renderer);
                f(&table)
            }
        }
    }

    fn table_hit(
        &self,
        bounds: Rect,
        model: &UiTableModel,
        style: &ModalTableCellStyle,
        point: Point,
        fonts: &[PalmFont],
    ) -> Option<ModalHit> {
        let table_hit = match style {
            ModalTableCellStyle::Default => {
                let table = TableView::new(model);
                table.hit_test(bounds, point)
            }
            ModalTableCellStyle::PalmWrappedText {
                font_id,
                padding_x,
                padding_y,
                line_spacing,
            } => {
                let renderer = PalmWrappedTextCellRenderer {
                    fonts,
                    font_id: *font_id,
                    padding_x: *padding_x,
                    padding_y: *padding_y,
                    line_spacing: *line_spacing,
                };
                let mut table = TableView::new(model);
                table.renderer = Some(&renderer);
                table.hit_test(bounds, point)
            }
        };
        table_hit.map(|hit| match hit {
            super::table_view::TableHit::Cell { row, .. } => ModalHit::TableRow {
                table_id: 0,
                row,
            },
        })
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
                ModalWidget::Table {
                    bounds,
                    model,
                    cell_style,
                    ..
                } => match cell_style {
                    ModalTableCellStyle::Default => {
                        let mut table = TableView::new(model);
                        table.clear = false;
                        table.render(ctx, *bounds, &mut RenderQueue::default());
                    }
                    ModalTableCellStyle::PalmWrappedText {
                        font_id,
                        padding_x,
                        padding_y,
                        line_spacing,
                    } => {
                        let renderer = PalmWrappedTextCellRenderer {
                            fonts: self.fonts,
                            font_id: *font_id,
                            padding_x: *padding_x,
                            padding_y: *padding_y,
                            line_spacing: *line_spacing,
                        };
                        let mut table = TableView::new(model);
                        table.clear = false;
                        table.renderer = Some(&renderer);
                        table.render(ctx, *bounds, &mut RenderQueue::default());
                    }
                },
                ModalWidget::ScrollBar {
                    bounds,
                    table_id,
                    ..
                } => {
                    let Some(ModalWidget::Table {
                        bounds: table_bounds,
                        model,
                        cell_style,
                        ..
                    }) = self.spec.widgets.iter().find(|entry| entry.id() == *table_id)
                    else {
                        continue;
                    };
                    let visible_rows = match cell_style {
                        ModalTableCellStyle::Default => TableView::new(model).visible_row_count(*table_bounds),
                        ModalTableCellStyle::PalmWrappedText {
                            font_id,
                            padding_x,
                            padding_y,
                            line_spacing,
                        } => {
                            let renderer = PalmWrappedTextCellRenderer {
                                fonts: self.fonts,
                                font_id: *font_id,
                                padding_x: *padding_x,
                                padding_y: *padding_y,
                                line_spacing: *line_spacing,
                            };
                            let mut table = TableView::new(model);
                            table.renderer = Some(&renderer);
                            table.visible_row_count(*table_bounds)
                        }
                    };
                    if model.rows.len() > visible_rows {
                        let mut scrollbar = TableScrollBarView::new(
                            model.top_row as usize,
                            visible_rows,
                            model.rows.len(),
                        );
                        scrollbar.render(ctx, *bounds, &mut RenderQueue::default());
                    }
                }
            }
        }
    }
}

fn scrollbar_bounds(spec: &ModalFormSpec, table_id: ObjectId) -> Option<Rect> {
    spec.widgets.iter().find_map(|widget| match widget {
        ModalWidget::ScrollBar { bounds, table_id: owner, .. } if *owner == table_id => Some(*bounds),
        _ => None,
    })
}

fn interaction_changed(
    model: &UiTableModel,
    interaction: Option<&TableInteraction>,
) -> bool {
    let Some(interaction) = interaction else {
        return false;
    };
    interaction.selected_row != model.selected_row.map(|row| row as usize)
        || interaction.selected_col != model.selected_col.map(|col| col as usize)
        || interaction.top_row != model.top_row as usize
        || interaction.activated
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
