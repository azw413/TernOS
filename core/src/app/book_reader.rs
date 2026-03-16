extern crate alloc;

use alloc::{collections::BTreeMap, format, rc::Rc, string::{String, ToString}, vec, vec::Vec};

use embedded_graphics::{
    geometry::Size,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, OriginDimensions, Point, Primitive},
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
    Drawable,
};

use crate::display::Display;
use crate::framebuffer::{DisplayBuffers, Rotation, BUFFER_SIZE, HEIGHT as FB_HEIGHT, WIDTH as FB_WIDTH};
use crate::image_viewer::{AppSource, ImageData, ImageError};
use crate::input;
use crate::palm::{
    controller::HelpDialogAction,
    menu_preview::{MenuBarPreview, MenuItemPreview, MenuPullDownPreview},
    runner::RuntimeHelpDialog,
    ui::{draw_help_dialog_native, draw_menu_overlay_native},
};
use crate::render_policy::RenderPolicy;
use crate::ternos::ui::{
    flush_queue,
    auto_button_layout_for_label,
    chrome::draw_alert_frame_hi,
    form::draw_form_button_hi,
    palm_text_height,
    palm_text_width,
    ModalFormAction, ModalFormController, ModalFormSpec, ModalFormView, ModalWidget, ObjectId,
    Rect, RenderQueue, StatusBarActionState, StatusBarView, TableCellRenderer, TableHit,
    TableScrollBarHit, TableScrollBarView, TableView, UiContext, UiTableCell, UiTableModel,
    UiTableRow, View,
};

const LIST_TOP: i32 = 60;
const LINE_HEIGHT: i32 = 24;
const LIST_MARGIN_X: i32 = 16;
const HEADER_Y: i32 = 24;
const BOOK_FULL_REFRESH_EVERY: usize = 10;
const BOOK_STATUS_H: i32 = StatusBarView::HEIGHT;
const BOOK_CONTENT_TOP: i32 = BOOK_STATUS_H + 5;

fn reader_page_origin_y(book: &crate::trbk::TrbkBookInfo) -> i32 {
    let desired_top = BOOK_STATUS_H + ((book.metadata.line_height as i32) / 2).max(12);
    desired_top - book.metadata.margin_top as i32
}

#[derive(Clone, Copy, Debug)]
pub enum PageTurnIndicator {
    Forward,
    Backward,
}

pub struct BookReaderState {
    pub current_book: Option<Rc<crate::trbk::TrbkBookInfo>>,
    pub current_page_ops: Option<crate::trbk::TrbkPage>,
    pub next_page_ops: Option<crate::trbk::TrbkPage>,
    pub toc_selected: usize,
    pub toc_top_row: usize,
    pub toc_cancel_focused: bool,
    pub toc_labels: Option<Vec<String>>,
    pub current_page: usize,
    pub book_turns_since_full: usize,
    pub last_rendered_page: Option<usize>,
    pub page_turn_indicator: Option<PageTurnIndicator>,
    pub overlay: Option<ReaderOverlay>,
    pub overlay_form: ModalFormController,
}

pub struct BookReaderContext<'a, S: AppSource> {
    pub display_buffers: &'a mut DisplayBuffers,
    pub gray2_lsb: &'a mut [u8],
    pub gray2_msb: &'a mut [u8],
    pub source: &'a mut S,
    pub full_refresh: &'a mut bool,
    pub render_policy: RenderPolicy,
    pub battery_percent: Option<u8>,
    pub palm_fonts: &'a [crate::palm::runtime::PalmFont],
}

pub struct BookViewResult {
    pub exit: bool,
    pub open_toc: bool,
    pub dirty: bool,
}

pub struct TocResult {
    pub exit: bool,
    pub jumped: bool,
    pub dirty: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TocTouchHit {
    Row(usize),
    Cancel,
    ScrollUp,
    ScrollDown,
    ScrollTrack(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaderMenuCommand {
    Contents,
    Page,
    Stats,
    Help,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReaderOverlay {
    PageJump { target_page: usize },
    Stats,
    Help(RuntimeHelpDialog),
}

pub struct ReaderOverlayResult {
    pub close: bool,
    pub dirty: bool,
    pub jumped: bool,
}

const READER_MENU_NAVIGATE: u16 = 1;
const READER_MENU_VIEW: u16 = 2;
const READER_CMD_CONTENTS: u16 = 100;
const READER_CMD_PAGE: u16 = 101;
const READER_CMD_STATS: u16 = 200;
const READER_CMD_HELP: u16 = 201;
const READER_OVERLAY_FORM_ID: u16 = 3000;
const READER_OVERLAY_BTN_OK: ObjectId = 3001;
const READER_OVERLAY_BTN_CANCEL: ObjectId = 3002;
const READER_OVERLAY_DIGIT_UP_BASE: ObjectId = 3100;
const READER_OVERLAY_DIGIT_FIELD_BASE: ObjectId = 3200;
const READER_OVERLAY_DIGIT_DOWN_BASE: ObjectId = 3300;

fn reader_help_text() -> String {
    [
        "Left/Up: previous page",
        "Right/Down: next page",
        "Confirm: focus Menu in status bar",
        "Menu -> Contents/Page/Stats/Help",
        "Home exits to launcher",
    ]
    .join("\n")
}

fn page_jump_digit_count(page_count: usize) -> usize {
    page_count.max(1).to_string().len().max(1)
}

fn page_jump_display_value(target_page: usize, page_count: usize) -> usize {
    target_page
        .saturating_add(1)
        .clamp(1, page_count.max(1))
}

fn page_jump_place_value(digits: usize, index: usize) -> usize {
    let power = digits.saturating_sub(index + 1) as u32;
    10usize.saturating_pow(power)
}

fn page_jump_adjust_digit(
    display_value: usize,
    page_count: usize,
    digits: usize,
    index: usize,
    delta: i32,
) -> usize {
    let place = page_jump_place_value(digits, index);
    let max_value = page_count.max(1);
    if delta < 0 {
        display_value.saturating_sub(place).clamp(1, max_value)
    } else {
        display_value.saturating_add(place).clamp(1, max_value)
    }
}

fn toc_modal_rect() -> Rect {
    Rect::new(30, BOOK_STATUS_H + 10, 420, 620)
}

fn toc_cancel_rect(modal: Rect) -> Rect {
    let cancel_w = 74;
    Rect::new(modal.x + modal.w - cancel_w - 16, modal.y + modal.h - 40, cancel_w, 24)
}

fn toc_table_rect(modal: Rect, cancel_rect: Rect) -> Rect {
    Rect::new(
        modal.x + 14,
        modal.y + 44,
        modal.w - 28 - 12,
        (cancel_rect.y - 14) - (modal.y + 44),
    )
}

fn toc_scrollbar_rect(_modal: Rect, table_rect: Rect) -> Rect {
    Rect::new(table_rect.x + table_rect.w + 4, table_rect.y, 11, table_rect.h)
}

struct TocTableRenderer<'a> {
    palm_fonts: &'a [crate::palm::runtime::PalmFont],
}

impl TableCellRenderer for TocTableRenderer<'_> {
    fn render_cell(
        &self,
        ctx: &mut UiContext<'_>,
        cell_rect: Rect,
        _row: &UiTableRow,
        cell: &UiTableCell,
        _row_index: usize,
        _col_index: usize,
        selected: bool,
    ) {
        let lines = wrap_toc_lines(&cell.text, (cell_rect.w - 12).max(20), self.palm_fonts);
        let line_h = (palm_text_height(0, self.palm_fonts, 1) + 2).max(10);
        for (line_idx, line) in lines.iter().enumerate() {
            crate::ternos::ui::draw_palm_text(
                ctx.buffers,
                line,
                cell_rect.x + 6,
                cell_rect.y + 5 + line_idx as i32 * line_h,
                0,
                self.palm_fonts,
                1,
                if selected {
                    BinaryColor::On
                } else {
                    BinaryColor::Off
                },
            );
        }
    }
}

fn wrap_toc_lines(text: &str, max_w: i32, fonts: &[crate::palm::runtime::PalmFont]) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current, word)
        };
        if palm_text_width(&candidate, 0, fonts, 1) <= max_w || current.is_empty() {
            current = candidate;
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(text.to_string());
    }
    lines
}

impl BookReaderState {
    fn ensure_toc_labels(&mut self, book: &crate::trbk::TrbkBookInfo) {
        if self.toc_labels.is_some() {
            return;
        }
        let mut labels: Vec<String> = Vec::with_capacity(book.toc.len());
        for entry in &book.toc {
            let mut label = String::new();
            let indent = (entry.level as usize).min(6);
            for _ in 0..indent {
                label.push_str("  ");
            }
            label.push_str(entry.title.as_str());
            labels.push(label);
        }
        self.toc_labels = Some(labels);
    }

    fn toc_rows(&self, table_w: i32, fonts: &[crate::palm::runtime::PalmFont]) -> Vec<UiTableRow> {
        let labels = self.toc_labels.as_ref().map(Vec::as_slice).unwrap_or(&[]);
        let text_w = (table_w - 12).max(20);
        let line_h = (palm_text_height(0, fonts, 1) + 2).max(10);
        labels
            .iter()
            .enumerate()
            .map(|(idx, label)| {
                let lines = wrap_toc_lines(label, text_w, fonts);
                let height = (lines.len() as i32 * line_h + 8).max(line_h + 8) as i16;
                UiTableRow {
                id: idx as u16,
                height,
                usable: true,
                selectable: true,
                data: 0,
                cells: vec![UiTableCell { text: label.clone() }],
            }})
            .collect()
    }

    fn toc_table_model(&self, table_w: i32, fonts: &[crate::palm::runtime::PalmFont]) -> UiTableModel {
        let labels = self.toc_labels.as_ref().map(Vec::as_slice).unwrap_or(&[]);
        UiTableModel {
            rows: self.toc_rows(table_w, fonts),
            cols: 1,
            columns: Vec::new(),
            selected_row: Some(self.toc_selected.min(labels.len().saturating_sub(1)) as u16),
            selected_col: Some(0),
            top_row: self.toc_top_row as u16,
        }
    }

    fn toc_visible_rows(&self, fonts: &[crate::palm::runtime::PalmFont]) -> usize {
        let modal = toc_modal_rect();
        let cancel_rect = toc_cancel_rect(modal);
        let table_rect = toc_table_rect(modal, cancel_rect);
        let model = self.toc_table_model(table_rect.w, fonts);
        TableView::new(&model).visible_row_count(table_rect)
    }

    fn clamp_toc_selection_to_view(&mut self, total_rows: usize, visible_rows: usize) {
        if total_rows == 0 || visible_rows == 0 {
            self.toc_selected = 0;
            self.toc_top_row = 0;
            self.toc_cancel_focused = false;
            return;
        }
        let max_top = total_rows.saturating_sub(visible_rows);
        self.toc_top_row = self.toc_top_row.min(max_top);
        let bottom = self
            .toc_top_row
            .saturating_add(visible_rows.saturating_sub(1))
            .min(total_rows.saturating_sub(1));
        if self.toc_selected < self.toc_top_row {
            self.toc_selected = self.toc_top_row;
        } else if self.toc_selected > bottom {
            self.toc_selected = bottom;
        }
    }

    pub fn new() -> Self {
        Self {
            current_book: None,
            current_page_ops: None,
            next_page_ops: None,
            toc_selected: 0,
            toc_top_row: 0,
            toc_cancel_focused: false,
            toc_labels: None,
            current_page: 0,
            book_turns_since_full: 0,
            last_rendered_page: None,
            page_turn_indicator: None,
            overlay: None,
            overlay_form: ModalFormController::default(),
        }
    }

    pub fn clear(&mut self) {
        self.current_book = None;
        self.current_page_ops = None;
        self.next_page_ops = None;
        self.toc_selected = 0;
        self.toc_top_row = 0;
        self.toc_cancel_focused = false;
        self.toc_labels = None;
        self.current_page = 0;
        self.book_turns_since_full = 0;
        self.last_rendered_page = None;
        self.page_turn_indicator = None;
        self.overlay = None;
        self.overlay_form.reset();
    }

    pub fn close<S: AppSource>(&mut self, source: &mut S) {
        self.clear();
        source.close_trbk();
    }

    pub fn open<S: AppSource>(
        &mut self,
        source: &mut S,
        path: &[String],
        entry: &crate::image_viewer::ImageEntry,
        entry_name: &str,
        book_positions: &BTreeMap<String, usize>,
    ) -> Result<(), ImageError> {
        let info = source.open_trbk(path, entry)?;
        self.current_book = Some(info);
        self.toc_labels = None;
        self.current_page = book_positions.get(entry_name).copied().unwrap_or(0);
        self.current_page_ops = source.trbk_page(self.current_page).ok();
        self.next_page_ops = None;
        self.last_rendered_page = None;
        self.book_turns_since_full = 0;
        Ok(())
    }

    pub fn has_book(&self) -> bool {
        self.current_book.is_some()
    }

    pub fn take_page_turn_indicator(&mut self) -> Option<PageTurnIndicator> {
        self.page_turn_indicator.take()
    }

    pub fn handle_view_input<S: AppSource>(
        &mut self,
        source: &mut S,
        buttons: &input::ButtonState,
    ) -> BookViewResult {
        let mut result = BookViewResult {
            exit: false,
            open_toc: false,
            dirty: false,
        };

        if buttons.is_pressed(input::Buttons::Left)
            || buttons.is_pressed(input::Buttons::Up)
        {
            if self.current_page > 0 {
                self.current_page = self.current_page.saturating_sub(1);
                self.current_page_ops = None;
                self.next_page_ops = None;
                self.book_turns_since_full = self.book_turns_since_full.saturating_add(1);
                self.page_turn_indicator = Some(PageTurnIndicator::Backward);
                result.dirty = true;
            }
            return result;
        }

        if buttons.is_pressed(input::Buttons::Right)
            || buttons.is_pressed(input::Buttons::Down)
        {
            if let Some(book) = &self.current_book {
                if self.current_page + 1 < book.page_count {
                    self.current_page += 1;
                    if let Some(next_ops) = self.next_page_ops.take() {
                        self.current_page_ops = Some(next_ops);
                    } else {
                        self.current_page_ops = None;
                    }
                    self.next_page_ops = None;
                    self.book_turns_since_full = self.book_turns_since_full.saturating_add(1);
                    self.page_turn_indicator = Some(PageTurnIndicator::Forward);
                    result.dirty = true;
                }
            }
            return result;
        }

        if buttons.is_pressed(input::Buttons::Confirm) {
            return result;
        }

        if buttons.is_pressed(input::Buttons::Back) {
            result.exit = true;
            result.dirty = true;
            return result;
        }

        // Keep source used to avoid unused warnings; may be needed later.
        let _ = source;
        result
    }

    pub fn apply_overlay_action(&mut self, action: ModalFormAction) -> ReaderOverlayResult {
        let mut result = ReaderOverlayResult {
            close: false,
            dirty: false,
            jumped: false,
        };
        if self.overlay.is_none() {
            return result;
        }
        match action {
            ModalFormAction::None => {}
            ModalFormAction::Redraw => {
                result.dirty = true;
            }
            ModalFormAction::Closed => {
                self.overlay = None;
                self.overlay_form.reset();
                result.close = true;
                result.dirty = true;
            }
            ModalFormAction::Activate(id) => match self.overlay.as_ref() {
                Some(ReaderOverlay::PageJump { .. }) => {
                    let Some(ReaderOverlay::PageJump { target_page }) = self.overlay.as_mut() else {
                        return result;
                    };
                    let page_count = self
                        .current_book
                        .as_ref()
                        .map(|book| book.page_count)
                        .unwrap_or(1);
                    match id {
                        READER_OVERLAY_BTN_OK => {
                            self.current_page = *target_page;
                            self.current_page_ops = None;
                            self.next_page_ops = None;
                            self.last_rendered_page = None;
                            self.book_turns_since_full = 0;
                            self.overlay = None;
                            self.overlay_form.reset();
                            result.close = true;
                            result.jumped = true;
                            result.dirty = true;
                        }
                        READER_OVERLAY_BTN_CANCEL => {
                            self.overlay = None;
                            self.overlay_form.reset();
                            result.close = true;
                            result.dirty = true;
                        }
                        id if id >= READER_OVERLAY_DIGIT_UP_BASE
                            && id < READER_OVERLAY_DIGIT_UP_BASE + 16 =>
                        {
                            let digits = page_jump_digit_count(page_count);
                            let index = (id - READER_OVERLAY_DIGIT_UP_BASE) as usize;
                            if index < digits {
                                let display_value =
                                    page_jump_display_value(*target_page, page_count);
                                let next_value = page_jump_adjust_digit(
                                    display_value,
                                    page_count,
                                    digits,
                                    index,
                                    -1,
                                );
                                *target_page = next_value.saturating_sub(1);
                                result.dirty = true;
                            }
                        }
                        id if id >= READER_OVERLAY_DIGIT_DOWN_BASE
                            && id < READER_OVERLAY_DIGIT_DOWN_BASE + 16 =>
                        {
                            let digits = page_jump_digit_count(page_count);
                            let index = (id - READER_OVERLAY_DIGIT_DOWN_BASE) as usize;
                            if index < digits {
                                let display_value =
                                    page_jump_display_value(*target_page, page_count);
                                let next_value = page_jump_adjust_digit(
                                    display_value,
                                    page_count,
                                    digits,
                                    index,
                                    1,
                                );
                                *target_page = next_value.saturating_sub(1);
                                result.dirty = true;
                            }
                        }
                        _ => {}
                    }
                }
                Some(ReaderOverlay::Stats) => {
                    if id == READER_OVERLAY_BTN_OK || id == READER_OVERLAY_BTN_CANCEL {
                        self.overlay = None;
                        self.overlay_form.reset();
                        result.close = true;
                        result.dirty = true;
                    }
                }
                Some(ReaderOverlay::Help(_)) => {}
                None => {}
            },
        }
        result
    }

    pub fn handle_overlay_input(
        &mut self,
        buttons: &input::ButtonState,
    ) -> ReaderOverlayResult {
        if self.overlay.is_none() {
            return ReaderOverlayResult {
                close: false,
                dirty: false,
                jumped: false,
            };
        }
        let Some(spec) = self.overlay_spec() else {
            return ReaderOverlayResult {
                close: false,
                dirty: false,
                jumped: false,
            };
        };
        let event = if buttons.is_pressed(input::Buttons::Back) {
            Some(crate::palm::ui_component::UiNavEvent::Back)
        } else if buttons.is_pressed(input::Buttons::Left) {
            Some(crate::palm::ui_component::UiNavEvent::Left)
        } else if buttons.is_pressed(input::Buttons::Right) {
            Some(crate::palm::ui_component::UiNavEvent::Right)
        } else if buttons.is_pressed(input::Buttons::Up) {
            Some(crate::palm::ui_component::UiNavEvent::Up)
        } else if buttons.is_pressed(input::Buttons::Down) {
            Some(crate::palm::ui_component::UiNavEvent::Down)
        } else if buttons.is_pressed(input::Buttons::Confirm) {
            Some(crate::palm::ui_component::UiNavEvent::Confirm)
        } else {
            None
        };
        let Some(event) = event else {
            return ReaderOverlayResult {
                close: false,
                dirty: false,
                jumped: false,
            };
        };

        let action = self.overlay_form.on_event(&spec, event);
        self.apply_overlay_action(action)
    }

    pub fn apply_help_action(&mut self, action: HelpDialogAction) -> ReaderOverlayResult {
        let mut result = ReaderOverlayResult {
            close: false,
            dirty: false,
            jumped: false,
        };
        let Some(ReaderOverlay::Help(dialog)) = self.overlay.as_mut() else {
            return result;
        };
        match action {
            HelpDialogAction::None => {}
            HelpDialogAction::Redraw => {
                result.dirty = true;
            }
            HelpDialogAction::Scroll(delta) => {
                if delta < 0 {
                    dialog.scroll_line = dialog.scroll_line.saturating_sub(delta.unsigned_abs() as usize);
                } else if delta > 0 {
                    dialog.scroll_line = dialog.scroll_line.saturating_add(delta as usize);
                }
                result.dirty = true;
            }
            HelpDialogAction::Dismiss => {
                self.overlay = None;
                result.close = true;
                result.dirty = true;
            }
        }
        result
    }

    pub fn open_menu_command(&mut self, command: ReaderMenuCommand) -> bool {
        match command {
            ReaderMenuCommand::Contents => {
                if let Some(book) = &self.current_book {
                    if !book.toc.is_empty() {
        self.toc_selected = find_toc_selection(book, self.current_page);
        self.toc_top_row = self.toc_selected.saturating_sub(2);
        self.toc_cancel_focused = false;
        self.toc_labels = None;
                        return true;
                    }
                }
                false
            }
            ReaderMenuCommand::Page => {
                self.overlay = Some(ReaderOverlay::PageJump {
                    target_page: self.current_page,
                });
                self.overlay_form.reset();
                true
            }
            ReaderMenuCommand::Stats => {
                self.overlay = Some(ReaderOverlay::Stats);
                self.overlay_form.reset();
                true
            }
            ReaderMenuCommand::Help => {
                self.overlay = Some(ReaderOverlay::Help(RuntimeHelpDialog {
                    help_id: 1,
                    text: reader_help_text(),
                    scroll_line: 0,
                }));
                self.overlay_form.reset();
                true
            }
        }
    }

    pub fn has_overlay(&self) -> bool {
        self.overlay.is_some()
    }

    pub fn help_dialog(&self) -> Option<RuntimeHelpDialog> {
        match self.overlay.as_ref() {
            Some(ReaderOverlay::Help(dialog)) => Some(dialog.clone()),
            _ => None,
        }
    }

    pub(crate) fn overlay_spec(&self) -> Option<ModalFormSpec> {
        let overlay = self.overlay.as_ref()?;
        let bounds = match overlay {
            ReaderOverlay::PageJump { .. } => Rect::new(72, BOOK_STATUS_H + 18, 336, 232),
            ReaderOverlay::Stats => Rect::new(22, BOOK_STATUS_H + 8, 438, 196),
            ReaderOverlay::Help(..) => return None,
        };
        let title = match overlay {
            ReaderOverlay::PageJump { .. } => "Page Number",
            ReaderOverlay::Stats => "Book Stats",
            ReaderOverlay::Help(..) => return None,
        };
        let body_x = bounds.x + 20;
        let body_y = bounds.y + 52;
        let btn_margin_right = 14;
        let btn_margin_bottom = 14;
        let btn_x = bounds.x + bounds.w - 80 - btn_margin_right;
        let btn_y = bounds.y + bounds.h - 32 - btn_margin_bottom;
        let done_w = palm_text_width("Done", 0, &[], 1).max(24);
        let done_h = palm_text_height(0, &[], 1).max(10);
        let btn = auto_button_layout_for_label(btn_x, btn_y, done_w, done_h, 58, 24, 10, 5);

        let mut widgets = Vec::new();
        let default_focus = match overlay {
            ReaderOverlay::PageJump { target_page } => {
                let page_count = self
                    .current_book
                    .as_ref()
                    .map(|b| b.page_count)
                    .unwrap_or(1);
                let digits = page_jump_digit_count(page_count);
                let display_value = page_jump_display_value(*target_page, page_count);
                let display_text = format!("{:0width$}", display_value, width = digits);
                widgets.push(ModalWidget::Label {
                    id: 0,
                    bounds: Rect::new(body_x, body_y, 200, 12),
                    text: "Goto page:".into(),
                    font_id: 0,
                });
                let digit_w = 30;
                let digit_h = 28;
                let rep_h = 18;
                let gap = 8;
                let total_w = digits as i32 * digit_w + (digits.saturating_sub(1) as i32 * gap);
                let digits_x = bounds.x + ((bounds.w - total_w) / 2).max(0);
                let top_y = body_y + 26;
                for (idx, ch) in display_text.chars().enumerate() {
                    let x = digits_x + idx as i32 * (digit_w + gap);
                    widgets.push(ModalWidget::Button {
                        id: READER_OVERLAY_DIGIT_UP_BASE + idx as u16,
                        bounds: Rect::new(x, top_y, digit_w, rep_h),
                        text: "^".into(),
                        font_id: 1,
                        style: 5,
                        no_frame: false,
                    });
                    widgets.push(ModalWidget::Field {
                        id: READER_OVERLAY_DIGIT_FIELD_BASE + idx as u16,
                        bounds: Rect::new(x, top_y + rep_h + 6, digit_w, digit_h),
                        text: ch.to_string(),
                        font_id: 1,
                        focusable: false,
                    });
                    widgets.push(ModalWidget::Button {
                        id: READER_OVERLAY_DIGIT_DOWN_BASE + idx as u16,
                        bounds: Rect::new(x, top_y + rep_h + 6 + digit_h + 6, digit_w, rep_h),
                        text: "v".into(),
                        font_id: 1,
                        style: 5,
                        no_frame: false,
                    });
                }
                widgets.push(ModalWidget::Label {
                    id: 0,
                    bounds: Rect::new(body_x, bounds.y + bounds.h - 74, 260, 12),
                    text: format!("1 - {}", page_count.max(1)),
                    font_id: 0,
                });
                let ok_w = 56;
                let cancel_w = 74;
                let buttons_y = bounds.y + bounds.h - 40;
                let cancel_x = bounds.x + bounds.w - cancel_w - 16;
                let ok_x = cancel_x - ok_w - 10;
                widgets.push(ModalWidget::Button {
                    id: READER_OVERLAY_BTN_OK,
                    bounds: Rect::new(ok_x, buttons_y, ok_w, 24),
                    text: "OK".into(),
                    font_id: 0,
                    style: 0,
                    no_frame: false,
                });
                widgets.push(ModalWidget::Button {
                    id: READER_OVERLAY_BTN_CANCEL,
                    bounds: Rect::new(cancel_x, buttons_y, cancel_w, 24),
                    text: "Cancel".into(),
                    font_id: 0,
                    style: 0,
                    no_frame: false,
                });
                READER_OVERLAY_DIGIT_UP_BASE + digits.saturating_sub(1) as u16
            }
            ReaderOverlay::Stats => {
                if let Some(book) = self.current_book.as_ref() {
                    let progress = if book.page_count > 0 {
                        ((self.current_page + 1) * 100) / book.page_count.max(1)
                    } else {
                        0
                    };
                    for (idx, line) in [
                        format!("Title: {}", book.metadata.title),
                        format!("Page: {}/{}", self.current_page + 1, book.page_count),
                        format!("Progress: {}%", progress),
                        format!("TOC entries: {}", book.toc.len()),
                    ]
                    .iter()
                    .enumerate()
                    {
                        widgets.push(ModalWidget::Label {
                            id: 0,
                            bounds: Rect::new(body_x, body_y + idx as i32 * 24, 380, 12),
                            text: line.clone(),
                            font_id: 0,
                        });
                    }
                }
                widgets.push(ModalWidget::Label {
                    id: 0,
                    bounds: Rect::new(body_x, bounds.y + bounds.h - 56, 220, 12),
                    text: "Confirm or Back to close".into(),
                    font_id: 0,
                });
                READER_OVERLAY_BTN_OK
            }
            ReaderOverlay::Help(..) => return None,
        };

        if matches!(overlay, ReaderOverlay::Stats) {
            widgets.push(ModalWidget::Button {
                id: READER_OVERLAY_BTN_OK,
                bounds: Rect::new(btn_x, btn_y, btn.w, btn.h),
                text: "Done".into(),
                font_id: 0,
                style: 0,
                no_frame: false,
            });
        }

        Some(ModalFormSpec {
            form_id: READER_OVERLAY_FORM_ID,
            bounds,
            title: title.into(),
            widgets,
            default_focus: Some(default_focus),
        })
    }

    pub fn handle_toc_input(
        &mut self,
        buttons: &input::ButtonState,
        fonts: &[crate::palm::runtime::PalmFont],
    ) -> TocResult {
        let mut result = TocResult {
            exit: false,
            jumped: false,
            dirty: false,
        };

        let Some(book) = self.current_book.as_ref().cloned() else {
            result.exit = true;
            result.dirty = true;
            return result;
        };

        self.ensure_toc_labels(book.as_ref());
        let toc_len = book.toc.len();
        let visible_rows = self.toc_visible_rows(fonts);

        if buttons.is_pressed(input::Buttons::Up) {
            if self.toc_cancel_focused {
                self.toc_cancel_focused = false;
                result.dirty = true;
                return result;
            }
            if self.toc_selected > 0 {
                self.toc_selected -= 1;
                if self.toc_selected < self.toc_top_row {
                    self.toc_top_row = self.toc_selected;
                }
                result.dirty = true;
            }
            return result;
        }
        if buttons.is_pressed(input::Buttons::Down) {
            if self.toc_cancel_focused {
                return result;
            }
            if self.toc_selected + 1 < toc_len {
                self.toc_selected += 1;
                let bottom = self.toc_top_row.saturating_add(visible_rows.saturating_sub(1));
                if self.toc_selected > bottom {
                    self.toc_top_row = self
                        .toc_selected
                        .saturating_sub(visible_rows.saturating_sub(1));
                }
                result.dirty = true;
            } else {
                self.toc_cancel_focused = true;
                result.dirty = true;
            }
            return result;
        }
        if buttons.is_pressed(input::Buttons::Left) || buttons.is_pressed(input::Buttons::Right) {
            self.toc_cancel_focused = !self.toc_cancel_focused;
            result.dirty = true;
            return result;
        }
        if buttons.is_pressed(input::Buttons::Confirm) {
            if self.toc_cancel_focused {
                result.exit = true;
                result.dirty = true;
                return result;
            }
            if let Some(entry) = book.toc.get(self.toc_selected) {
                self.current_page = entry.page_index as usize;
                self.current_page_ops = None;
                self.next_page_ops = None;
                self.last_rendered_page = None;
                self.book_turns_since_full = 0;
                result.jumped = true;
                result.dirty = true;
            }
            return result;
        }
        if buttons.is_pressed(input::Buttons::Back) {
            result.exit = true;
            result.dirty = true;
            return result;
        }

        result
    }

    pub fn toc_hit_test(
        &mut self,
        point: crate::ternos::ui::Point,
        fonts: &[crate::palm::runtime::PalmFont],
    ) -> Option<TocTouchHit> {
        let book = self.current_book.as_ref()?.clone();
        self.ensure_toc_labels(book.as_ref());

        let modal = toc_modal_rect();
        let cancel_rect = toc_cancel_rect(modal);
        let table_rect = toc_table_rect(modal, cancel_rect);
        let scrollbar_rect = toc_scrollbar_rect(modal, table_rect);
        if cancel_rect.contains(point) {
            return Some(TocTouchHit::Cancel);
        }

        let model = self.toc_table_model(table_rect.w, fonts);
        let table = TableView::new(&model);
        if let Some(TableHit::Cell { row, .. }) = table.hit_test(table_rect, point) {
            return Some(TocTouchHit::Row(row));
        }

        let visible_rows = table.visible_row_count(table_rect);
        let labels = self.toc_labels.as_ref().map(Vec::as_slice).unwrap_or(&[]);
        if labels.len() > visible_rows {
            let scrollbar = TableScrollBarView::new(self.toc_top_row, visible_rows, labels.len());
            return match scrollbar.hit_test(scrollbar_rect, point) {
                Some(TableScrollBarHit::ArrowUp) => Some(TocTouchHit::ScrollUp),
                Some(TableScrollBarHit::ArrowDown) => Some(TocTouchHit::ScrollDown),
                Some(TableScrollBarHit::Track { top_row }) => Some(TocTouchHit::ScrollTrack(top_row)),
                None => None,
            };
        }

        None
    }

    pub fn handle_toc_touch_press(
        &mut self,
        hit: TocTouchHit,
        fonts: &[crate::palm::runtime::PalmFont],
    ) -> TocResult {
        let mut result = TocResult {
            exit: false,
            jumped: false,
            dirty: false,
        };

        let Some(book) = self.current_book.as_ref().cloned() else {
            result.exit = true;
            result.dirty = true;
            return result;
        };
        self.ensure_toc_labels(book.as_ref());
        let toc_len = book.toc.len();
        let visible_rows = self.toc_visible_rows(fonts);

        match hit {
            TocTouchHit::Row(row) => {
                let row = row.min(toc_len.saturating_sub(1));
                let changed = self.toc_selected != row || self.toc_cancel_focused;
                self.toc_selected = row;
                self.toc_cancel_focused = false;
                result.dirty = changed;
            }
            TocTouchHit::Cancel => {
                let changed = !self.toc_cancel_focused;
                self.toc_cancel_focused = true;
                result.dirty = changed;
            }
            TocTouchHit::ScrollUp => {
                if self.toc_top_row > 0 {
                    self.toc_top_row -= 1;
                    self.clamp_toc_selection_to_view(toc_len, visible_rows);
                    result.dirty = true;
                }
            }
            TocTouchHit::ScrollDown => {
                let max_top = toc_len.saturating_sub(visible_rows);
                if self.toc_top_row < max_top {
                    self.toc_top_row += 1;
                    self.clamp_toc_selection_to_view(toc_len, visible_rows);
                    result.dirty = true;
                }
            }
            TocTouchHit::ScrollTrack(top_row) => {
                let max_top = toc_len.saturating_sub(visible_rows);
                let new_top = top_row.min(max_top);
                if self.toc_top_row != new_top {
                    self.toc_top_row = new_top;
                    self.clamp_toc_selection_to_view(toc_len, visible_rows);
                    result.dirty = true;
                }
            }
        }

        result
    }

    pub fn handle_toc_touch_release(
        &mut self,
        hit: TocTouchHit,
        pressed: Option<TocTouchHit>,
    ) -> TocResult {
        let mut result = TocResult {
            exit: false,
            jumped: false,
            dirty: false,
        };

        let Some(book) = self.current_book.as_ref().cloned() else {
            result.exit = true;
            result.dirty = true;
            return result;
        };
        self.ensure_toc_labels(book.as_ref());

        match hit {
            TocTouchHit::Row(row) => {
                let row = row.min(book.toc.len().saturating_sub(1));
                let changed = self.toc_selected != row || self.toc_cancel_focused;
                self.toc_selected = row;
                self.toc_cancel_focused = false;
                if changed {
                    result.dirty = true;
                }
                if pressed == Some(TocTouchHit::Row(row)) {
                    if let Some(entry) = book.toc.get(row) {
                        self.current_page = entry.page_index as usize;
                        self.current_page_ops = None;
                        self.next_page_ops = None;
                        self.last_rendered_page = None;
                        self.book_turns_since_full = 0;
                        result.jumped = true;
                        result.dirty = true;
                    }
                }
            }
            TocTouchHit::Cancel => {
                let changed = !self.toc_cancel_focused;
                self.toc_cancel_focused = true;
                if changed {
                    result.dirty = true;
                }
                if pressed == Some(TocTouchHit::Cancel) {
                    result.exit = true;
                    result.dirty = true;
                }
            }
            TocTouchHit::ScrollUp | TocTouchHit::ScrollDown | TocTouchHit::ScrollTrack(_) => {}
        }

        result
    }

    pub fn draw_toc<S: AppSource>(
        &mut self,
        ctx: &mut BookReaderContext<'_, S>,
        display: &mut impl Display,
        home_focused: bool,
        menu_focused: bool,
    ) -> Result<(), ImageError> {
        self.render_book_frame(ctx, home_focused, menu_focused)?;
        let Some(book) = self.current_book.as_ref().cloned() else {
            return Err(ImageError::Decode);
        };
        self.ensure_toc_labels(book.as_ref());
        let label_count = self.toc_labels.as_ref().map(Vec::len).unwrap_or(0);
        let modal = toc_modal_rect();
        let cancel_rect = toc_cancel_rect(modal);
        let table_rect = toc_table_rect(modal, cancel_rect);
        let scrollbar_rect = toc_scrollbar_rect(modal, table_rect);
        let model = self.toc_table_model(table_rect.w, ctx.palm_fonts);
        let visible_rows = TableView::new(&model).visible_row_count(table_rect);
        let renderer = TocTableRenderer {
            palm_fonts: ctx.palm_fonts,
        };
        let mut ui = UiContext {
            buffers: ctx.display_buffers,
            render_policy: ctx.render_policy,
            gray2: None,
        };
        let mut table = TableView::new(&model);
        table.clear = false;
        table.renderer = Some(&renderer);
        draw_alert_frame_hi(ui.buffers, modal.x, modal.y, modal.w, modal.h, 34);
        let title_w = palm_text_width("Table of Contents", 1, ctx.palm_fonts, 1);
        let title_h = palm_text_height(1, ctx.palm_fonts, 1);
        crate::ternos::ui::draw_palm_text(
            ui.buffers,
            "Table of Contents",
            modal.x + ((modal.w - title_w) / 2).max(0),
            modal.y + ((34 - title_h) / 2).max(2) - 1,
            1,
            ctx.palm_fonts,
            1,
            BinaryColor::On,
        );
        table.render(&mut ui, table_rect, &mut RenderQueue::default());
        if label_count > visible_rows {
            let mut scrollbar =
                TableScrollBarView::new(self.toc_top_row, visible_rows, label_count);
            scrollbar.render(&mut ui, scrollbar_rect, &mut RenderQueue::default());
        }
        draw_form_button_hi(
            ui.buffers,
            ctx.palm_fonts,
            cancel_rect.x,
            cancel_rect.y,
            cancel_rect.w,
            cancel_rect.h,
            0,
            0,
            false,
            "Cancel",
            self.toc_cancel_focused,
        );
        let mut rq = RenderQueue::default();
        let refresh = if *ctx.full_refresh {
            ctx.render_policy.refresh_mode(*ctx.full_refresh)
        } else {
            crate::display::RefreshMode::Fast
        };
        rq.push(modal, refresh);
        flush_queue(display, ctx.display_buffers, &mut rq, refresh);
        Ok(())
    }

    pub fn draw_book<S: AppSource>(
        &mut self,
        ctx: &mut BookReaderContext<'_, S>,
        display: &mut impl Display,
        home_focused: bool,
        menu_focused: bool,
    ) -> Result<(), ImageError> {
        self.render_book_frame(ctx, home_focused, menu_focused)?;
        let Some(book) = &self.current_book else {
            return Err(ImageError::Decode);
        };
        let mode = ctx.render_policy.refresh_mode(*ctx.full_refresh);
        let mut gray2_used = false;
        let mut gray2_absolute = false;
        if self.current_page_ops.is_some() {
            // Detect grayscale state from prepared planes.
            gray2_used = ctx.gray2_lsb.iter().any(|b| *b != 0) || ctx.gray2_msb.iter().any(|b| *b != 0);
            gray2_absolute = false;
        }
        self.last_rendered_page = Some(self.current_page);
        if self.book_turns_since_full >= BOOK_FULL_REFRESH_EVERY {
            *ctx.full_refresh = true;
            self.book_turns_since_full = 0;
        }
        let mode = ctx.render_policy.refresh_mode(*ctx.full_refresh);
        if gray2_used {
            display.display(ctx.display_buffers, mode);
            let lsb_buf: &[u8; BUFFER_SIZE] = ctx.gray2_lsb.as_ref().try_into().unwrap();
            let msb_buf: &[u8; BUFFER_SIZE] = ctx.gray2_msb.as_ref().try_into().unwrap();
            display.copy_grayscale_buffers(lsb_buf, msb_buf);
            if gray2_absolute {
                display.display_absolute_grayscale(ctx.render_policy.absolute_grayscale_mode);
            } else {
                display.display_differential_grayscale(false);
            }
        } else {
            let mut rq = RenderQueue::default();
            let size = ctx.display_buffers.size();
            rq.push(Rect::new(0, 0, size.width as i32, size.height as i32), mode);
            flush_queue(display, ctx.display_buffers, &mut rq, mode);
        }

        if self.next_page_ops.is_none() {
            let next = self.current_page + 1;
            if next < book.page_count {
                self.next_page_ops = ctx.source.trbk_page(next).ok();
            }
        }
        Ok(())
    }

    pub fn render_book_frame<S: AppSource>(
        &mut self,
        ctx: &mut BookReaderContext<'_, S>,
        home_focused: bool,
        menu_focused: bool,
    ) -> Result<(), ImageError> {
        let Some(book) = self.current_book.clone() else {
            return Err(ImageError::Decode);
        };
        let book_page_count = book.page_count;
        let mut gray2_used = false;
        let mut gray2_absolute = false;
        ctx.display_buffers.clear(BinaryColor::On).ok();
        draw_reader_status_bar(
            ctx,
            Some((self.current_page, book_page_count)),
            home_focused,
            menu_focused,
        );
        ctx.gray2_lsb.fill(0);
        ctx.gray2_msb.fill(0);
        if self.current_page_ops.is_none() {
            self.current_page_ops = ctx.source.trbk_page(self.current_page).ok();
        }
        let page = self.current_page_ops.clone();
        if let Some(page) = page.as_ref() {
            self.render_trbk_page_ops(ctx, book.as_ref(), page, &mut gray2_used, &mut gray2_absolute);
        }
        Ok(())
    }

    fn render_trbk_page_ops<S: AppSource>(
        &mut self,
        ctx: &mut BookReaderContext<'_, S>,
        book: &crate::trbk::TrbkBookInfo,
        page: &crate::trbk::TrbkPage,
        gray2_used: &mut bool,
        gray2_absolute: &mut bool,
    ) {
        let content_origin_y = reader_page_origin_y(book);
        for op in &page.ops {
            match op {
                crate::trbk::TrbkOp::TextRun { x, y, style, text } => {
                    let gray2_lsb = &mut *ctx.gray2_lsb;
                    let gray2_msb = &mut *ctx.gray2_msb;
                    let mut gray2_ctx = Some((gray2_lsb, gray2_msb, &mut *gray2_used));
                    draw_trbk_text(
                        ctx.display_buffers,
                        book,
                        &mut gray2_ctx,
                        *x,
                        *y + content_origin_y,
                        *style,
                        text,
                    );
                }
                crate::trbk::TrbkOp::Image {
                    x,
                    y,
                    width,
                    height,
                    image_index,
                } => {
                    let op_w = *width as u32;
                    let op_h = *height as u32;
                    match ctx.source.trbk_image(*image_index as usize) {
                        Ok(image) => {
                            match &image {
                                ImageData::Gray2Stream { width, height, key } => {
                                    let size = ctx.display_buffers.size();
                                    if *x == 0
                                        && *y == 0
                                        && op_w == size.width
                                        && op_h == size.height
                                        && *width == op_w
                                        && *height == op_h
                                    {
                                        let rotation = ctx.display_buffers.rotation();
                                        let base_buf = ctx.display_buffers.get_active_buffer_mut();
                                        base_buf.fill(0xFF);
                                        if ctx
                                            .source
                                            .load_gray2_stream(
                                                key,
                                                *width,
                                                *height,
                                                rotation,
                                                base_buf,
                                                &mut *ctx.gray2_lsb,
                                                &mut *ctx.gray2_msb,
                                            )
                                            .is_ok()
                                        {
                                            *gray2_used = true;
                                            *gray2_absolute = true;
                                        } else {
                                            log::warn!(
                                                "Gray2 stream load failed for image {} ({}x{})",
                                                image_index,
                                                width,
                                                height
                                            );
                                        }
                                    } else if *width == op_w && *height == op_h {
                                        let rotation = ctx.display_buffers.rotation();
                                        let base_buf = ctx.display_buffers.get_active_buffer_mut();
                                        if ctx
                                            .source
                                            .load_gray2_stream_region(
                                                key,
                                                *width,
                                                *height,
                                                rotation,
                                                base_buf,
                                                &mut *ctx.gray2_lsb,
                                                &mut *ctx.gray2_msb,
                                                *x,
                                                *y,
                                            )
                                            .is_ok()
                                        {
                                            *gray2_used = true;
                                        } else {
                                            log::warn!(
                                                "Gray2 stream region load failed for image {} ({}x{})",
                                                image_index,
                                                width,
                                                height
                                            );
                                        }
                                    } else {
                                        log::warn!(
                                            "Gray2 stream skipped (non-fullscreen) image {} at ({}, {}) size {}x{}",
                                            image_index,
                                            x,
                                            y,
                                            width,
                                            height
                                        );
                                    }
                                }
                                _ => {
                                    let gray2_lsb = &mut *ctx.gray2_lsb;
                                    let gray2_msb = &mut *ctx.gray2_msb;
                                    let mut gray2_ctx =
                                        Some((gray2_lsb, gray2_msb, &mut *gray2_used));
                                    draw_trbk_image(
                                        ctx.display_buffers,
                                        &image,
                                        &mut gray2_ctx,
                                        ctx.render_policy,
                                        *x,
                                        *y + content_origin_y,
                                        *width as i32,
                                        *height as i32,
                                    );
                                }
                            }
                        }
                        Err(err) => {
                            log::warn!(
                                "Failed to load TRBK image {} ({}x{}): {:?}",
                                image_index,
                                width,
                                height,
                                err
                            );
                        }
                    }
                }
            }
        }
    }

}

pub fn reader_menu_bar() -> MenuBarPreview {
    MenuBarPreview {
        resource_id: 1,
        menus: alloc::vec![
            MenuPullDownPreview {
                resource_id: READER_MENU_NAVIGATE,
                title: "Navigate".into(),
                items: alloc::vec![
                    MenuItemPreview {
                        id: READER_CMD_CONTENTS,
                        text: "Contents".into(),
                        shortcut: None,
                    },
                    MenuItemPreview {
                        id: READER_CMD_PAGE,
                        text: "Page".into(),
                        shortcut: None,
                    },
                ],
            },
            MenuPullDownPreview {
                resource_id: READER_MENU_VIEW,
                title: "View".into(),
                items: alloc::vec![
                    MenuItemPreview {
                        id: READER_CMD_STATS,
                        text: "Stats".into(),
                        shortcut: None,
                    },
                    MenuItemPreview {
                        id: READER_CMD_HELP,
                        text: "Help".into(),
                        shortcut: None,
                    },
                ],
            },
        ],
    }
}

pub fn reader_menu_command(item_id: u16) -> Option<ReaderMenuCommand> {
    match item_id {
        READER_CMD_CONTENTS => Some(ReaderMenuCommand::Contents),
        READER_CMD_PAGE => Some(ReaderMenuCommand::Page),
        READER_CMD_STATS => Some(ReaderMenuCommand::Stats),
        READER_CMD_HELP => Some(ReaderMenuCommand::Help),
        _ => None,
    }
}

fn draw_trbk_text(
    buffers: &mut DisplayBuffers,
    book: &crate::trbk::TrbkBookInfo,
    gray2: &mut Option<(&mut [u8], &mut [u8], &mut bool)>,
    x: i32,
    y: i32,
    style: u8,
    text: &str,
) {
    if book.glyphs.is_empty() {
        let fallback = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        Text::new(text, Point::new(x, y), fallback)
            .draw(buffers)
            .ok();
        return;
    }

    let mut pen_x = x;
    let baseline = y;
    for ch in text.chars() {
        if ch == '\r' || ch == '\n' {
            continue;
        }
        let codepoint = ch as u32;
        if let Some(glyph) = find_glyph(book.glyphs.as_slice(), style, codepoint) {
            draw_glyph(buffers, glyph, gray2, pen_x, baseline);
            pen_x += glyph.x_advance as i32;
        } else {
            pen_x += book.metadata.char_width as i32;
        }
    }
}

pub(crate) fn draw_trbk_image(
    buffers: &mut DisplayBuffers,
    image: &ImageData,
    gray2: &mut Option<(&mut [u8], &mut [u8], &mut bool)>,
    render_policy: RenderPolicy,
    x: i32,
    y: i32,
    target_w: i32,
    target_h: i32,
) {
    match image {
        ImageData::Mono1 {
            width,
            height,
            bits,
        } => {
            let src_w = *width as i32;
            let src_h = *height as i32;
            let dst_w = target_w.max(1);
            let dst_h = target_h.max(1);
            for ty in 0..dst_h {
                let src_y = (ty as i64 * src_h as i64 / dst_h as i64) as i32;
                for tx in 0..dst_w {
                    let src_x = (tx as i64 * src_w as i64 / dst_w as i64) as i32;
                    if src_x < 0 || src_y < 0 {
                        continue;
                    }
                    let idx = (src_y as usize) * (*width as usize) + src_x as usize;
                    let byte = idx / 8;
                    if byte >= bits.len() {
                        continue;
                    }
                    let bit = 7 - (idx % 8);
                    let white = (bits[byte] >> bit) & 0x01 == 1;
                    buffers.set_pixel(
                        x + tx,
                        y + ty,
                        if white {
                            BinaryColor::On
                        } else {
                            BinaryColor::Off
                        },
                    );
                }
            }
        }
        ImageData::Gray8 {
            width,
            height,
            pixels,
        } => {
            let src_w = *width as i32;
            let src_h = *height as i32;
            let dst_w = target_w.max(1);
            let dst_h = target_h.max(1);
            for ty in 0..dst_h {
                let src_y = (ty as i64 * src_h as i64 / dst_h as i64) as i32;
                for tx in 0..dst_w {
                    let src_x = (tx as i64 * src_w as i64 / dst_w as i64) as i32;
                    let idx = (src_y as usize) * (*width as usize) + src_x as usize;
                    if idx >= pixels.len() {
                        continue;
                    }
                    let lum = pixels[idx];
                    let color = if render_policy.binary_color_for_luma(tx, ty, lum) {
                        BinaryColor::On
                    } else {
                        BinaryColor::Off
                    };
                    buffers.set_pixel(x + tx, y + ty, color);
                }
            }
        }
        ImageData::Gray2 {
            width,
            height,
            data,
        } => {
            let plane = ((*width as usize * *height as usize) + 7) / 8;
            if data.len() < plane * 3 {
                return;
            }
            let base = &data[..plane];
            let lsb = &data[plane..plane * 2];
            let msb = &data[plane * 2..plane * 3];
            let Some((gray2_lsb, gray2_msb, gray2_used)) = gray2.as_mut() else {
                return;
            };
            if gray2_lsb.len() < BUFFER_SIZE || gray2_msb.len() < BUFFER_SIZE {
                return;
            }
            **gray2_used = true;
            let src_w = *width as i32;
            let src_h = *height as i32;
            let dst_w = target_w.max(1);
            let dst_h = target_h.max(1);
            for ty in 0..dst_h {
                let src_y = (ty as i64 * src_h as i64 / dst_h as i64) as i32;
                for tx in 0..dst_w {
                    let src_x = (tx as i64 * src_w as i64 / dst_w as i64) as i32;
                    if src_x < 0 || src_y < 0 {
                        continue;
                    }
                    let idx = (src_y as usize) * (*width as usize) + src_x as usize;
                    let byte = idx / 8;
                    if byte >= base.len() || byte >= lsb.len() || byte >= msb.len() {
                        continue;
                    }
                    let bit = 7 - (idx % 8);
                    let base_white = (base[byte] >> bit) & 0x01 == 1;
                    buffers.set_pixel(
                        x + tx,
                        y + ty,
                        if base_white {
                            BinaryColor::On
                        } else {
                            BinaryColor::Off
                        },
                    );
                    let dst_x = x + tx;
                    let dst_y = y + ty;
                    let Some((fx, fy)) =
                        map_display_point(buffers.rotation(), dst_x, dst_y)
                    else {
                        continue;
                    };
                    let dst_idx = fy * FB_WIDTH + fx;
                    let dst_byte = dst_idx / 8;
                    let dst_bit = 7 - (dst_idx % 8);
                    if (lsb[byte] >> bit) & 0x01 == 1 {
                        gray2_lsb[dst_byte] |= 1 << dst_bit;
                    }
                    if (msb[byte] >> bit) & 0x01 == 1 {
                        gray2_msb[dst_byte] |= 1 << dst_bit;
                    }
                }
            }
        }
        ImageData::Gray2Stream { .. } => {}
    }
}

fn draw_reader_status_bar<S: AppSource>(
    ctx: &mut BookReaderContext<'_, S>,
    page_info: Option<(usize, usize)>,
    home_focused: bool,
    menu_focused: bool,
) {
    let size = ctx.display_buffers.size();
    let mut shell_ui = UiContext {
        buffers: ctx.display_buffers,
        render_policy: ctx.render_policy,
        gray2: None,
    };
    let mut status_bar = StatusBarView::new(ctx.palm_fonts);
    status_bar.battery_percent = ctx.battery_percent;
    status_bar.home = StatusBarActionState {
        enabled: true,
        focused: home_focused,
    };
    status_bar.menu = StatusBarActionState {
        enabled: true,
        focused: menu_focused,
    };
    let page_label = page_info.map(|(page, total)| format!("{}/{}", page.saturating_add(1), total));
    status_bar.right_text = page_label.as_deref();
    status_bar.render(
        &mut shell_ui,
        Rect::new(0, 0, size.width as i32, BOOK_STATUS_H),
        &mut RenderQueue::default(),
    );
}

pub fn draw_reader_menu_overlay(
    buffers: &mut DisplayBuffers,
    fonts: &[crate::palm::runtime::PalmFont],
    overlay: (&MenuBarPreview, usize, Option<usize>),
) -> Rect {
    let (menu, active_menu_index, active_item_index) = overlay;
    draw_menu_overlay_native(
        buffers,
        menu,
        active_menu_index,
        active_item_index,
        fonts,
        0,
        BOOK_STATUS_H + 5,
        buffers.size().width as i32,
    )
}

pub fn draw_reader_overlay<S: AppSource>(
    state: &mut BookReaderState,
    ctx: &mut BookReaderContext<'_, S>,
    display: &mut impl Display,
    help_focus: Option<crate::palm::ui::HelpOverlayHit>,
) {
    if let Some(dialog) = state.help_dialog() {
        let rect = draw_help_dialog_native(
            ctx.display_buffers,
            &dialog,
            ctx.palm_fonts,
            help_focus,
            crate::ternos::ui::Rect::new(18, BOOK_STATUS_H + 8, 448, 236),
        );
        let mut rq = RenderQueue::default();
        rq.push(rect, crate::display::RefreshMode::Half);
        flush_queue(display, ctx.display_buffers, &mut rq, crate::display::RefreshMode::Half);
        return;
    }
    let Some(spec) = state.overlay_spec() else {
        return;
    };
    state.overlay_form.sync(&spec);
    let mut form_view = ModalFormView {
        spec: &spec,
        fonts: ctx.palm_fonts,
        focused_id: state.overlay_form.focused_id(),
    };
    let mut ui = UiContext {
        buffers: ctx.display_buffers,
        render_policy: ctx.render_policy,
        gray2: None,
    };
    form_view.render(&mut ui, spec.bounds, &mut RenderQueue::default());

    let mut rq = RenderQueue::default();
    rq.push(
        Rect::new(
            spec.bounds.x - 1,
            spec.bounds.y - 1,
            spec.bounds.w + 2,
            spec.bounds.h + 3,
        ),
        crate::display::RefreshMode::Half,
    );
    flush_queue(display, ctx.display_buffers, &mut rq, crate::display::RefreshMode::Half);
}

fn map_display_point(rotation: Rotation, x: i32, y: i32) -> Option<(usize, usize)> {
    if x < 0 || y < 0 {
        return None;
    }
    let xu = x as usize;
    let yu = y as usize;
    let (x, y) = match rotation {
        Rotation::Rotate0 => (xu, yu),
        Rotation::Rotate90 => {
            if xu >= FB_HEIGHT {
                return None;
            }
            (yu, FB_HEIGHT - 1 - xu)
        }
        Rotation::Rotate180 => {
            if xu >= FB_WIDTH || yu >= FB_HEIGHT {
                return None;
            }
            (FB_WIDTH - 1 - xu, FB_HEIGHT - 1 - yu)
        }
        Rotation::Rotate270 => {
            if yu >= FB_WIDTH {
                return None;
            }
            (FB_WIDTH - 1 - yu, xu)
        }
    };
    if x >= FB_WIDTH || y >= FB_HEIGHT {
        None
    } else {
        Some((x, y))
    }
}

fn find_glyph<'a>(
    glyphs: &'a [crate::trbk::TrbkGlyph],
    style: u8,
    codepoint: u32,
) -> Option<&'a crate::trbk::TrbkGlyph> {
    glyphs
        .iter()
        .find(|glyph| glyph.style == style && glyph.codepoint == codepoint)
}

pub fn find_toc_selection(book: &crate::trbk::TrbkBookInfo, page: usize) -> usize {
    let mut selected = 0usize;
    for (idx, entry) in book.toc.iter().enumerate() {
        if (entry.page_index as usize) <= page {
            selected = idx;
        } else {
            break;
        }
    }
    selected
}

fn draw_glyph(
    buffers: &mut DisplayBuffers,
    glyph: &crate::trbk::TrbkGlyph,
    gray2: &mut Option<(&mut [u8], &mut [u8], &mut bool)>,
    origin_x: i32,
    baseline: i32,
) {
    let width = glyph.width as i32;
    let height = glyph.height as i32;
    if width == 0 || height == 0 {
        return;
    }
    let start_x = origin_x + glyph.x_offset as i32;
    let start_y = baseline - glyph.y_offset as i32;
    let rotation = buffers.rotation();
    let mut idx = 0usize;
    let has_gray2 = glyph.bitmap_lsb.is_some() && glyph.bitmap_msb.is_some();
    for row in 0..height {
        for col in 0..width {
            let byte = idx / 8;
            let bit = 7 - (idx % 8);
            if byte < glyph.bitmap_bw.len() {
                let bw_set = (glyph.bitmap_bw[byte] & (1 << bit)) != 0;
                let draw_black = if has_gray2 { !bw_set } else { bw_set };
                if draw_black {
                    buffers.set_pixel(start_x + col, start_y + row, BinaryColor::Off);
                }
            }
            if let (Some(lsb), Some(msb)) =
                (glyph.bitmap_lsb.as_ref(), glyph.bitmap_msb.as_ref())
            {
                if let Some((gray2_lsb, gray2_msb, gray2_used)) = gray2.as_mut() {
                    if gray2_lsb.len() < BUFFER_SIZE || gray2_msb.len() < BUFFER_SIZE {
                        continue;
                    }
                    **gray2_used = true;
                    if byte < lsb.len() && (lsb[byte] & (1 << bit)) != 0 {
                        if let Some((fx, fy)) =
                            map_display_point(rotation, start_x + col, start_y + row)
                        {
                            let dst_idx = fy * FB_WIDTH + fx;
                            let dst_byte = dst_idx / 8;
                            let dst_bit = 7 - (dst_idx % 8);
                            gray2_lsb[dst_byte] |= 1 << dst_bit;
                        }
                    }
                    if byte < msb.len() && (msb[byte] & (1 << bit)) != 0 {
                        if let Some((fx, fy)) =
                            map_display_point(rotation, start_x + col, start_y + row)
                        {
                            let dst_idx = fy * FB_WIDTH + fx;
                            let dst_byte = dst_idx / 8;
                            let dst_bit = 7 - (dst_idx % 8);
                            gray2_msb[dst_byte] |= 1 << dst_bit;
                        }
                    }
                }
            }
            idx += 1;
        }
    }
}
