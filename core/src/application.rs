extern crate alloc;

use alloc::{format, string::{String, ToString}};
use alloc::vec::Vec;

use embedded_graphics::{
    Drawable,
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, OriginDimensions, Point, Primitive, Size},
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};

mod generated_icons {
    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

fn is_trbk(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".trbk") || lower.ends_with(".tbk")
}

fn is_epub(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with(".epub") || name.ends_with(".epb")
}

fn is_prc(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".prc") || lower.ends_with(".tdb")
}

use crate::{
    app::{
        book_reader::{
            draw_reader_menu_overlay, draw_reader_overlay, draw_trbk_image, reader_menu_bar,
            reader_menu_command, BookReaderContext, BookReaderState, PageTurnIndicator,
            ReaderMenuCommand,
        },
        home::{
            HomeAction,
            HomeIcons,
            HomeRenderContext,
            HomeState,
        },
        reader_shell::{self, ReaderStatusBarFocus},
        image_viewer::{ImageViewerContext, ImageViewerState},
        settings::{
            about_modal_spec, about_ok_id, home_menu_bar, home_menu_command,
            record_detail_delete_id, record_detail_modal_spec, record_detail_ok_id,
            records_table_id,
            records_modal_spec, HomeMenuCommand,
        },
        system::{ApplyResumeOutcome, ResumeContext, SleepWallpaperIcons, SystemRenderContext, SystemState},
    },
    build_info,
    display::RefreshMode,
    framebuffer::{DisplayBuffers, Rotation},
    image_viewer::{AppSource, EntryKind, ImageEntry, ImageError, InstalledDatabaseEntry},
    input,
    platform::PlatformInputEvent,
    palm,
    render_policy::RenderPolicy,
    ternos::ui::{flush_queue, handle_status_bar_button, preferred_status_bar_focus, ModalFormController, ModalFormSpec, ModalFormView, ModalHit, ModalWidget, ObjectId, Rect, RenderQueue, StatusBarActionState, StatusBarButtons, StatusBarHit, StatusBarView, UiContext, View},
};
use crate::palm::shell::PrcStatusBarFocus;

const LIST_MARGIN_X: i32 = 16;
const HEADER_Y: i32 = 24;
const PAGE_INDICATOR_MARGIN: i32 = 12;
const PAGE_INDICATOR_Y: i32 = 24;
pub struct Application<'a, S: AppSource> {
    dirty: bool,
    display_buffers: &'a mut DisplayBuffers,
    source: &'a mut S,
    home: HomeState,
    state: AppState,
    image_viewer: ImageViewerState,
    book_reader: BookReaderState,
    system: SystemState,
    current_entry: Option<String>,
    last_viewed_entry: Option<String>,
    error_message: Option<String>,
    prc_lines: Vec<String>,
    prc_scroll: usize,
    prc_form_index: usize,
    prc_forms: Vec<palm::form_preview::FormPreview>,
    prc_bitmaps: Vec<palm::bitmap::PrcBitmap>,
    prc_runtime_form_id: Option<u16>,
    prc_runtime_underlay_form_id: Option<u16>,
    prc_ui_controller: palm::controller::PrcUiController,
    prc_runtime_bitmap_draws: Vec<palm::runner::RuntimeBitmapDraw>,
    prc_runtime_button_labels: Vec<palm::runner::RuntimeButtonLabel>,
    prc_runtime_selected_controls: Vec<palm::runner::RuntimeSelectedControl>,
    prc_runtime_field_draws: Vec<palm::runner::RuntimeFieldDraw>,
    prc_runtime_table_draws: Vec<palm::runner::RuntimeTableDraw>,
    prc_runtime_focused_field_id: Option<u16>,
    prc_system_fonts: Vec<palm::runtime::PalmFont>,
    home_system_fonts: Vec<palm::runtime::PalmFont>,
    prc_menu_controller: palm::controller::PrcMenuController,
    reader_menu_controller: palm::controller::PrcMenuController,
    home_menu_controller: palm::controller::PrcMenuController,
    home_about_form: ModalFormController,
    home_records_form: ModalFormController,
    home_record_detail_form: ModalFormController,
    prc_help_controller: palm::controller::PrcHelpDialogController,
    reader_help_controller: palm::controller::PrcHelpDialogController,
    prc_active_entry: Option<ImageEntry>,
    prc_session: Option<palm::runner::PrcRuntimeSession>,
    prc_blocked_timeout_ticks: u32,
    prc_blocked_elapsed_ms: u32,
    prc_soft_menu_focused: bool,
    prc_soft_menu_last_control: Option<u16>,
    prc_status_bar_focus: Option<PrcStatusBarFocus>,
    prc_touch_pressed_status: Option<PrcStatusBarFocus>,
    prc_status_bar_last_control: Option<u16>,
    reader_status_bar_focus: Option<ReaderStatusBarFocus>,
    reader_touch_pressed_status: Option<ReaderStatusBarFocus>,
    records_status_bar_focus: Option<StatusBarHit>,
    records_touch_pressed_status: Option<StatusBarHit>,
    reader_touch_pressed_menu: Option<palm::ui::MenuOverlayHit>,
    home_touch_pressed_menu: Option<palm::ui::MenuOverlayHit>,
    home_touch_pressed_about: Option<ObjectId>,
    home_touch_pressed_records: Option<ModalHit>,
    home_touch_pressed_record_detail: Option<ObjectId>,
    reader_touch_pressed_overlay: Option<crate::ternos::ui::ObjectId>,
    reader_touch_pressed_toc: Option<ModalHit>,
    home_menu_last_rect: Option<Rect>,
    reader_menu_last_rect: Option<Rect>,
    prc_return_to_start_menu: bool,
    prc_reserved_gray_initialized: bool,
    install_scan_elapsed_ms: u32,
    install_last_summary: Option<(u32, u32, u32, u32, u32)>,
    gray2_lsb: Vec<u8>,
    gray2_msb: Vec<u8>,
    render_policy: RenderPolicy,
    exit_from: ExitFrom,
    exit_overlay_drawn: bool,
    home_about_open: bool,
    home_records_open: bool,
    home_records: Vec<InstalledDatabaseEntry>,
    home_records_selected_row: usize,
    home_records_top_row: usize,
    home_record_detail_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AppState {
    StartMenu,
    Records,
    Viewing,
    BookViewing,
    ExitingPending,
    Toc,
    PrcViewing,
    SleepingPending,
    Sleeping,
    Error,
}

#[derive(Clone, Copy, Debug)]
enum ExitFrom {
    Image,
    Book,
}

impl<'a, S: AppSource> Application<'a, S> {
    const PALM_KEY_LEFT: u16 = 0x001C;
    const PALM_KEY_RIGHT: u16 = 0x001D;
    const PALM_KEY_UP: u16 = 0x001E;
    const PALM_KEY_DOWN: u16 = 0x001F;
    const PALM_KEY_RETURN: u16 = 0x000A;

    fn best_prc_form_index(&self) -> Option<usize> {
        self.prc_forms.iter().enumerate().max_by_key(|(_, f)| {
            let area = (f.w.max(0) as i32) * (f.h.max(0) as i32);
            let objs = f.objects.len() as i32;
            area.saturating_mul(4).saturating_add(objs.saturating_mul(100))
        }).map(|(idx, _)| idx)
    }

    fn runtime_prc_form(&self) -> Option<palm::form_preview::FormPreview> {
        let fid = self.prc_runtime_form_id?;
        self.prc_forms.iter().find(|f| f.form_id == fid).cloned()
    }

    fn prc_table_object(
        &self,
        form: &palm::form_preview::FormPreview,
        control_id: u16,
    ) -> Option<(u16, i16, i16, i16, i16)> {
        form.objects.iter().find_map(|obj| match obj {
            palm::form_preview::FormPreviewObject::Table { id, x, y, w, h }
                if *id == control_id =>
            {
                Some((*id, *x, *y, *w, *h))
            }
            _ => None,
        })
    }

    fn prc_table_draw(
        &self,
        form_id: u16,
        table_id: u16,
    ) -> Option<&palm::runner::RuntimeTableDraw> {
        self.prc_runtime_table_draws
            .iter()
            .find(|t| t.form_id == form_id && t.table_id == table_id)
    }

    fn prc_table_first_selectable_row(
        &self,
        table: &palm::runner::RuntimeTableDraw,
    ) -> Option<usize> {
        (0..table.rows.max(table.row_usable.len() as u16) as usize).find(|row| {
            table.row_usable.get(*row).copied().unwrap_or(true)
                && table.row_selectable.get(*row).copied().unwrap_or(true)
        })
    }

    fn prc_table_next_row(
        &self,
        table: &palm::runner::RuntimeTableDraw,
        current: Option<usize>,
        delta: i32,
    ) -> Option<usize> {
        let rows = table.rows.max(table.row_usable.len() as u16) as usize;
        if rows == 0 {
            return None;
        }
        let start = current.or_else(|| self.prc_table_first_selectable_row(table))?;
        let mut row = start as i32;
        loop {
            row += delta;
            if row < 0 || row >= rows as i32 {
                return None;
            }
            let row_u = row as usize;
            if table.row_usable.get(row_u).copied().unwrap_or(true)
                && table.row_selectable.get(row_u).copied().unwrap_or(true)
            {
                return Some(row_u);
            }
        }
    }

    fn prc_table_pen_point(
        &self,
        table: &palm::runner::RuntimeTableDraw,
        table_x: i16,
        table_y: i16,
        table_w: i16,
        table_h: i16,
        row: usize,
    ) -> Option<(u16, u16)> {
        let visible_rows: Vec<usize> = (0..table.rows.max(table.row_usable.len() as u16) as usize)
            .filter(|r| table.row_usable.get(*r).copied().unwrap_or(true))
            .collect();
        let vr = visible_rows.iter().position(|r| *r == row)?;
        let inner_h = (table_h as i32 - 2).max(1);
        let row_hints = &table.row_height;
        let row_heights: Vec<i32> = visible_rows
            .iter()
            .map(|idx| row_hints.get(*idx).copied().unwrap_or(11).max(1) as i32)
            .collect();
        let natural_h = row_heights.iter().sum::<i32>().max(1);
        let mut y_cursor = table_y as i32 + 1;
        for h in row_heights.iter().take(vr) {
            y_cursor += ((*h * inner_h) / natural_h).max(1);
        }
        let row_px = ((row_heights[vr] * inner_h) / natural_h).max(1);
        let y = (y_cursor + row_px / 2).clamp(0, u16::MAX as i32) as u16;
        let x = (table_x as i32 + (table_w as i32 / 4).max(2)).clamp(0, u16::MAX as i32) as u16;
        Some((x, y))
    }

    fn prc_sync_table_selection(
        &mut self,
        form_id: u16,
        table_id: u16,
        row: i16,
        col: i16,
    ) {
        if let Some(table) = self
            .prc_runtime_table_draws
            .iter_mut()
            .find(|t| t.form_id == form_id && t.table_id == table_id)
        {
            table.selected_row = row;
            table.selected_col = col;
        }
        if let Some(session) = self.prc_session.as_mut() {
            let _ = session.set_table_selection(form_id, table_id, row, col);
        }
    }

    fn nav_button_from_buttons(buttons: &input::ButtonState) -> Option<crate::platform::ButtonId> {
        use crate::input::Buttons;
        if buttons.is_pressed(Buttons::Left) {
            Some(crate::platform::ButtonId::Left)
        } else if buttons.is_pressed(Buttons::Right) {
            Some(crate::platform::ButtonId::Right)
        } else if buttons.is_pressed(Buttons::Up) {
            Some(crate::platform::ButtonId::Up)
        } else if buttons.is_pressed(Buttons::Down) {
            Some(crate::platform::ButtonId::Down)
        } else if buttons.is_pressed(Buttons::Confirm) {
            Some(crate::platform::ButtonId::Confirm)
        } else if buttons.is_pressed(Buttons::Back) {
            Some(crate::platform::ButtonId::Back)
        } else {
            None
        }
    }

    fn reader_status_hit(&self) -> Option<StatusBarHit> {
        reader_shell::to_status_hit(self.reader_status_bar_focus)
    }

    fn prc_status_hit(&self) -> Option<StatusBarHit> {
        palm::shell::to_status_hit(self.prc_status_bar_focus)
    }

    fn records_status_hit(&self) -> Option<StatusBarHit> {
        self.records_status_bar_focus
    }

    fn set_reader_status_from_hit(&mut self, hit: Option<StatusBarHit>) {
        self.reader_status_bar_focus = reader_shell::from_status_hit(hit);
    }

    fn set_prc_status_from_hit(&mut self, hit: Option<StatusBarHit>) {
        self.prc_status_bar_focus = palm::shell::from_status_hit(hit);
    }

    fn set_records_status_from_hit(&mut self, hit: Option<StatusBarHit>) {
        self.records_status_bar_focus = hit;
    }

    fn handle_reader_status_bar_button(&mut self, button: crate::platform::ButtonId) -> bool {
        let result = handle_status_bar_button(
            self.reader_status_hit(),
            StatusBarButtons {
                home_enabled: true,
                menu_enabled: true,
            },
            button,
        );
        if !result.consumed {
            return false;
        }
        self.set_reader_status_from_hit(result.focus);
        if let Some(hit) = result.activated {
            match hit {
                StatusBarHit::Home => {
                    self.exit_from = ExitFrom::Book;
                    self.exit_overlay_drawn = false;
                    self.state = AppState::ExitingPending;
                    self.dirty = true;
                }
                StatusBarHit::Menu => {
                    if self.reader_menu_controller.open() {
                        self.dirty = true;
                    }
                }
            }
        } else {
            self.dirty = true;
        }
        true
    }

    fn records_handle_touch_event(&mut self, event: &PlatformInputEvent) -> bool {
        if !matches!(
            event,
            PlatformInputEvent::TouchDown { .. } | PlatformInputEvent::TouchUp { .. }
        ) {
            return false;
        }
        let (x, y, is_down, is_up) = match *event {
            PlatformInputEvent::TouchDown { x, y } => (x, y, true, false),
            PlatformInputEvent::TouchUp { x, y } => (x, y, false, true),
            _ => return false,
        };
        let point = crate::ternos::ui::Point::new(x, y);

        if let Some(hit) =
            reader_shell::status_bar_hit(self.display_buffers.size().width as i32, point)
        {
            match hit {
                StatusBarHit::Home => {
                    if is_down {
                        self.records_status_bar_focus = Some(StatusBarHit::Home);
                        self.records_touch_pressed_status = Some(StatusBarHit::Home);
                        self.dirty = true;
                    } else if is_up && self.records_touch_pressed_status == Some(StatusBarHit::Home) {
                        self.records_touch_pressed_status = None;
                        self.close_home_records();
                    }
                    return true;
                }
                StatusBarHit::Menu => {
                    if is_up {
                        self.records_touch_pressed_status = None;
                    }
                    return true;
                }
            }
        }

        if let Some(spec) = self.home_record_detail_spec() {
            self.home_record_detail_form.sync(&spec);
            if let Some(hit) = self
                .home_record_detail_form
                .hit_test(&spec, point, self.home_system_fonts.as_slice())
            {
                let ModalHit::Widget(id) = hit else {
                    return true;
                };
                let changed = self.home_record_detail_form.select_id(&spec, id);
                if is_down {
                    self.home_touch_pressed_record_detail = Some(id);
                    if changed {
                        self.dirty = true;
                    }
                } else if is_up {
                    let pressed = self.home_touch_pressed_record_detail.take();
                    if pressed == Some(id) {
                        let action = self.home_record_detail_form.activate_id(&spec, id);
                        self.apply_home_record_detail_action(action);
                    } else if changed {
                        self.dirty = true;
                    }
                }
                return true;
            }
            if is_up {
                self.home_touch_pressed_record_detail = None;
            }
            return true;
        }

        let spec = self.home_records_spec();
        self.home_records_form.sync(&spec);
        if let Some(hit) = self
            .home_records_form
            .hit_test(&spec, point, self.home_system_fonts.as_slice())
        {
            let changed = self
                .home_records_form
                .select_hit(&spec, hit, self.home_system_fonts.as_slice());
            if is_down {
                self.home_touch_pressed_records = Some(hit);
                if changed {
                    self.dirty = true;
                }
            } else if is_up {
                let pressed = self.home_touch_pressed_records.take();
                if pressed == Some(hit) {
                    let action = self.home_records_form.activate_hit(
                        &spec,
                        hit,
                        self.home_system_fonts.as_slice(),
                    );
                    self.apply_home_records_action(action);
                } else if changed {
                    self.dirty = true;
                }
            }
            return true;
        }
        if is_up {
            self.home_touch_pressed_records = None;
        }

        false
    }

    fn handle_prc_status_bar_button(&mut self, button: crate::platform::ButtonId) -> bool {
        let result = handle_status_bar_button(
            self.prc_status_hit(),
            StatusBarButtons {
                home_enabled: true,
                menu_enabled: self.prc_menu_controller.menu_count() > 0,
            },
            button,
        );
        if !result.consumed {
            return false;
        }
        self.set_prc_status_from_hit(result.focus);
        self.prc_touch_pressed_status = None;
        if let Some(hit) = result.activated {
            match hit {
                StatusBarHit::Home => self.exit_prc_viewer_to_origin(),
                StatusBarHit::Menu => {
                    if self.prc_menu_controller.open() {
                        self.dirty = true;
                    }
                }
            }
        } else {
            self.dirty = true;
        }
        true
    }

    fn handle_records_status_bar_button(&mut self, button: crate::platform::ButtonId) -> bool {
        let result = handle_status_bar_button(
            self.records_status_hit(),
            StatusBarButtons {
                home_enabled: true,
                menu_enabled: false,
            },
            button,
        );
        if !result.consumed {
            return false;
        }
        self.set_records_status_from_hit(result.focus);
        self.records_touch_pressed_status = None;
        if let Some(StatusBarHit::Home) = result.activated {
            self.set_state_start_menu(true);
        } else if button == crate::platform::ButtonId::Down && result.focus.is_none() {
            let spec = self.home_records_spec();
            self.home_records_form.sync(&spec);
            if let Some(id) = spec.default_focus {
                self.home_records_form.select_id(&spec, id);
            }
            self.dirty = true;
        } else {
            self.dirty = true;
        }
        true
    }

    fn handle_reader_help_buttons(
        &mut self,
        buttons: &input::ButtonState,
        elapsed_ms: u32,
    ) -> bool {
        let Some(dialog) = self.book_reader.help_dialog() else {
            return false;
        };
        self.reader_help_controller.sync(&dialog);
        if let Some(event) = reader_shell::nav_event_from_buttons(buttons) {
            let result = self.book_reader.apply_help_action(
                self.reader_help_controller
                    .on_event(&dialog, self.home_system_fonts.as_slice(), event),
            );
            if result.dirty {
                self.dirty = true;
            }
        } else if self.system.add_idle(elapsed_ms) {
            self.start_sleep_request();
        }
        true
    }

    fn handle_reader_menu_buttons(
        &mut self,
        buttons: &input::ButtonState,
        elapsed_ms: u32,
    ) -> bool {
        if !self.reader_menu_controller.is_active() {
            return false;
        }
        if let Some(event) = reader_shell::nav_event_from_buttons(buttons) {
            match self.reader_menu_controller.on_event(event) {
                palm::controller::MenuAction::Activate(item_id) => {
                    self.apply_reader_menu_command(item_id);
                }
                palm::controller::MenuAction::Redraw | palm::controller::MenuAction::Closed => {
                    self.reader_touch_pressed_menu = None;
                    self.dirty = true;
                }
                palm::controller::MenuAction::None => {}
            }
        } else if self.system.add_idle(elapsed_ms) {
            self.start_sleep_request();
        }
        true
    }

    fn handle_home_menu_buttons(
        &mut self,
        buttons: &input::ButtonState,
        elapsed_ms: u32,
    ) -> bool {
        if !self.home_menu_controller.is_active() {
            return false;
        }
        if let Some(event) = reader_shell::nav_event_from_buttons(buttons) {
            match self.home_menu_controller.on_event(event) {
                palm::controller::MenuAction::Activate(item_id) => {
                    self.apply_home_menu_command(item_id);
                }
                palm::controller::MenuAction::Redraw | palm::controller::MenuAction::Closed => {
                    self.home_touch_pressed_menu = None;
                    self.dirty = true;
                }
                palm::controller::MenuAction::None => {}
            }
        } else if self.system.add_idle(elapsed_ms) {
            self.start_sleep_request();
        }
        true
    }

    fn handle_reader_status_bar_buttons(
        &mut self,
        buttons: &input::ButtonState,
        elapsed_ms: u32,
    ) -> bool {
        if self.reader_status_bar_focus.is_none() {
            return false;
        }
        if let Some(button) = Self::nav_button_from_buttons(buttons) {
            let _ = self.handle_reader_status_bar_button(button);
        } else if self.system.add_idle(elapsed_ms) {
            self.start_sleep_request();
        }
        true
    }

    fn handle_reader_shell_buttons(
        &mut self,
        buttons: &input::ButtonState,
        elapsed_ms: u32,
        toc: bool,
    ) {
        if self.handle_reader_help_buttons(buttons, elapsed_ms) {
            return;
        }
        if self.book_reader.has_overlay() {
            let result = self
                .book_reader
                .handle_overlay_input(buttons, self.home_system_fonts.as_slice());
            if result.jumped {
                self.set_state_book_viewing();
            } else if result.dirty {
                self.dirty = true;
            } else if self.system.add_idle(elapsed_ms) {
                self.start_sleep_request();
            }
            return;
        }
        if self.handle_reader_menu_buttons(buttons, elapsed_ms) {
            return;
        }
        if self.handle_reader_status_bar_buttons(buttons, elapsed_ms) {
            return;
        }
        if !toc && buttons.is_pressed(input::Buttons::Confirm) {
            self.set_reader_status_from_hit(preferred_status_bar_focus(StatusBarButtons {
                home_enabled: true,
                menu_enabled: true,
            }));
            self.dirty = true;
            return;
        }
        if toc {
            let result = self
                .book_reader
                .handle_toc_input(buttons, self.home_system_fonts.as_slice());
            if result.exit || result.jumped {
                self.set_state_book_viewing();
            } else if result.dirty {
                self.dirty = true;
            } else if self.system.add_idle(elapsed_ms) {
                self.start_sleep_request();
            }
        } else {
            let result = self.book_reader.handle_view_input(self.source, buttons);
            if result.exit {
                self.exit_from = ExitFrom::Book;
                self.exit_overlay_drawn = false;
                self.state = AppState::ExitingPending;
                self.dirty = true;
            } else if result.open_toc {
                self.set_state_toc();
            } else if result.dirty {
                self.dirty = true;
            } else if self.system.add_idle(elapsed_ms) {
                self.start_sleep_request();
            }
        }
    }

    fn prc_pane_layout(&self) -> Option<(palm::form_preview::FormPreview, i32, i32, i32)> {
        const PRC_STATUS_H: i32 = 34;
        let form = self
            .runtime_prc_form()
            .or_else(|| self.prc_forms.get(self.prc_form_index).cloned())
            .or_else(|| self.prc_forms.first().cloned())?;
        let size = self.display_buffers.size();
        let content_top = PRC_STATUS_H + 2;
        let content_h = (size.height as i32 - content_top).max(1);
        let max_scale_w = ((size.width as i32) / 160).max(1);
        let max_scale_h = (content_h / 160).max(1);
        let max_scale = max_scale_w.min(max_scale_h).max(1);
        let scale = if max_scale >= 3 { 3 } else { max_scale };
        let pane_w = 160 * scale;
        let pane_x = ((size.width as i32 - pane_w) / 2).max(0);
        let pane_y = content_top;
        Some((form, pane_x, pane_y, scale.max(1)))
    }

    fn prc_handle_touch_event(&mut self, event: &PlatformInputEvent) -> bool {
        const PRC_STATUS_H: i32 = 34;
        let (x, y, is_down, is_up) = match *event {
            PlatformInputEvent::TouchDown { x, y } => (x, y, true, false),
            PlatformInputEvent::TouchUp { x, y } => (x, y, false, true),
            _ => return false,
        };
        if self.prc_session.is_none() {
            return false;
        }

        let Some((form, pane_x, pane_y, scale)) = self.prc_pane_layout() else {
            return false;
        };

        if let Some(shell_hit) = palm::shell::status_bar_hit(
            self.display_buffers.size().width as i32,
            self.prc_menu_controller.menu_count() > 0,
            crate::ternos::ui::Point::new(x, y),
        ) {
            self.prc_status_bar_focus = Some(shell_hit);
            if is_down {
                self.prc_touch_pressed_status = Some(shell_hit);
                self.dirty = true;
                return true;
            }
            if is_up && self.prc_touch_pressed_status == Some(shell_hit) {
                self.prc_touch_pressed_status = None;
                match shell_hit {
                    PrcStatusBarFocus::Home => {
                        self.exit_prc_viewer_to_origin();
                    }
                    PrcStatusBarFocus::Menu => {
                        if self.prc_menu_controller.open() {
                            self.dirty = true;
                        }
                    }
                }
                return true;
            }
        } else if is_up {
            self.prc_touch_pressed_status = None;
        }

        let local_x = x - pane_x;
        let local_y = y - pane_y;
        let palm_x = local_x.div_euclid(scale.max(1));
        let palm_y = local_y.div_euclid(scale.max(1));

        if let Some(help) = self
            .prc_session
            .as_ref()
            .and_then(|session| session.help_dialog())
        {
            if let Some(hit) = palm::ui::hit_test_help_overlay(
                &help,
                self.prc_system_fonts.as_slice(),
                crate::ternos::ui::Point::new(palm_x, palm_y),
            ) {
                match hit {
                    palm::ui::HelpOverlayHit::Done => {
                        self.prc_help_controller.focus_control(palm::ui::HelpOverlayHit::Done);
                        if is_up {
                            if let Some(session) = self.prc_session.as_mut() {
                                let _ = session.dismiss_help_dialog();
                                self.prc_help_controller.clear();
                                self.resume_prc_runtime_session();
                            }
                        } else {
                            self.dirty = true;
                        }
                        return true;
                    }
                    palm::ui::HelpOverlayHit::ScrollUp => {
                        self.prc_help_controller
                            .focus_control(palm::ui::HelpOverlayHit::ScrollUp);
                        if is_down
                            && let Some(session) = self.prc_session.as_mut()
                            && session.scroll_help_dialog(-self.prc_help_controller.scroll_step_lines)
                        {
                            self.dirty = true;
                        }
                        return true;
                    }
                    palm::ui::HelpOverlayHit::ScrollDown => {
                        self.prc_help_controller
                            .focus_control(palm::ui::HelpOverlayHit::ScrollDown);
                        if is_down
                            && let Some(session) = self.prc_session.as_mut()
                            && session.scroll_help_dialog(self.prc_help_controller.scroll_step_lines)
                        {
                            self.dirty = true;
                        }
                        return true;
                    }
                }
            }
            return false;
        }

        if self.prc_menu_controller.is_active() {
            if let Some((menu, active_menu_index, _)) = self.prc_menu_controller.overlay() {
                match palm::ui::hit_test_menu_overlay(
                    menu,
                    active_menu_index,
                    self.prc_system_fonts.as_slice(),
                    crate::ternos::ui::Point::new(palm_x, palm_y),
                ) {
                    Some(palm::ui::MenuOverlayHit::Title(menu_index)) => {
                        if self.prc_menu_controller.select_menu(menu_index) {
                            self.dirty = true;
                        }
                        return true;
                    }
                    Some(palm::ui::MenuOverlayHit::Item {
                        menu_index,
                        item_index,
                    }) => {
                        let mut changed = self.prc_menu_controller.select_menu(menu_index);
                        changed |= self.prc_menu_controller.select_item(item_index);
                        if is_up {
                            match self
                                .prc_menu_controller
                                .on_event(palm::ui_component::UiNavEvent::Confirm)
                            {
                                palm::controller::MenuAction::Activate(item_id) => {
                                    if let Some(session) = self.prc_session.as_mut() {
                                        session.inject_event_now(
                                            palm::runtime::EVT_MENU,
                                            item_id,
                                            "menuSelect",
                                        );
                                        self.prc_blocked_elapsed_ms = 0;
                                        self.prc_blocked_timeout_ticks = 0;
                                        self.resume_prc_runtime_session();
                                    }
                                    self.dirty = true;
                                }
                                palm::controller::MenuAction::Redraw
                                | palm::controller::MenuAction::Closed => {
                                    self.dirty = true;
                                }
                                palm::controller::MenuAction::None => {}
                            }
                        } else if changed {
                            self.dirty = true;
                        }
                        return true;
                    }
                    None => {
                        if is_down {
                            self.prc_menu_controller.close();
                            self.dirty = true;
                            return true;
                        }
                    }
                }
            }
            return false;
        }

        let content_top = PRC_STATUS_H + 2;
        let pane_h = 160 * scale;
        let size = self.display_buffers.size();
        let strip_top = (content_top + pane_h).clamp(0, size.height as i32);
        let strip_h = (size.height as i32 - strip_top).max(0);
        if strip_h > 0 {
            let soft_rect = self.prc_soft_menu_button_rect(strip_top, strip_h);
            if is_down
                && x >= soft_rect.x
                && y >= soft_rect.y
                && x < soft_rect.x + soft_rect.w
                && y < soft_rect.y + soft_rect.h
            {
                if self.prc_menu_controller.open() {
                    self.dirty = true;
                }
                return true;
            }
        }

        if local_x < 0 || local_y < 0 {
            return false;
        }

        if let Some(hit) = palm::ui::hit_test_form_preview(
            &form,
            crate::ternos::ui::Point::new(palm_x, palm_y),
        ) {
            let _ = self.prc_ui_controller.select_control_id(Some(&form), hit.id);
            if let Some(session) = self.prc_session.as_mut() {
                match hit.kind {
                    palm::ui::FormControlKind::Field => {
                        session.inject_event_now(
                            palm::runtime::EVT_FLD_ENTER,
                            hit.id,
                            "touchFldEnter",
                        );
                    }
                    palm::ui::FormControlKind::Table => {
                        if is_down {
                            session.inject_pen_down_now(palm_x as u16, palm_y as u16, "touchTblPenDown");
                        }
                    }
                    palm::ui::FormControlKind::Control => {
                        session.inject_control_select_now(hit.id);
                    }
                }
                self.prc_blocked_elapsed_ms = 0;
                self.prc_blocked_timeout_ticks = 0;
                self.resume_prc_runtime_session();
            }
            if is_down {
                self.dirty = true;
                return true;
            }
        }

        false
    }

    fn start_menu_handle_touch_event(
        &mut self,
        event: &PlatformInputEvent,
        recents: &[String],
    ) -> bool {
        if !matches!(
            event,
            PlatformInputEvent::TouchDown { .. } | PlatformInputEvent::TouchUp { .. }
        ) {
            return false;
        }
        let (x, y, is_down, is_up) = match *event {
            PlatformInputEvent::TouchDown { x, y } => (x, y, true, false),
            PlatformInputEvent::TouchUp { x, y } => (x, y, false, true),
            _ => return false,
        };
        let point = crate::ternos::ui::Point::new(x, y);

        if let Some(spec) = self.home_record_detail_spec() {
            self.home_record_detail_form.sync(&spec);
            if let Some(hit) = self
                .home_record_detail_form
                .hit_test(&spec, point, self.home_system_fonts.as_slice())
            {
                let ModalHit::Widget(id) = hit else {
                    return true;
                };
                let changed = self.home_record_detail_form.select_id(&spec, id);
                if is_down {
                    self.home_touch_pressed_record_detail = Some(id);
                    if changed {
                        self.dirty = true;
                    }
                } else if is_up {
                    let pressed = self.home_touch_pressed_record_detail.take();
                    if pressed == Some(id) {
                        let action = self.home_record_detail_form.activate_id(&spec, id);
                        self.apply_home_record_detail_action(action);
                    } else if changed {
                        self.dirty = true;
                    }
                }
                return true;
            }
            if is_up {
                self.home_touch_pressed_record_detail = None;
            }
            return true;
        }

        if self.home_about_open {
            let spec = about_modal_spec(
                build_info::VERSION,
                build_info::BUILD_TIME,
                self.display_buffers.size().width as i32,
            );
            if let Some(hit) = self
                .home_about_form
                .hit_test(&spec, point, self.home_system_fonts.as_slice())
            {
                let ModalHit::Widget(id) = hit else {
                    return true;
                };
                let changed = self.home_about_form.select_id(&spec, id);
                if is_down {
                    self.home_touch_pressed_about = Some(id);
                    if changed {
                        self.dirty = true;
                    }
                } else if is_up {
                    let pressed = self.home_touch_pressed_about.take();
                    if pressed == Some(id) && id == about_ok_id() {
                        self.home_about_open = false;
                        self.home_about_form.reset();
                        self.dirty = true;
                    } else if changed {
                        self.dirty = true;
                    }
                }
                return true;
            }
            if is_up {
                self.home_touch_pressed_about = None;
            }
            return true;
        }

        if self.home_menu_controller.is_active() {
            if let Some((menu, active_menu_index, _)) = self.home_menu_controller.overlay() {
                let hit = palm::ui::hit_test_menu_overlay_native(
                    menu,
                    active_menu_index,
                    self.home_system_fonts.as_slice(),
                    0,
                    StatusBarView::HEIGHT + 5,
                    self.display_buffers.size().width as i32,
                    point,
                );
                match hit {
                    Some(palm::ui::MenuOverlayHit::Title(menu_index)) => {
                        if is_down {
                            self.home_touch_pressed_menu =
                                Some(palm::ui::MenuOverlayHit::Title(menu_index));
                            if self.home_menu_controller.select_menu(menu_index) {
                                self.dirty = true;
                            }
                        } else if is_up {
                            let _ = self.home_menu_controller.select_menu(menu_index);
                            self.home_touch_pressed_menu = None;
                            self.dirty = true;
                        }
                        return true;
                    }
                    Some(palm::ui::MenuOverlayHit::Item {
                        menu_index,
                        item_index,
                    }) => {
                        if is_down {
                            self.home_touch_pressed_menu = Some(palm::ui::MenuOverlayHit::Item {
                                menu_index,
                                item_index,
                            });
                            let mut changed = self.home_menu_controller.select_menu(menu_index);
                            changed |= self.home_menu_controller.select_item(item_index);
                            if changed {
                                self.dirty = true;
                            }
                        } else if is_up {
                            let mut changed = self.home_menu_controller.select_menu(menu_index);
                            changed |= self.home_menu_controller.select_item(item_index);
                            let pressed = self.home_touch_pressed_menu.take();
                            if pressed
                                == Some(palm::ui::MenuOverlayHit::Item {
                                    menu_index,
                                    item_index,
                                })
                            {
                                match self
                                    .home_menu_controller
                                    .on_event(palm::ui_component::UiNavEvent::Confirm)
                                {
                                    palm::controller::MenuAction::Activate(item_id) => {
                                        self.apply_home_menu_command(item_id);
                                    }
                                    palm::controller::MenuAction::Redraw
                                    | palm::controller::MenuAction::Closed => {
                                        self.dirty = true;
                                    }
                                    palm::controller::MenuAction::None => {
                                        if changed {
                                            self.dirty = true;
                                        }
                                    }
                                }
                            } else if changed {
                                self.dirty = true;
                            }
                        }
                        return true;
                    }
                    None => {
                        if is_down || is_up {
                            self.home_touch_pressed_menu = None;
                            self.home_menu_controller.close();
                            self.dirty = true;
                            return true;
                        }
                    }
                }
            }
            return false;
        }
        let size = self.display_buffers.size();
        match self
            .home
            .handle_start_menu_touch(recents, event, size.width as i32, size.height as i32)
        {
            HomeAction::OpenMenu => {
                if self.home_menu_controller.open() {
                    self.dirty = true;
                }
                true
            }
            HomeAction::OpenRecent(path) => {
                if path.to_ascii_lowercase().ends_with(".tdb")
                    || path.to_ascii_lowercase().ends_with(".prc")
                {
                    if let Err(err) = self.open_prc_path(&path) {
                        self.set_error(err);
                    }
                } else {
                    if let Err(err) = self.open_path(&path) {
                        if self.system.remove_recent(&path) {
                            if self.last_viewed_entry.as_deref() == Some(path.as_str()) {
                                self.last_viewed_entry = None;
                            }
                            self.system.save_recent_entries_now(self.source);
                        }
                        self.set_error(err);
                    }
                }
                true
            }
            HomeAction::None => {
                self.dirty = true;
                true
            }
        }
    }

    fn reader_handle_touch_event(&mut self, event: &PlatformInputEvent) -> bool {
        let (x, y, is_down, is_up) = match *event {
            PlatformInputEvent::TouchDown { x, y } => (x, y, true, false),
            PlatformInputEvent::TouchUp { x, y } => (x, y, false, true),
            _ => return false,
        };
        let point = crate::ternos::ui::Point::new(x, y);

        if let Some(dialog) = self.book_reader.help_dialog() {
            if let Some(hit) =
                reader_shell::help_overlay_hit(&dialog, self.home_system_fonts.as_slice(), point)
            {
                self.reader_help_controller.focus_control(hit);
                match hit {
                    palm::ui::HelpOverlayHit::Done => {
                        if is_up {
                            let result = self
                                .book_reader
                                .apply_help_action(palm::controller::HelpDialogAction::Dismiss);
                            if result.dirty {
                                self.dirty = true;
                            }
                        } else {
                            self.dirty = true;
                        }
                    }
                    palm::ui::HelpOverlayHit::ScrollUp => {
                        if is_down {
                            let result = self.book_reader.apply_help_action(
                                palm::controller::HelpDialogAction::Scroll(
                                    -self.reader_help_controller.scroll_step_lines,
                                ),
                            );
                            if result.dirty {
                                self.dirty = true;
                            }
                        } else {
                            self.dirty = true;
                        }
                    }
                    palm::ui::HelpOverlayHit::ScrollDown => {
                        if is_down {
                            let result = self.book_reader.apply_help_action(
                                palm::controller::HelpDialogAction::Scroll(
                                    self.reader_help_controller.scroll_step_lines,
                                ),
                            );
                            if result.dirty {
                                self.dirty = true;
                            }
                        } else {
                            self.dirty = true;
                        }
                    }
                }
                return true;
            }
            return false;
        }

        if self.book_reader.has_overlay()
            && let Some(spec) = self.book_reader.overlay_spec()
        {
            if let Some(hit) = self
                .book_reader
                .overlay_form
                .hit_test(&spec, point, self.home_system_fonts.as_slice())
            {
                let ModalHit::Widget(id) = hit else {
                    return true;
                };
                let changed = self.book_reader.overlay_form.select_id(&spec, id);
                if is_down {
                    self.reader_touch_pressed_overlay = Some(id);
                    if changed {
                        self.dirty = true;
                    }
                } else if is_up {
                    let pressed = self.reader_touch_pressed_overlay.take();
                    if pressed == Some(id) {
                        let action = self.book_reader.overlay_form.activate_id(&spec, id);
                        let result = self.book_reader.apply_overlay_action(action);
                        if result.jumped {
                            self.set_state_book_viewing();
                        } else if result.dirty || changed {
                            self.dirty = true;
                        }
                    } else if changed {
                        self.dirty = true;
                    }
                }
                return true;
            }
            if is_up {
                self.reader_touch_pressed_overlay = None;
            }
            return true;
        }

        if matches!(self.state, AppState::Toc) {
            if let Some(hit) = self.book_reader.toc_hit_test(point, self.home_system_fonts.as_slice()) {
                if is_down {
                    self.reader_touch_pressed_toc = Some(hit);
                    let result =
                        self.book_reader
                            .handle_toc_touch_press(hit, self.home_system_fonts.as_slice());
                    if result.exit || result.jumped {
                        self.set_state_book_viewing();
                    } else if result.dirty {
                        self.dirty = true;
                    }
                } else if is_up {
                    let pressed = self.reader_touch_pressed_toc.take();
                    let result = self.book_reader.handle_toc_touch_release(
                        hit,
                        pressed,
                        self.home_system_fonts.as_slice(),
                    );
                    if result.exit || result.jumped {
                        self.set_state_book_viewing();
                    } else if result.dirty {
                        self.dirty = true;
                    }
                }
                return true;
            }
            if is_up {
                self.reader_touch_pressed_toc = None;
            }
        }

        if self.reader_menu_controller.is_active() {
            if let Some((menu, active_menu_index, _)) = self.reader_menu_controller.overlay() {
                let hit = reader_shell::menu_overlay_hit(
                    menu,
                    active_menu_index,
                    self.home_system_fonts.as_slice(),
                    self.display_buffers.size().width as i32,
                    point,
                );
                match hit {
                    Some(palm::ui::MenuOverlayHit::Title(menu_index)) => {
                        if is_down {
                            self.reader_touch_pressed_menu =
                                Some(palm::ui::MenuOverlayHit::Title(menu_index));
                            if self.reader_menu_controller.select_menu(menu_index) {
                                self.dirty = true;
                            }
                        } else if is_up {
                            let _ = self.reader_menu_controller.select_menu(menu_index);
                            self.reader_touch_pressed_menu = None;
                            self.dirty = true;
                        }
                        return true;
                    }
                    Some(palm::ui::MenuOverlayHit::Item {
                        menu_index,
                        item_index,
                    }) => {
                        if is_down {
                            self.reader_touch_pressed_menu = Some(palm::ui::MenuOverlayHit::Item {
                                menu_index,
                                item_index,
                            });
                            let mut changed = self.reader_menu_controller.select_menu(menu_index);
                            changed |= self.reader_menu_controller.select_item(item_index);
                            if changed {
                                self.dirty = true;
                            }
                        } else if is_up {
                            let mut changed = self.reader_menu_controller.select_menu(menu_index);
                            changed |= self.reader_menu_controller.select_item(item_index);
                            let pressed = self.reader_touch_pressed_menu.take();
                            if pressed
                                == Some(palm::ui::MenuOverlayHit::Item {
                                    menu_index,
                                    item_index,
                                })
                            {
                                match self
                                    .reader_menu_controller
                                    .on_event(palm::ui_component::UiNavEvent::Confirm)
                                {
                                    palm::controller::MenuAction::Activate(item_id) => {
                                        self.apply_reader_menu_command(item_id);
                                    }
                                    palm::controller::MenuAction::Redraw
                                    | palm::controller::MenuAction::Closed => {
                                        self.dirty = true;
                                    }
                                    palm::controller::MenuAction::None => {
                                        if changed {
                                            self.dirty = true;
                                        }
                                    }
                                }
                            } else if changed {
                                self.dirty = true;
                            }
                        }
                        return true;
                    }
                    None => {
                        if is_down || is_up {
                            self.reader_touch_pressed_menu = None;
                            self.reader_menu_controller.close();
                            self.dirty = true;
                            return true;
                        }
                    }
                }
            }
            return false;
        }

        let Some(hit) = reader_shell::status_bar_hit(
            self.display_buffers.size().width as i32,
            point,
        ) else {
            if is_up {
                self.reader_touch_pressed_status = None;
            }
            return false;
        };
        match hit {
            StatusBarHit::Home => {
                if is_down {
                    self.reader_status_bar_focus = Some(ReaderStatusBarFocus::Home);
                    self.reader_touch_pressed_status = Some(ReaderStatusBarFocus::Home);
                    self.dirty = true;
                } else if is_up
                    && self.reader_touch_pressed_status == Some(ReaderStatusBarFocus::Home)
                {
                    self.reader_touch_pressed_status = None;
                    self.exit_from = ExitFrom::Book;
                    self.exit_overlay_drawn = false;
                    self.state = AppState::ExitingPending;
                    self.dirty = true;
                }
                true
            }
            StatusBarHit::Menu => {
                if is_down {
                    self.reader_status_bar_focus = Some(ReaderStatusBarFocus::Menu);
                    self.reader_touch_pressed_status = Some(ReaderStatusBarFocus::Menu);
                    self.dirty = true;
                } else if is_up
                    && self.reader_touch_pressed_status == Some(ReaderStatusBarFocus::Menu)
                {
                    self.reader_touch_pressed_status = None;
                    if self.reader_menu_controller.open() {
                        self.dirty = true;
                    }
                }
                true
            }
        }
    }

    fn apply_reader_menu_command(&mut self, item_id: u16) {
        let Some(command) = reader_menu_command(item_id) else {
            return;
        };
        self.reader_menu_controller.close();
        self.reader_touch_pressed_menu = None;
        match (self.state.clone(), command) {
            (AppState::BookViewing, ReaderMenuCommand::Contents) => {
                if self.book_reader.open_menu_command(ReaderMenuCommand::Contents) {
                    self.set_state_toc();
                } else {
                    self.dirty = true;
                }
            }
            (AppState::Toc, ReaderMenuCommand::Contents) => {
                self.dirty = true;
            }
            (_, ReaderMenuCommand::Page)
            | (_, ReaderMenuCommand::Stats)
            | (_, ReaderMenuCommand::Help) => {
                if command == ReaderMenuCommand::Help {
                    self.reader_help_controller.clear();
                }
                if self.book_reader.open_menu_command(command) {
                    self.dirty = true;
                }
            }
            _ => {}
        }
    }

    fn apply_home_menu_command(&mut self, item_id: u16) {
        let Some(command) = home_menu_command(item_id) else {
            return;
        };
        self.home_menu_controller.close();
        self.home_touch_pressed_menu = None;
        match command {
            HomeMenuCommand::Records => {
                self.home_about_open = false;
                self.home_about_form.reset();
                self.home_records = self.source.list_installed_databases();
                self.home_records_selected_row = self
                    .home_records_selected_row
                    .min(self.home_records.len().saturating_sub(1));
                self.home_records_top_row = self
                    .home_records_top_row
                    .min(self.home_records_selected_row);
                self.home_records_open = true;
                self.home_record_detail_index = None;
                self.home_records_form.reset();
                self.home_record_detail_form.reset();
                self.home_touch_pressed_records = None;
                self.home_touch_pressed_record_detail = None;
                self.records_status_bar_focus = None;
                self.records_touch_pressed_status = None;
                self.state = AppState::Records;
                self.dirty = true;
            }
            HomeMenuCommand::About => {
                self.home_records_open = false;
                self.home_record_detail_index = None;
                self.home_records_form.reset();
                self.home_record_detail_form.reset();
                self.home_about_open = true;
                self.home_about_form.reset();
                self.dirty = true;
            }
        }
    }

    fn home_records_spec(&self) -> ModalFormSpec {
        records_modal_spec(
            &self.home_records,
            self.home_records_selected_row,
            self.home_records_top_row,
            self.display_buffers.size().width as i32,
            self.display_buffers.size().height as i32,
        )
    }

    fn home_record_detail_spec(&self) -> Option<ModalFormSpec> {
        let entry = self.home_records.get(self.home_record_detail_index?)?;
        Some(record_detail_modal_spec(
            entry,
            self.display_buffers.size().width as i32,
            self.display_buffers.size().height as i32,
        ))
    }

    fn close_home_records(&mut self) {
        self.home_records_open = false;
        self.home_record_detail_index = None;
        self.home_records_form.reset();
        self.home_record_detail_form.reset();
        self.home_touch_pressed_records = None;
        self.home_touch_pressed_record_detail = None;
        self.records_status_bar_focus = None;
        self.records_touch_pressed_status = None;
        self.set_state_start_menu(true);
    }

    fn apply_home_records_action(&mut self, action: crate::ternos::ui::ModalFormAction) {
        match action {
            crate::ternos::ui::ModalFormAction::None => {}
            crate::ternos::ui::ModalFormAction::Redraw => self.dirty = true,
            crate::ternos::ui::ModalFormAction::Closed => self.close_home_records(),
            crate::ternos::ui::ModalFormAction::Activate(_) => {}
            crate::ternos::ui::ModalFormAction::TableChanged {
                selected_row,
                top_row,
                activated,
                ..
            } => {
                if let Some(row) = selected_row {
                    self.home_records_selected_row =
                        row.min(self.home_records.len().saturating_sub(1));
                }
                self.home_records_top_row = top_row;
                if activated {
                    self.home_record_detail_index = Some(self.home_records_selected_row);
                    self.home_record_detail_form.reset();
                }
                self.dirty = true;
            }
        }
    }

    fn apply_home_record_detail_action(&mut self, action: crate::ternos::ui::ModalFormAction) {
        match action {
            crate::ternos::ui::ModalFormAction::None => {}
            crate::ternos::ui::ModalFormAction::Redraw => self.dirty = true,
            crate::ternos::ui::ModalFormAction::Closed => {
                self.home_record_detail_index = None;
                self.home_record_detail_form.reset();
                self.home_touch_pressed_record_detail = None;
                self.dirty = true;
            }
            crate::ternos::ui::ModalFormAction::Activate(id) => {
                if id == record_detail_ok_id() {
                    self.home_record_detail_index = None;
                    self.home_record_detail_form.reset();
                    self.home_touch_pressed_record_detail = None;
                    self.dirty = true;
                } else if id == record_detail_delete_id()
                    && let Some(index) = self.home_record_detail_index
                    && let Some(entry) = self.home_records.get(index).cloned()
                    && self.source.delete_installed_database(&entry.path).is_ok()
                {
                    let _ = self.system.remove_recent(&entry.path);
                    self.home.installed_apps.clear();
                    self.home.start_menu_need_base_refresh = true;
                    self.home_records = self.source.list_installed_databases();
                    self.home_records_selected_row = self
                        .home_records_selected_row
                        .min(self.home_records.len().saturating_sub(1));
                    self.home_records_top_row =
                        self.home_records_top_row.min(self.home_records_selected_row);
                    self.home_record_detail_index = None;
                    self.home_record_detail_form.reset();
                    self.home_records_form.reset();
                    self.home_touch_pressed_record_detail = None;
                    self.dirty = true;
                }
            }
            crate::ternos::ui::ModalFormAction::TableChanged { .. } => {}
        }
    }

    fn prc_form_by_id(&self, fid: u16) -> Option<palm::form_preview::FormPreview> {
        self.prc_forms.iter().find(|f| f.form_id == fid).cloned()
    }

    fn prc_soft_menu_button_rect(&self, strip_top: i32, strip_h: i32) -> Rect {
        let size = self.display_buffers.size();
        let btn = 68i32;
        let pad = 14i32;
        let x = pad;
        let mut y = strip_top + strip_h - btn - pad;
        if y < strip_top + 4 {
            y = strip_top + 4;
        }
        let max_x = (size.width as i32 - btn - 4).max(0);
        Rect::new(x.min(max_x), y, btn, btn)
    }

    pub fn new(
        display_buffers: &'a mut DisplayBuffers,
        source: &'a mut S,
        display_caps: crate::platform::DisplayCaps,
    ) -> Self {
        display_buffers.set_rotation(Rotation::Rotate90);
        let render_policy = RenderPolicy::from_display_caps(display_caps);
        let resume_name = source.load_resume();
        let book_positions = source
            .load_book_positions()
            .into_iter()
            .collect();
        let recent_entries = source.load_recent_entries();
        let system = SystemState::new(resume_name, book_positions, recent_entries);
        let mut app = Application {
            dirty: true,
            display_buffers,
            source,
            home: HomeState::new(),
            state: AppState::StartMenu,
            image_viewer: ImageViewerState::new(),
            book_reader: BookReaderState::new(),
            system,
            current_entry: None,
            last_viewed_entry: None,
            error_message: None,
            prc_lines: Vec::new(),
            prc_scroll: 0,
            prc_form_index: 0,
            prc_forms: Vec::new(),
            prc_bitmaps: Vec::new(),
            prc_runtime_form_id: None,
            prc_runtime_underlay_form_id: None,
            prc_ui_controller: palm::controller::PrcUiController::default(),
            prc_runtime_bitmap_draws: Vec::new(),
            prc_runtime_button_labels: Vec::new(),
            prc_runtime_selected_controls: Vec::new(),
            prc_runtime_field_draws: Vec::new(),
            prc_runtime_table_draws: Vec::new(),
            prc_runtime_focused_field_id: None,
            prc_system_fonts: Vec::new(),
            home_system_fonts: Vec::new(),
            prc_menu_controller: palm::controller::PrcMenuController::default(),
            reader_menu_controller: {
                let mut controller = palm::controller::PrcMenuController::default();
                controller.set_menu_bar(Some(reader_menu_bar()));
                controller
            },
            home_menu_controller: {
                let mut controller = palm::controller::PrcMenuController::default();
                controller.set_menu_bar(Some(home_menu_bar()));
                controller
            },
            home_about_form: ModalFormController::default(),
            home_records_form: ModalFormController::default(),
            home_record_detail_form: ModalFormController::default(),
            prc_help_controller: palm::controller::PrcHelpDialogController::default(),
            reader_help_controller: palm::controller::PrcHelpDialogController::default(),
            prc_active_entry: None,
            prc_session: None,
            prc_blocked_timeout_ticks: 0,
            prc_blocked_elapsed_ms: 0,
            prc_soft_menu_focused: false,
            prc_soft_menu_last_control: None,
            prc_status_bar_focus: None,
            prc_touch_pressed_status: None,
            prc_status_bar_last_control: None,
            reader_status_bar_focus: None,
            reader_touch_pressed_status: None,
            records_status_bar_focus: None,
            records_touch_pressed_status: None,
            reader_touch_pressed_menu: None,
            home_touch_pressed_menu: None,
            home_touch_pressed_about: None,
            home_touch_pressed_records: None,
            home_touch_pressed_record_detail: None,
            reader_touch_pressed_overlay: None,
            reader_touch_pressed_toc: None,
            home_menu_last_rect: None,
            reader_menu_last_rect: None,
            prc_return_to_start_menu: false,
            prc_reserved_gray_initialized: false,
            install_scan_elapsed_ms: 0,
            install_last_summary: None,
            gray2_lsb: Vec::new(),
            gray2_msb: Vec::new(),
            render_policy,
            exit_from: ExitFrom::Image,
            exit_overlay_drawn: false,
            home_about_open: false,
            home_records_open: false,
            home_records: Vec::new(),
            home_records_selected_row: 0,
            home_records_top_row: 0,
            home_record_detail_index: None,
        };
        app.try_resume();
        app
    }

    fn ensure_gray2_buffers(&mut self) {
        if self.gray2_lsb.len() != crate::framebuffer::BUFFER_SIZE {
            self.gray2_lsb.resize(crate::framebuffer::BUFFER_SIZE, 0);
        }
        if self.gray2_msb.len() != crate::framebuffer::BUFFER_SIZE {
            self.gray2_msb.resize(crate::framebuffer::BUFFER_SIZE, 0);
        }
    }

    fn scan_palm_install_inbox(&mut self) {
        let Some(summary) = self.source.scan_palm_install_inbox() else {
            return;
        };
        let sig = (
            summary.scanned,
            summary.installed,
            summary.upgraded,
            summary.skipped,
            summary.failed,
        );
        if self.install_last_summary == Some(sig) {
            return;
        }
        self.install_last_summary = Some(sig);
        if summary.scanned == 0 && summary.failed == 0 {
            return;
        }
        log::info!(
            "Palm install inbox scanned={} installed={} upgraded={} skipped={} failed={}",
            summary.scanned,
            summary.installed,
            summary.upgraded,
            summary.skipped,
            summary.failed
        );
        if summary.installed > 0 || summary.upgraded > 0 || summary.failed > 0 {
            self.home.show_install_summary_dialog(summary);
            self.dirty = true;
        }
    }

    pub fn update(&mut self, buttons: &input::ButtonState, elapsed_ms: u32) {
        self.update_with_events(buttons, &[], elapsed_ms);
    }

    pub fn update_with_events(
        &mut self,
        buttons: &input::ButtonState,
        events: &[PlatformInputEvent],
        elapsed_ms: u32,
    ) {
        if !events.is_empty() {
            self.system.reset_idle();
        }

        if self.state == AppState::StartMenu {
            self.install_scan_elapsed_ms = self.install_scan_elapsed_ms.saturating_add(elapsed_ms);
            if self.install_scan_elapsed_ms >= 2000 {
                self.install_scan_elapsed_ms = 0;
                self.scan_palm_install_inbox();
            }
        }

        if self.state == AppState::Sleeping
            && (buttons.is_pressed(input::Buttons::Power)
                || buttons.is_held(input::Buttons::Power))
        {
            self.source.wake();
            if let Some(overlay) = self.system.sleep_overlay.take() {
                SystemState::restore_rect_bits(self.display_buffers, &overlay);
                if self.book_reader.current_book.is_some() {
                    self.set_state_book_viewing();
                    self.system.full_refresh = true;
                    self.system.wake_restore_only = false;
                } else if self.image_viewer.has_image() {
                    self.set_state_viewing();
                    self.system.wake_restore_only = true;
                } else {
                    self.set_state_start_menu(true);
                }
            } else {
                self.set_state_start_menu(true);
            }
            self.system.on_wake();
            self.dirty = true;
            return;
        }

        if self.state != AppState::Sleeping
            && self.state != AppState::SleepingPending
            && buttons.is_pressed(input::Buttons::Power)
        {
            self.start_sleep_request();
            return;
        }

        if Self::has_input(buttons) {
            self.system.reset_idle();
        }

        if matches!(self.state, AppState::PrcViewing) {
            for event in events {
                if self.prc_handle_touch_event(event) {
                    return;
                }
            }
        }

        if matches!(self.state, AppState::BookViewing | AppState::Toc) {
            for event in events {
                if self.reader_handle_touch_event(event) {
                    return;
                }
            }
        }

        if matches!(self.state, AppState::StartMenu) {
            let recents = self.system.collect_recent_paths(self.last_viewed_entry.as_ref());
            for event in events {
                if self.start_menu_handle_touch_event(event, &recents) {
                    return;
                }
            }
        }
        if matches!(self.state, AppState::Records) {
            for event in events {
                if self.records_handle_touch_event(event) {
                    return;
                }
            }
        }

        match self.state {
            AppState::StartMenu => {
                if self.home_about_open {
                    if buttons.is_pressed(input::Buttons::Back)
                        || buttons.is_pressed(input::Buttons::Confirm)
                    {
                        self.home_about_open = false;
                        self.home_about_form.reset();
                        self.home_touch_pressed_about = None;
                        self.dirty = true;
                    } else if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                    return;
                }
                if self.handle_home_menu_buttons(buttons, elapsed_ms) {
                    return;
                }
                let recents = self.system.collect_recent_paths(self.last_viewed_entry.as_ref());
                match self.home.handle_start_menu_input(&recents, buttons) {
                    HomeAction::OpenMenu => {
                        if self.home_menu_controller.open() {
                            self.dirty = true;
                        }
                    }
                    HomeAction::OpenRecent(path) => {
                        if path.to_ascii_lowercase().ends_with(".tdb")
                            || path.to_ascii_lowercase().ends_with(".prc")
                        {
                            if let Err(err) = self.open_prc_path(&path) {
                                self.set_error(err);
                            }
                        } else {
                            if let Err(err) = self.open_path(&path) {
                                if self.system.remove_recent(&path) {
                                    if self.last_viewed_entry.as_deref() == Some(path.as_str()) {
                                        self.last_viewed_entry = None;
                                    }
                                    self.system.save_recent_entries_now(self.source);
                                }
                                self.set_error(err);
                            }
                        }
                    }
                    HomeAction::None => {
                        if Self::has_input(buttons) {
                            self.dirty = true;
                        } else {
                            if self.system.add_idle(elapsed_ms) {
                                self.start_sleep_request();
                            }
                        }
                    }
                }
                if !Self::has_input(buttons)
                    && self.system.add_idle(elapsed_ms)
                {
                    self.start_sleep_request();
                }
            }
            AppState::Records => {
                if self.home_record_detail_index.is_some() {
                    if buttons.is_pressed(input::Buttons::Back) {
                        self.apply_home_record_detail_action(crate::ternos::ui::ModalFormAction::Closed);
                    } else if let Some(event) = reader_shell::nav_event_from_buttons(buttons)
                        && let Some(spec) = self.home_record_detail_spec()
                    {
                        let action = self.home_record_detail_form.on_event(
                            &spec,
                            event,
                            self.home_system_fonts.as_slice(),
                        );
                        self.apply_home_record_detail_action(action);
                    } else if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                    return;
                }
                if self.records_status_bar_focus.is_some() {
                    if let Some(button) = Self::nav_button_from_buttons(buttons) {
                        let _ = self.handle_records_status_bar_button(button);
                    } else if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                    return;
                }
                if buttons.is_pressed(input::Buttons::Up)
                    && self.home_records_form.focused_id() == Some(records_table_id())
                    && self.home_records_selected_row == 0
                    && self.home_records_top_row == 0
                {
                    self.set_records_status_from_hit(preferred_status_bar_focus(StatusBarButtons {
                        home_enabled: true,
                        menu_enabled: false,
                    }));
                    self.records_touch_pressed_status = None;
                    self.dirty = true;
                    return;
                }
                if let Some(event) = reader_shell::nav_event_from_buttons(buttons) {
                    let spec = self.home_records_spec();
                    let action = self.home_records_form.on_event(
                        &spec,
                        event,
                        self.home_system_fonts.as_slice(),
                    );
                    self.apply_home_records_action(action);
                } else if self.system.add_idle(elapsed_ms) {
                    self.start_sleep_request();
                }
            }
            AppState::Viewing => {
                if buttons.is_pressed(input::Buttons::Left) {
                    self.open_neighbor_file(-1);
                } else if buttons.is_pressed(input::Buttons::Right) {
                    self.open_neighbor_file(1);
                } else if buttons.is_pressed(input::Buttons::Back)
                    || buttons.is_pressed(input::Buttons::Confirm)
                {
                    self.exit_from = ExitFrom::Image;
                    self.exit_overlay_drawn = false;
                    self.state = AppState::ExitingPending;
                    self.dirty = true;
                } else {
                    if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                }
            }
            AppState::BookViewing => {
                self.handle_reader_shell_buttons(buttons, elapsed_ms, false);
            }
            AppState::Toc => {
                self.handle_reader_shell_buttons(buttons, elapsed_ms, true);
            }
            AppState::PrcViewing => {
                {
                    let form = self.runtime_prc_form();
                    if self.prc_ui_controller.sync_with_form(form.as_ref()) {
                        self.dirty = true;
                    }
                }
                if self.prc_session.is_some()
                    && self.prc_blocked_timeout_ticks == 0
                    && self.prc_status_bar_focus.is_none()
                    && !self.prc_menu_controller.is_active()
                    && self
                        .prc_session
                        .as_ref()
                        .and_then(|s| s.help_dialog())
                        .is_none()
                {
                    self.resume_prc_runtime_session();
                }
                if let Some(dialog) = self
                    .prc_session
                    .as_ref()
                    .and_then(|s| s.help_dialog())
                {
                    self.prc_help_controller.sync(&dialog);
                    let event = if buttons.is_pressed(input::Buttons::Up) {
                        Some(palm::ui_component::UiNavEvent::Up)
                    } else if buttons.is_pressed(input::Buttons::Down) {
                        Some(palm::ui_component::UiNavEvent::Down)
                    } else if buttons.is_pressed(input::Buttons::Left) {
                        Some(palm::ui_component::UiNavEvent::Left)
                    } else if buttons.is_pressed(input::Buttons::Right) {
                        Some(palm::ui_component::UiNavEvent::Right)
                    } else if buttons.is_pressed(input::Buttons::Back) {
                        Some(palm::ui_component::UiNavEvent::Back)
                    } else if buttons.is_pressed(input::Buttons::Confirm) {
                        Some(palm::ui_component::UiNavEvent::Confirm)
                    } else {
                        None
                    };
                    if let Some(event) = event {
                        match self.prc_help_controller.on_event(&dialog, self.prc_system_fonts.as_slice(), event) {
                            palm::controller::HelpDialogAction::Redraw => {
                                self.dirty = true;
                            }
                            palm::controller::HelpDialogAction::Scroll(delta) => {
                                if let Some(session) = self.prc_session.as_mut()
                                    && session.scroll_help_dialog(delta)
                                {
                                    self.dirty = true;
                                }
                            }
                            palm::controller::HelpDialogAction::Dismiss => {
                                if let Some(session) = self.prc_session.as_mut() {
                                    let _ = session.dismiss_help_dialog();
                                    self.prc_help_controller.clear();
                                    self.resume_prc_runtime_session();
                                }
                            }
                            palm::controller::HelpDialogAction::None => {}
                        }
                    } else if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                    return;
                }
                if self.prc_menu_controller.is_active() {
                    let event = if buttons.is_pressed(input::Buttons::Back) {
                        Some(palm::ui_component::UiNavEvent::Back)
                    } else if buttons.is_pressed(input::Buttons::Left) {
                        Some(palm::ui_component::UiNavEvent::Left)
                    } else if buttons.is_pressed(input::Buttons::Right) {
                        Some(palm::ui_component::UiNavEvent::Right)
                    } else if buttons.is_pressed(input::Buttons::Up) {
                        Some(palm::ui_component::UiNavEvent::Up)
                    } else if buttons.is_pressed(input::Buttons::Down) {
                        Some(palm::ui_component::UiNavEvent::Down)
                    } else if buttons.is_pressed(input::Buttons::Confirm) {
                        Some(palm::ui_component::UiNavEvent::Confirm)
                    } else {
                        None
                    };
                    if let Some(event) = event {
                        match self.prc_menu_controller.on_event(event) {
                            palm::controller::MenuAction::Activate(item_id) => {
                                if let Some(session) = self.prc_session.as_mut() {
                                    session.inject_event_now(
                                        palm::runtime::EVT_MENU,
                                        item_id,
                                        "menuSelect",
                                    );
                                    self.prc_blocked_elapsed_ms = 0;
                                    self.prc_blocked_timeout_ticks = 0;
                                    self.resume_prc_runtime_session();
                                } else {
                                    self.dirty = true;
                                }
                            }
                            palm::controller::MenuAction::Redraw
                            | palm::controller::MenuAction::Closed => {
                                self.dirty = true;
                            }
                            palm::controller::MenuAction::None => {}
                        }
                    } else if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                    return;
                }
                if self.prc_blocked_timeout_ticks > 0 {
                    self.prc_blocked_elapsed_ms = self.prc_blocked_elapsed_ms.saturating_add(elapsed_ms);
                    let wait_ms = self.prc_blocked_timeout_ticks.saturating_mul(10);
                    if self.prc_blocked_elapsed_ms >= wait_ms {
                        self.prc_blocked_elapsed_ms = 0;
                        self.prc_blocked_timeout_ticks = 0;
                        self.resume_prc_runtime_session();
                    }
                }
                let typed_chars = buttons.typed_chars();
                if !typed_chars.is_empty()
                    && self.prc_runtime_focused_field_id.is_some()
                    && self.prc_session.is_some()
                {
                    if let Some(session) = self.prc_session.as_mut() {
                        for ch in typed_chars {
                            session.inject_event_now(
                                palm::runtime::EVT_KEY_DOWN,
                                *ch,
                                "keyDown",
                            );
                        }
                    }
                    self.prc_blocked_elapsed_ms = 0;
                    self.prc_blocked_timeout_ticks = 0;
                    self.resume_prc_runtime_session();
                    return;
                }
                if buttons.is_pressed(input::Buttons::Left) {
                    if self.prc_status_bar_focus.is_some() {
                        if self.handle_prc_status_bar_button(crate::platform::ButtonId::Left) {
                            
                        }
                    } else if self.prc_status_bar_focus.is_none() {
                        let form = self.runtime_prc_form();
                        let focused_table = form.as_ref().and_then(|form| {
                            self.prc_ui_controller
                                .focused_control_id()
                                .and_then(|id| self.prc_table_object(form, id))
                        });
                        if let Some((table_id, _x, _y, _w, _h)) = focused_table
                            && let Some(table) =
                                self.prc_table_draw(form.as_ref().map(|f| f.form_id).unwrap_or(0), table_id)
                            && table.cols > 1
                        {
                            // Keep horizontal navigation available for multi-column Palm tables.
                        }
                        if self.prc_ui_controller.move_focus_direction(
                            form.as_ref(),
                            palm::controller::FocusDirection::Left,
                        ) {
                            self.dirty = true;
                        } else if let Some(session) = self.prc_session.as_mut() {
                            session.inject_event_now(
                                palm::runtime::EVT_KEY_DOWN,
                                Self::PALM_KEY_LEFT,
                                "keyLeft",
                            );
                            self.prc_blocked_elapsed_ms = 0;
                            self.prc_blocked_timeout_ticks = 0;
                            self.resume_prc_runtime_session();
                        }
                    }
                } else if buttons.is_pressed(input::Buttons::Right) {
                    if self.prc_status_bar_focus.is_some() {
                        if self.handle_prc_status_bar_button(crate::platform::ButtonId::Right) {
                            
                        }
                    } else if self.prc_status_bar_focus.is_none() {
                        let form = self.runtime_prc_form();
                        if self.prc_ui_controller.move_focus_direction(
                            form.as_ref(),
                            palm::controller::FocusDirection::Right,
                        ) {
                            self.dirty = true;
                        } else if let Some(session) = self.prc_session.as_mut() {
                            session.inject_event_now(
                                palm::runtime::EVT_KEY_DOWN,
                                Self::PALM_KEY_RIGHT,
                                "keyRight",
                            );
                            self.prc_blocked_elapsed_ms = 0;
                            self.prc_blocked_timeout_ticks = 0;
                            self.resume_prc_runtime_session();
                        }
                    }
                } else if buttons.is_pressed(input::Buttons::Up) {
                    let form = self.runtime_prc_form();
                    if self.prc_status_bar_focus.is_some() {
                    } else if let Some(form_ref) = form.as_ref() {
                        let table_nav = self
                            .prc_ui_controller
                            .focused_control_id()
                            .and_then(|control_id| self.prc_table_object(form_ref, control_id))
                            .and_then(|(table_id, _, _, _, _)| {
                                let table = self.prc_table_draw(form_ref.form_id, table_id)?;
                                let current =
                                    (table.selected_row >= 0).then_some(table.selected_row as usize);
                                let row = self.prc_table_next_row(table, current, -1)?;
                                Some((table_id, row))
                            });
                        if let Some((table_id, row)) = table_nav {
                            self.prc_sync_table_selection(form_ref.form_id, table_id, row as i16, 0);
                            self.dirty = true;
                        } else if self.prc_ui_controller.move_focus_direction(
                            form.as_ref(),
                            palm::controller::FocusDirection::Up,
                        ) {
                            self.dirty = true;
                        } else if self.prc_status_bar_focus.is_none() {
                            self.prc_status_bar_last_control =
                                self.prc_ui_controller.focused_control_id();
                            self.set_prc_status_from_hit(preferred_status_bar_focus(StatusBarButtons {
                                home_enabled: true,
                                menu_enabled: self.prc_menu_controller.menu_count() > 0,
                            }));
                            self.dirty = true;
                        } else if let Some(session) = self.prc_session.as_mut() {
                            session.inject_event_now(
                                palm::runtime::EVT_KEY_DOWN,
                                Self::PALM_KEY_UP,
                                "keyUp",
                            );
                            self.prc_blocked_elapsed_ms = 0;
                            self.prc_blocked_timeout_ticks = 0;
                            self.resume_prc_runtime_session();
                        }
                    } else if self.prc_ui_controller.move_focus_direction(
                        form.as_ref(),
                        palm::controller::FocusDirection::Up,
                    ) {
                        self.dirty = true;
                    } else if self.prc_status_bar_focus.is_none() {
                        self.prc_status_bar_last_control =
                            self.prc_ui_controller.focused_control_id();
                        self.set_prc_status_from_hit(preferred_status_bar_focus(StatusBarButtons {
                            home_enabled: true,
                            menu_enabled: self.prc_menu_controller.menu_count() > 0,
                        }));
                        self.dirty = true;
                    } else if let Some(session) = self.prc_session.as_mut() {
                        session.inject_event_now(
                            palm::runtime::EVT_KEY_DOWN,
                            Self::PALM_KEY_UP,
                            "keyUp",
                        );
                        self.prc_blocked_elapsed_ms = 0;
                        self.prc_blocked_timeout_ticks = 0;
                        self.resume_prc_runtime_session();
                    }
                } else if buttons.is_pressed(input::Buttons::Down) {
                    let form = self.runtime_prc_form();
                    if self.prc_status_bar_focus.is_some() {
                        if self.handle_prc_status_bar_button(crate::platform::ButtonId::Down) {
                            let target_id = self
                                .prc_ui_controller
                                .first_button_id(form.as_ref())
                                .or_else(|| self.prc_ui_controller.first_control_id(form.as_ref()));
                            let restored = if let Some(id) = target_id {
                                let _ = self.prc_ui_controller.select_control_id(form.as_ref(), id);
                                self.prc_ui_controller.focused_control_id() == Some(id)
                            } else {
                                false
                            };
                            if !restored {
                                let _ = self.prc_ui_controller.move_focus_direction(
                                    form.as_ref(),
                                    palm::controller::FocusDirection::Down,
                                );
                            }
                            self.dirty = true;
                        }
                    } else if let Some(form_ref) = form.as_ref() {
                        let table_nav = self
                            .prc_ui_controller
                            .focused_control_id()
                            .and_then(|control_id| self.prc_table_object(form_ref, control_id))
                            .and_then(|(table_id, _, _, _, _)| {
                                let table = self.prc_table_draw(form_ref.form_id, table_id)?;
                                let current =
                                    (table.selected_row >= 0).then_some(table.selected_row as usize);
                                let row = self
                                    .prc_table_next_row(table, current, 1)
                                    .or_else(|| current.or_else(|| self.prc_table_first_selectable_row(table)))?;
                                Some((table_id, row))
                            });
                        if let Some((table_id, row)) = table_nav {
                            self.prc_sync_table_selection(form_ref.form_id, table_id, row as i16, 0);
                            self.dirty = true;
                        } else if self.prc_ui_controller.move_focus_direction(
                            form.as_ref(),
                            palm::controller::FocusDirection::Down,
                        ) {
                            self.dirty = true;
                        } else if let Some(session) = self.prc_session.as_mut() {
                            session.inject_event_now(
                                palm::runtime::EVT_KEY_DOWN,
                                Self::PALM_KEY_DOWN,
                                "keyDownNav",
                            );
                            self.prc_blocked_elapsed_ms = 0;
                            self.prc_blocked_timeout_ticks = 0;
                            self.resume_prc_runtime_session();
                        }
                    } else if self.prc_ui_controller.move_focus_direction(
                        form.as_ref(),
                        palm::controller::FocusDirection::Down,
                    ) {
                        self.dirty = true;
                    } else if let Some(session) = self.prc_session.as_mut() {
                        session.inject_event_now(
                            palm::runtime::EVT_KEY_DOWN,
                            Self::PALM_KEY_DOWN,
                            "keyDownNav",
                        );
                        self.prc_blocked_elapsed_ms = 0;
                        self.prc_blocked_timeout_ticks = 0;
                        self.resume_prc_runtime_session();
                    }
                } else if buttons.is_pressed(input::Buttons::Confirm) {
                    if let Some(shell_focus) = self.prc_status_bar_focus {
                        let _ = shell_focus;
                        self.handle_prc_status_bar_button(crate::platform::ButtonId::Confirm);
                    } else {
                        let form = self.runtime_prc_form();
                        if let Some(control_id) = self.prc_ui_controller.focused_control_id() {
                            let table_activation = form.as_ref().and_then(|form_ref| {
                                let (table_id, x, y, w, h) =
                                    self.prc_table_object(form_ref, control_id)?;
                                let table = self.prc_table_draw(form_ref.form_id, table_id)?;
                                let row = if table.selected_row >= 0 {
                                    Some(table.selected_row as usize)
                                } else {
                                    self.prc_table_first_selectable_row(table)
                                }?;
                                let (pen_x, pen_y) =
                                    self.prc_table_pen_point(table, x, y, w, h, row)?;
                                Some((form_ref.form_id, table_id, row as i16, pen_x, pen_y))
                            });
                            if let Some((form_id, table_id, row, pen_x, pen_y)) = table_activation {
                                self.prc_sync_table_selection(form_id, table_id, row, 0);
                                if let Some(session) = self.prc_session.as_mut() {
                                    session.inject_pen_down_now(pen_x, pen_y, "keyTblPenDown");
                                    self.prc_blocked_elapsed_ms = 0;
                                    self.prc_blocked_timeout_ticks = 0;
                                    self.resume_prc_runtime_session();
                                    return;
                                }
                            }
                        }
                        if let (Some(control_id), Some(session)) =
                            (self.prc_ui_controller.focused_control_id(), self.prc_session.as_mut())
                        {
                            let focused_is_field = form
                                .as_ref()
                                .and_then(|f| {
                                    f.objects.iter().find_map(|o| match o {
                                        palm::form_preview::FormPreviewObject::Field { id, .. }
                                            if *id == control_id =>
                                        {
                                            Some(true)
                                        }
                                        _ => None,
                                    })
                                })
                                .unwrap_or(false);
                            if focused_is_field {
                                session.inject_event_now(
                                    palm::runtime::EVT_FLD_ENTER,
                                    control_id,
                                    "fldEnter",
                                );
                            } else {
                                session.inject_control_select_now(control_id);
                            }
                            self.prc_blocked_elapsed_ms = 0;
                            self.prc_blocked_timeout_ticks = 0;
                            self.resume_prc_runtime_session();
                        } else {
                            if let Some(session) = self.prc_session.as_mut() {
                                session.inject_event_now(
                                    palm::runtime::EVT_KEY_DOWN,
                                    Self::PALM_KEY_RETURN,
                                    "keyReturn",
                                );
                                self.prc_blocked_elapsed_ms = 0;
                                self.prc_blocked_timeout_ticks = 0;
                                self.resume_prc_runtime_session();
                            }
                        }
                    }
                } else if self.system.add_idle(elapsed_ms) {
                    self.start_sleep_request();
                }
            }
            AppState::SleepingPending => {}
            AppState::Sleeping => {}
            AppState::ExitingPending => {}
            AppState::Error => {
                if buttons.is_pressed(input::Buttons::Back)
                    || buttons.is_pressed(input::Buttons::Confirm)
                {
                    self.error_message = None;
                    self.set_state_start_menu(true);
                }
            }
        }
    }

    pub fn needs_draw(&self) -> bool {
        self.dirty
    }

    pub fn draw(&mut self, display: &mut impl crate::display::Display) {
        if !self.dirty {
            return;
        }

        self.dirty = false;
        match self.state {
            AppState::StartMenu => self.draw_start_menu(display),
            AppState::Records => self.draw_records(display),
            AppState::Viewing => self.draw_image_viewer(display),
            AppState::BookViewing => {
                if let Some(indicator) = self.book_reader.take_page_turn_indicator() {
                    self.draw_page_turn_indicator(display, indicator);
                }
                self.draw_book_reader(display);
            }
            AppState::ExitingPending => {
                if !self.exit_overlay_drawn {
                    match self.exit_from {
                        ExitFrom::Image => self.draw_image_viewer(display),
                        ExitFrom::Book => self.draw_book_reader(display),
                    }
                    self.draw_exiting_overlay(display);
                    self.exit_overlay_drawn = true;
                    self.dirty = true;
                    return;
                }
                match self.exit_from {
                    ExitFrom::Image => self.exit_image(),
                    ExitFrom::Book => self.exit_book(),
                }
                self.state = AppState::StartMenu;
                self.home.start_menu_cache.clear();
                self.set_state_start_menu(true);
            }
            AppState::Toc => self.draw_toc_view(display),
            AppState::PrcViewing => self.draw_prc_viewer(display),
            AppState::SleepingPending => {
                self.draw_sleeping_indicator(display);
                let resume_debug = format!(
                    "state={:?} current_entry={:?} last_viewed_entry={:?} has_book={} current_page={} last_rendered={:?}",
                    self.state,
                    self.current_entry,
                    self.last_viewed_entry,
                    self.book_reader.current_book.is_some(),
                    self.book_reader.current_page,
                    self.book_reader.last_rendered_page
                );
                let outcome = self.system.save_resume_or_error(ResumeContext {
                    source: self.source,
                    resume_debug: &resume_debug,
                    in_start_menu: self.state == AppState::StartMenu,
                    current_entry: self.current_entry.as_ref(),
                    last_viewed_entry: self.last_viewed_entry.as_ref(),
                    home_current_entry: None,
                    book_reader: &self.book_reader,
                });
                if outcome.is_ok() {
                    self.state = AppState::Sleeping;
                    self.system.start_sleep_overlay();
                    self.draw_sleep_overlay(display);
                } else if let Err(message) = outcome {
                    self.set_state_error_message(message);
                }
            }
            AppState::Sleeping => {
                self.draw_sleep_overlay(display);
            }
            AppState::Error => self.draw_error(display),
        }
        self.system.full_refresh = false;
        if self.state == AppState::Error && self.system.sleep_after_error {
            self.system.sleep_after_error = false;
            self.state = AppState::Sleeping;
            self.system.start_sleep_overlay();
            self.dirty = true;
        }
    }

    pub fn with_source<R>(&mut self, f: impl FnOnce(&mut S) -> R) -> R {
        f(self.source)
    }

    pub fn source_mut(&mut self) -> &mut S {
        self.source
    }

    fn has_input(buttons: &input::ButtonState) -> bool {
        use input::Buttons::*;
        let list = [Back, Confirm, Left, Right, Up, Down, Power];
        list.iter()
            .any(|b| buttons.is_pressed(*b) || buttons.is_held(*b))
    }

    pub fn take_sleep_transition(&mut self) -> bool {
        self.system.take_sleep_transition()
    }

    pub fn take_wake_transition(&mut self) -> bool {
        self.system.take_wake_transition()
    }

    pub fn set_battery_percent(&mut self, percent: Option<u8>) {
        if self.system.set_battery_percent(percent) && self.state == AppState::StartMenu {
            self.dirty = true;
        }
    }

    fn split_entry_path(full_path: &str) -> Result<(Vec<String>, ImageEntry), ImageError> {
        let mut parts: Vec<String> = full_path
            .split('/')
            .filter(|p| !p.is_empty())
            .map(|p| p.to_string())
            .collect();
        if parts.is_empty() {
            return Err(ImageError::Message("Invalid path.".into()));
        }
        let name = parts.pop().unwrap_or_default();
        Ok((
            parts,
            ImageEntry {
                name,
                kind: EntryKind::File,
            },
        ))
    }

    fn entry_full_path(path: &[String], entry: &ImageEntry) -> String {
        if path.is_empty() {
            entry.name.clone()
        } else {
            format!("{}/{}", path.join("/"), entry.name)
        }
    }

    fn open_path(&mut self, full_path: &str) -> Result<(), ImageError> {
        let (path, entry) = Self::split_entry_path(full_path)?;
        if is_prc(&entry.name) {
            self.open_prc_path(full_path)
        } else {
            self.open_file_entry(path, entry);
            Ok(())
        }
    }

    fn open_neighbor_file(&mut self, delta: i32) {
        let Some(current) = self.current_entry.clone() else {
            return;
        };
        let Ok((path, entry)) = Self::split_entry_path(&current) else {
            return;
        };
        let Ok(entries) = self.source.refresh(&path) else {
            return;
        };
        let Some(index) = entries.iter().position(|candidate| candidate.name == entry.name) else {
            return;
        };
        let next = (index as i32 + delta).clamp(0, entries.len().saturating_sub(1) as i32) as usize;
        let Some(next_entry) = entries.get(next).cloned() else {
            return;
        };
        if next_entry.kind == EntryKind::File {
            self.open_file_entry(path, next_entry);
        }
    }

    fn open_file_entry(&mut self, path: Vec<String>, entry: ImageEntry) {
        if is_trbk(&entry.name) {
            self.open_book_entry(path, entry);
            return;
        }
        if is_epub(&entry.name) {
            self.set_error(ImageError::Message(
                "EPUB files must be converted to .trbk.".into(),
            ));
            return;
        }
        if is_prc(&entry.name) {
            self.open_prc_entry(path, entry);
            return;
        }
        self.open_image_entry(path, entry);
    }

    fn open_book_entry(&mut self, path: Vec<String>, entry: ImageEntry) {
        let entry_name = Self::entry_full_path(&path, &entry);
        match self.book_reader.open(
            self.source,
            &path,
            &entry,
            &entry_name,
            &self.system.book_positions,
        ) {
            Ok(()) => {
                self.current_entry = Some(entry_name.clone());
                self.last_viewed_entry = Some(entry_name.clone());
                self.mark_recent_now(entry_name);
                log::info!("Opened book entry: {:?}", self.current_entry);
                self.set_state_book_viewing();
            }
            Err(err) => self.set_error(err),
        }
    }

    fn open_image_entry(&mut self, path: Vec<String>, entry: ImageEntry) {
        match self.image_viewer.open(self.source, &path, &entry) {
            Ok(()) => {
                let entry_name = Self::entry_full_path(&path, &entry);
                self.current_entry = Some(entry_name.clone());
                self.last_viewed_entry = Some(entry_name.clone());
                self.mark_recent_now(entry_name);
                log::info!("Opened image entry: {:?}", self.current_entry);
                self.set_state_viewing();
                self.system.reset_idle();
                self.system.sleep_overlay = None;
                self.system.clear_sleep_overlay_pending();
            }
            Err(err) => self.set_error(err),
        }
    }

    fn open_prc_entry(&mut self, path: Vec<String>, entry: ImageEntry) {
        match self.source.load_prc_info(&path, &entry) {
            Ok(info) => {
                let entry_name = Self::entry_full_path(&path, &entry);
                self.current_entry = Some(entry_name.clone());
                self.last_viewed_entry = Some(entry_name.clone());
                self.mark_recent_now(entry_name);
                self.prc_return_to_start_menu = true;
                self.prc_active_entry = Some(entry.clone());
                self.prc_session = None;
                self.prc_blocked_timeout_ticks = 0;
                self.prc_blocked_elapsed_ms = 0;
                self.prc_lines = palm::format_info_lines(&info);
                let runtime_snapshot = self.log_prc_info(&path, &entry, &info);
                self.prc_runtime_form_id = runtime_snapshot.form_id;
                self.prc_runtime_underlay_form_id = runtime_snapshot.underlay_form_id;
                self.prc_runtime_focused_field_id = runtime_snapshot.focused_field_id;
                self.prc_ui_controller.reset();
                self.prc_runtime_bitmap_draws = runtime_snapshot.bitmap_draws;
                self.prc_runtime_button_labels = runtime_snapshot.button_labels;
                self.prc_runtime_selected_controls = runtime_snapshot.selected_controls;
                self.prc_runtime_field_draws = runtime_snapshot.field_draws;
                self.prc_runtime_table_draws = runtime_snapshot.table_draws;
                log::info!(
                    "PRC runtime_ui form_id={:?} bitmap_draws={} field_draws={} tables={} help={}",
                    self.prc_runtime_form_id,
                    self.prc_runtime_bitmap_draws.len(),
                    self.prc_runtime_field_draws.len(),
                    self.prc_runtime_table_draws.len(),
                    runtime_snapshot.help_dialog.is_some()
                );
                self.prc_system_fonts = self.source.load_prc_system_fonts();
                self.prc_forms.clear();
                self.prc_bitmaps.clear();
                self.prc_menu_controller.set_menu_bar(None);
                if let Ok(prc_raw) = self.source.load_prc_bytes(&path, &entry) {
                    let mut merged_resources =
                        self.source
                            .load_prc_app_resources(&path, &entry, &info);
                    merged_resources.extend(palm::parse_prc_resource_blobs(&prc_raw));
                    self.prc_forms =
                        palm::form_preview::parse_form_previews_from_resource_blobs(
                            &merged_resources,
                        );
                    self.prc_bitmaps =
                        palm::bitmap::parse_prc_bitmaps_from_resource_blobs(&merged_resources);
                    if self.prc_forms.is_empty() {
                        self.prc_forms = palm::form_preview::parse_form_previews(&prc_raw);
                    }
                    if self.prc_bitmaps.is_empty() {
                        self.prc_bitmaps = palm::bitmap::parse_prc_bitmaps(&prc_raw);
                    }
                    let menu_bar = palm::menu_preview::parse_menu_bar_preview(&prc_raw);
                    log::info!(
                        "PRC parsed previews forms={} bitmaps={} menus={} merged_resources={}",
                        self.prc_forms.len(),
                        self.prc_bitmaps.len(),
                        menu_bar.as_ref().map(|m| m.menus.len()).unwrap_or(0),
                        merged_resources.len(),
                    );
                    if let Some(menu_bar) = menu_bar.as_ref() {
                        for menu in &menu_bar.menus {
                            log::info!(
                                "PRC menu parsed id={} title='{}' items={}",
                                menu.resource_id,
                                menu.title,
                                menu.items.len()
                            );
                        }
                    }
                    self.prc_menu_controller.set_menu_bar(menu_bar);
                }
                if let Ok(session) = palm::runner::PrcRuntimeSession::from_source(
                    self.source,
                    &path,
                    &entry,
                    &info,
                    0,
                ) {
                    self.prc_session = Some(session);
                    self.resume_prc_runtime_session();
                }
                self.prc_form_index = self.best_prc_form_index().unwrap_or(0);
                self.prc_lines
                    .insert(0, format!("Form resources parsed: {}", self.prc_forms.len()));
                self.prc_lines
                    .insert(1, format!("Bitmap resources parsed: {}", self.prc_bitmaps.len()));
                if let Some(fid) = self.prc_runtime_form_id {
                    self.prc_lines.insert(2, format!("Runtime form id: {}", fid));
                }
                self.prc_scroll = 0;
                self.set_state_prc_viewing();
            }
            Err(err) => self.set_error(err),
        }
    }

    fn open_prc_path(&mut self, full_path: &str) -> Result<(), ImageError> {
        let mut parts: Vec<String> = full_path
            .split('/')
            .filter(|p| !p.is_empty())
            .map(|p| p.to_string())
            .collect();
        if parts.is_empty() {
            return Err(ImageError::Message("Invalid app path.".into()));
        }
        let name = parts.pop().unwrap_or_default();
        let path = parts;
        let entry = ImageEntry {
            name,
            kind: EntryKind::File,
        };
        match self.source.load_prc_info(&path, &entry) {
            Ok(info) => {
                let recent_path = full_path.to_string();
                self.current_entry = Some(recent_path.clone());
                self.last_viewed_entry = Some(recent_path.clone());
                self.mark_recent_now(recent_path);
                self.prc_return_to_start_menu = true;
                self.prc_active_entry = Some(entry.clone());
                self.prc_session = None;
                self.prc_blocked_timeout_ticks = 0;
                self.prc_blocked_elapsed_ms = 0;
                self.prc_lines = palm::format_info_lines(&info);
                let runtime_snapshot = self.log_prc_info(&path, &entry, &info);
                self.prc_runtime_form_id = runtime_snapshot.form_id;
                self.prc_runtime_underlay_form_id = runtime_snapshot.underlay_form_id;
                self.prc_runtime_focused_field_id = runtime_snapshot.focused_field_id;
                self.prc_ui_controller.reset();
                self.prc_runtime_bitmap_draws = runtime_snapshot.bitmap_draws;
                self.prc_runtime_button_labels = runtime_snapshot.button_labels;
                self.prc_runtime_selected_controls = runtime_snapshot.selected_controls;
                self.prc_runtime_field_draws = runtime_snapshot.field_draws;
                self.prc_runtime_table_draws = runtime_snapshot.table_draws;
                self.prc_system_fonts = self.source.load_prc_system_fonts();
                self.prc_forms.clear();
                self.prc_bitmaps.clear();
                self.prc_menu_controller.set_menu_bar(None);
                if let Ok(prc_raw) = self.source.load_prc_bytes(&path, &entry) {
                    let mut merged_resources =
                        self.source
                            .load_prc_app_resources(&path, &entry, &info);
                    merged_resources.extend(palm::parse_prc_resource_blobs(&prc_raw));
                    self.prc_forms =
                        palm::form_preview::parse_form_previews_from_resource_blobs(
                            &merged_resources,
                        );
                    self.prc_bitmaps =
                        palm::bitmap::parse_prc_bitmaps_from_resource_blobs(&merged_resources);
                    if self.prc_forms.is_empty() {
                        self.prc_forms = palm::form_preview::parse_form_previews(&prc_raw);
                    }
                    if self.prc_bitmaps.is_empty() {
                        self.prc_bitmaps = palm::bitmap::parse_prc_bitmaps(&prc_raw);
                    }
                    let menu_bar = palm::menu_preview::parse_menu_bar_preview(&prc_raw);
                    self.prc_menu_controller.set_menu_bar(menu_bar);
                }
                if let Ok(session) = palm::runner::PrcRuntimeSession::from_source(
                    self.source,
                    &path,
                    &entry,
                    &info,
                    0,
                )
                {
                    self.prc_session = Some(session);
                    self.resume_prc_runtime_session();
                }
                self.prc_form_index = self.best_prc_form_index().unwrap_or(0);
                self.prc_scroll = 0;
                self.set_state_prc_viewing();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn mark_recent_now(&mut self, path: String) {
        self.system.mark_recent(path);
        self.system.save_recent_entries_now(self.source);
        self.home.start_menu_cache.clear();
        self.home.start_menu_need_base_refresh = true;
    }

    fn resume_prc_runtime_session(&mut self) {
        let Some(session) = self.prc_session.as_mut() else {
            return;
        };
        let prev_help_dialog = session.help_dialog();
        let runtime_out = session.resume();
        let runtime_snapshot = runtime_out.snapshot;
        let changed = self.prc_runtime_form_id != runtime_snapshot.form_id
            || self.prc_runtime_underlay_form_id != runtime_snapshot.underlay_form_id
            || self.prc_runtime_focused_field_id != runtime_snapshot.focused_field_id
            || self.prc_runtime_bitmap_draws.len() != runtime_snapshot.bitmap_draws.len()
            || self
                .prc_runtime_bitmap_draws
                .iter()
                .zip(runtime_snapshot.bitmap_draws.iter())
                .any(|(a, b)| a.resource_id != b.resource_id || a.x != b.x || a.y != b.y)
            || self.prc_runtime_button_labels != runtime_snapshot.button_labels
            || self.prc_runtime_selected_controls != runtime_snapshot.selected_controls
            || self.prc_runtime_field_draws != runtime_snapshot.field_draws
            || self.prc_runtime_table_draws != runtime_snapshot.table_draws
            || prev_help_dialog != runtime_snapshot.help_dialog;
        log::info!(
            "PRC runtime_ui update form_id={:?} bitmap_draws={} field_draws={} tables={} first_field={:?} help={:?} changed={}",
            runtime_snapshot.form_id,
            runtime_snapshot.bitmap_draws.len(),
            runtime_snapshot.field_draws.len(),
            runtime_snapshot.table_draws.len(),
            runtime_snapshot
                .field_draws
                .first()
                .map(|f| (f.field_id, f.text.len())),
            runtime_snapshot.help_dialog.as_ref().map(|h| h.help_id),
            changed
        );
        self.prc_runtime_form_id = runtime_snapshot.form_id;
        self.prc_runtime_underlay_form_id = runtime_snapshot.underlay_form_id;
        self.prc_runtime_focused_field_id = runtime_snapshot.focused_field_id;
        self.prc_runtime_bitmap_draws = runtime_snapshot.bitmap_draws;
        self.prc_runtime_button_labels = runtime_snapshot.button_labels;
        self.prc_runtime_selected_controls = runtime_snapshot.selected_controls;
        self.prc_runtime_field_draws = runtime_snapshot.field_draws;
        self.prc_runtime_table_draws = runtime_snapshot.table_draws;
        {
            let form = self.runtime_prc_form();
            if self.prc_ui_controller.sync_with_form(form.as_ref()) {
                self.dirty = true;
            }
        }
        self.prc_blocked_timeout_ticks = match runtime_out.state {
            palm::runner::RuntimeRunState::BlockedOnEvent { timeout_ticks } => {
                log::info!(
                    "PRC runtime blocked on EvtGetEvent timeout={} ticks steps={}",
                    timeout_ticks,
                    runtime_out.steps
                );
                timeout_ticks
            }
            palm::runner::RuntimeRunState::Stopped(reason) => {
                log::info!(
                    "PRC runtime stopped reason={:?} steps={}",
                    reason,
                    runtime_out.steps
                );
                0
            }
            palm::runner::RuntimeRunState::Running => {
                log::info!("PRC runtime running steps={}", runtime_out.steps);
                0
            }
        };
        self.prc_blocked_elapsed_ms = 0;
        if changed {
            self.dirty = true;
        }
    }

    fn log_prc_info(
        &mut self,
        path: &[String],
        entry: &ImageEntry,
        info: &palm::PrcInfo,
    ) -> palm::runner::RuntimeUiSnapshot {
        log::info!(
            "PRC name='{}' type='{}' creator='{}' kind={:?} entries={} ver={} attrs=0x{:04X} size={} code_bytes={} other_bytes={}",
            info.db_name,
            info.type_code,
            info.creator_code,
            info.kind,
            info.entry_count,
            info.version,
            info.attributes,
            info.file_size,
            info.code_bytes,
            info.other_bytes
        );
        if Self::prc_verbose_logs() {
            let mut group_counts = [
                ("mem", 0u32),
                ("dm", 0u32),
                ("sys", 0u32),
                ("evt", 0u32),
                ("fld", 0u32),
                ("frm", 0u32),
                ("lst", 0u32),
                ("win", 0u32),
                ("menu", 0u32),
                ("tim", 0u32),
                ("str", 0u32),
                ("snd", 0u32),
                ("fnt", 0u32),
                ("lib", 0u32),
                ("unknown", 0u32),
            ];
            log::info!(
                "PRC traps a_total={} trap15_total={} unique_a_traps={}",
                info.a_trap_total,
                info.trap15_total,
                info.unique_a_traps.len()
            );
            for trap in &info.unique_a_traps {
                let meta = palm::traps::table::lookup(*trap);
                for (group, count) in &mut group_counts {
                    if *group == meta.group.as_str() {
                        *count = count.saturating_add(1);
                        break;
                    }
                }
                log::info!(
                    "PRC trap A 0x{:04X} group={} name={}",
                    trap,
                    meta.group.as_str(),
                    meta.name
                );
            }
            for (group, count) in group_counts {
                if count > 0 {
                    log::info!("PRC trap_group {} count={}", group, count);
                }
            }
            for res in &info.resources {
                log::info!(
                    "PRC resource kind='{}' id={} offset={} size={}",
                    res.kind,
                    res.id,
                    res.offset,
                    res.size
                );
            }
            for scan in &info.code_scan {
                log::info!(
                    "PRC code_scan id={} size={} a_traps={} trap15={} unique_a={}",
                    scan.resource_id,
                    scan.size,
                    scan.a_trap_count,
                    scan.trap15_count,
                    scan.unique_a_traps.len()
                );
                for trap in &scan.unique_a_traps {
                    let meta = palm::traps::table::lookup(*trap);
                    log::info!(
                        "PRC code_scan id={} trap=0x{:04X} group={} name={}",
                        scan.resource_id,
                        trap,
                        meta.group.as_str(),
                        meta.name
                    );
                }
            }

            let dry_run = palm::runtime::dry_run_default(info);
            log::info!(
                "PRC dry_run(strict) total_hits={} handled={} stubbed={}",
                dry_run.total_hits,
                dry_run.handled,
                dry_run.stubbed
            );
            if let Some(stop) = dry_run.unimplemented {
                if stop.trap15 {
                    log::info!(
                        "PRC dry_run stop trap15 resource_id={} code_offset={} file_offset={}",
                        stop.resource_id,
                        stop.code_offset,
                        stop.file_offset
                    );
                } else {
                    log::info!(
                        "PRC dry_run stop trap=0x{:04X} group={} name={} resource_id={} code_offset={} file_offset={}",
                        stop.trap_word,
                        stop.group.as_str(),
                        stop.name,
                        stop.resource_id,
                        stop.code_offset,
                        stop.file_offset
                    );
                }
            } else {
                log::info!("PRC dry_run(strict) complete without unimplemented trap");
            }
            for probe in &dry_run.lib_dispatch_probes {
                if let Some(selector) = probe.selector {
                    log::info!(
                        "PRC lib_probe resource_id={} code_offset={} file_offset={} selector=0x{:04X} next1={:?} next2={:?}",
                        probe.resource_id,
                        probe.code_offset,
                        probe.file_offset,
                        selector,
                        probe.next_word_1,
                        probe.next_word_2
                    );
                } else {
                    log::info!(
                        "PRC lib_probe resource_id={} code_offset={} file_offset={} selector=? next1={:?} next2={:?}",
                        probe.resource_id,
                        probe.code_offset,
                        probe.file_offset,
                        probe.next_word_1,
                        probe.next_word_2
                    );
                }
            }

            let dry_run_no_lib = palm::runtime::dry_run_ignore_lib(info);
            log::info!(
                "PRC dry_run(ignore_lib) total_hits={} handled={} stubbed={}",
                dry_run_no_lib.total_hits,
                dry_run_no_lib.handled,
                dry_run_no_lib.stubbed
            );
            if let Some(stop) = dry_run_no_lib.unimplemented {
                if stop.trap15 {
                    log::info!(
                        "PRC dry_run(ignore_lib) stop trap15 resource_id={} code_offset={} file_offset={}",
                        stop.resource_id,
                        stop.code_offset,
                        stop.file_offset
                    );
                } else {
                    log::info!(
                        "PRC dry_run(ignore_lib) stop trap=0x{:04X} group={} name={} resource_id={} code_offset={} file_offset={}",
                        stop.trap_word,
                        stop.group.as_str(),
                        stop.name,
                        stop.resource_id,
                        stop.code_offset,
                        stop.file_offset
                    );
                }
            } else {
                log::info!("PRC dry_run(ignore_lib) complete without unimplemented trap");
            }
            for probe in &dry_run_no_lib.lib_dispatch_probes {
                if let Some(selector) = probe.selector {
                    log::info!(
                        "PRC lib_probe(ignore_lib) resource_id={} code_offset={} file_offset={} selector=0x{:04X} next1={:?} next2={:?}",
                        probe.resource_id,
                        probe.code_offset,
                        probe.file_offset,
                        selector,
                        probe.next_word_1,
                        probe.next_word_2
                    );
                } else {
                    log::info!(
                        "PRC lib_probe(ignore_lib) resource_id={} code_offset={} file_offset={} selector=? next1={:?} next2={:?}",
                        probe.resource_id,
                        probe.code_offset,
                        probe.file_offset,
                        probe.next_word_1,
                        probe.next_word_2
                    );
                }
            }

            let dry_run_bootstrap = palm::runtime::dry_run_ignore_bootstrap_lib(info);
            log::info!(
                "PRC dry_run(ignore_bootstrap_lib) total_hits={} handled={} stubbed={}",
                dry_run_bootstrap.total_hits,
                dry_run_bootstrap.handled,
                dry_run_bootstrap.stubbed
            );
            if let Some(stop) = dry_run_bootstrap.unimplemented {
                if stop.trap15 {
                    log::info!(
                        "PRC dry_run(ignore_bootstrap_lib) stop trap15 resource_id={} code_offset={} file_offset={}",
                        stop.resource_id,
                        stop.code_offset,
                        stop.file_offset
                    );
                } else {
                    log::info!(
                        "PRC dry_run(ignore_bootstrap_lib) stop trap=0x{:04X} group={} name={} resource_id={} code_offset={} file_offset={}",
                        stop.trap_word,
                        stop.group.as_str(),
                        stop.name,
                        stop.resource_id,
                        stop.code_offset,
                        stop.file_offset
                    );
                }
            } else {
                log::info!("PRC dry_run(ignore_bootstrap_lib) complete without unimplemented trap");
            }
        }

        if Self::prc_verbose_logs() {
            palm::runner::log_prc_runtime_first_trap(
                self.source,
                path,
                entry,
                info,
                true,
            )
        } else {
            palm::runner::RuntimeUiSnapshot::default()
        }
    }

    fn prc_verbose_logs() -> bool {
        false
    }

    fn exit_image(&mut self) {
        self.source.save_resume(None);
        self.system.save_recent_entries_now(self.source);
    }

    fn exit_book(&mut self) {
        self.system.update_book_position(
            &self.book_reader,
            self.current_entry.as_ref(),
            self.last_viewed_entry.as_ref(),
        );
        self.system.save_book_positions_now(self.source);
        self.system.save_recent_entries_now(self.source);
        self.book_reader.close(self.source);
    }

    fn set_error(&mut self, err: ImageError) {
        let message = match err {
            ImageError::Io => "I/O error while accessing storage.".into(),
            ImageError::Decode => "Failed to decode image.".into(),
            ImageError::Unsupported => "Unsupported image format.".into(),
            ImageError::Message(message) => message,
        };
        self.set_state_error_message(message);
    }

    fn set_state_start_menu(&mut self, need_base_refresh: bool) {
        self.state = AppState::StartMenu;
        self.home.start_menu_need_base_refresh = need_base_refresh;
        self.install_scan_elapsed_ms = 2000;
        self.home_records_open = false;
        self.home_record_detail_index = None;
        self.home_records_form.reset();
        self.home_record_detail_form.reset();
        self.records_status_bar_focus = None;
        self.records_touch_pressed_status = None;
        self.home_menu_last_rect = None;
        self.dirty = true;
    }

    fn release_prc_resources(&mut self) {
        self.prc_active_entry = None;
        self.prc_session = None;
        self.prc_runtime_form_id = None;
        self.prc_runtime_underlay_form_id = None;
        self.prc_runtime_focused_field_id = None;
        self.prc_blocked_timeout_ticks = 0;
        self.prc_blocked_elapsed_ms = 0;
        self.prc_soft_menu_focused = false;
        self.prc_soft_menu_last_control = None;
        self.prc_status_bar_focus = None;
        self.prc_touch_pressed_status = None;
        self.prc_status_bar_last_control = None;
        self.prc_scroll = 0;
        self.prc_form_index = 0;
        self.prc_ui_controller.reset();
        self.prc_lines = Vec::new();
        self.prc_forms = Vec::new();
        self.prc_bitmaps = Vec::new();
        self.prc_runtime_bitmap_draws = Vec::new();
        self.prc_runtime_button_labels = Vec::new();
        self.prc_runtime_selected_controls = Vec::new();
        self.prc_runtime_field_draws = Vec::new();
        self.prc_runtime_table_draws = Vec::new();
        self.prc_system_fonts = Vec::new();
        self.prc_menu_controller.reset();
        self.prc_reserved_gray_initialized = false;
    }

    fn exit_prc_viewer_to_origin(&mut self) {
        self.prc_return_to_start_menu = false;
        self.release_prc_resources();
        self.system.full_refresh = true;
        self.set_state_start_menu(true);
    }

    fn set_state_viewing(&mut self) {
        self.state = AppState::Viewing;
        self.home_about_open = false;
        self.home_about_form.reset();
        self.home_records_open = false;
        self.home_records_form.reset();
        self.home_record_detail_form.reset();
        self.home_record_detail_index = None;
        self.home_menu_controller.close();
        self.home_menu_last_rect = None;
        self.system.full_refresh = true;
        self.dirty = true;
    }

    fn set_state_book_viewing(&mut self) {
        self.state = AppState::BookViewing;
        self.home_about_open = false;
        self.home_about_form.reset();
        self.home_records_open = false;
        self.home_records_form.reset();
        self.home_record_detail_form.reset();
        self.home_record_detail_index = None;
        self.home_menu_controller.close();
        self.home_menu_last_rect = None;
        self.reader_status_bar_focus = None;
        self.reader_touch_pressed_status = None;
        self.reader_touch_pressed_menu = None;
        self.reader_touch_pressed_overlay = None;
        self.reader_touch_pressed_toc = None;
        self.reader_help_controller.clear();
        self.reader_menu_controller.close();
        self.reader_menu_last_rect = None;
        self.system.full_refresh = true;
        self.dirty = true;
    }

    fn set_state_toc(&mut self) {
        self.state = AppState::Toc;
        self.home_about_open = false;
        self.home_about_form.reset();
        self.home_records_open = false;
        self.home_records_form.reset();
        self.home_record_detail_form.reset();
        self.home_record_detail_index = None;
        self.home_menu_controller.close();
        self.home_menu_last_rect = None;
        self.reader_status_bar_focus = None;
        self.reader_touch_pressed_status = None;
        self.reader_touch_pressed_menu = None;
        self.reader_touch_pressed_overlay = None;
        self.reader_touch_pressed_toc = None;
        self.reader_help_controller.clear();
        self.reader_menu_controller.close();
        self.reader_menu_last_rect = None;
        self.dirty = true;
    }

    fn set_state_prc_viewing(&mut self) {
        self.state = AppState::PrcViewing;
        self.home_about_open = false;
        self.home_about_form.reset();
        self.home_records_open = false;
        self.home_records_form.reset();
        self.home_record_detail_form.reset();
        self.home_record_detail_index = None;
        self.home_menu_controller.close();
        self.home_menu_last_rect = None;
        self.prc_soft_menu_focused = false;
        self.prc_soft_menu_last_control = None;
        self.prc_status_bar_focus = None;
        self.prc_touch_pressed_status = None;
        self.prc_status_bar_last_control = None;
        self.prc_reserved_gray_initialized = false;
        self.system.full_refresh = true;
        self.dirty = true;
    }

    fn set_state_error_message(&mut self, message: String) {
        self.error_message = Some(message);
        self.state = AppState::Error;
        self.dirty = true;
    }


    fn draw_start_menu(&mut self, display: &mut impl crate::display::Display) {
        self.ensure_home_system_fonts();
        let recents = self.system.collect_recent_paths(self.last_viewed_entry.as_ref());
        let icons = HomeIcons {
            icon_size: generated_icons::ICON_SIZE as i32,
            folder_dark: generated_icons::ICON_FOLDER_DARK_MASK,
            folder_light: generated_icons::ICON_FOLDER_LIGHT_MASK,
            gear_dark: generated_icons::ICON_GEAR_DARK_MASK,
            gear_light: generated_icons::ICON_GEAR_LIGHT_MASK,
            battery_dark: generated_icons::ICON_BATTERY_DARK_MASK,
            battery_light: generated_icons::ICON_BATTERY_LIGHT_MASK,
        };
        let mut ctx = HomeRenderContext {
            display_buffers: self.display_buffers,
            gray2_lsb: self.gray2_lsb.as_mut_slice(),
            gray2_msb: self.gray2_msb.as_mut_slice(),
            source: self.source,
            full_refresh: self.system.full_refresh,
            battery_percent: self.system.battery_percent,
            render_policy: self.render_policy,
            palm_fonts: self.home_system_fonts.as_slice(),
            icons,
            draw_trbk_image,
        };
        self.home.draw_start_menu(&mut ctx, display, &recents);
        if self.home_about_open {
            self.home_menu_last_rect = None;
            let spec = about_modal_spec(
                build_info::VERSION,
                build_info::BUILD_TIME,
                self.display_buffers.size().width as i32,
            );
            self.home_about_form.sync(&spec);
            let mut form_view = ModalFormView {
                spec: &spec,
                fonts: self.home_system_fonts.as_slice(),
                focused_id: self.home_about_form.focused_id(),
            };
            let mut ui = UiContext {
                buffers: self.display_buffers,
                render_policy: self.render_policy,
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
                RefreshMode::Half,
            );
            flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Half);
            return;
        }
        if let Some(overlay) = self.home_menu_controller.overlay() {
            let rect = palm::ui::draw_menu_overlay_native(
                self.display_buffers,
                overlay.0,
                overlay.1,
                overlay.2,
                self.home_system_fonts.as_slice(),
                0,
                StatusBarView::HEIGHT + 5,
                self.display_buffers.size().width as i32,
            );
            let dirty = if let Some(prev) = self.home_menu_last_rect {
                let x0 = prev.x.min(rect.x);
                let y0 = prev.y.min(rect.y);
                let x1 = (prev.x + prev.w).max(rect.x + rect.w);
                let y1 = (prev.y + prev.h).max(rect.y + rect.h);
                Rect::new(x0, y0, x1 - x0, y1 - y0)
            } else {
                rect
            };
            self.home_menu_last_rect = Some(rect);
            let mut rq = RenderQueue::default();
            rq.push(dirty, RefreshMode::Fast);
            flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Fast);
        } else if let Some(prev) = self.home_menu_last_rect.take() {
            let mut rq = RenderQueue::default();
            rq.push(prev, RefreshMode::Fast);
            flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Fast);
        }
    }

    fn draw_records(&mut self, display: &mut impl crate::display::Display) {
        self.ensure_home_system_fonts();
        self.display_buffers.clear(BinaryColor::On).ok();
        let size = self.display_buffers.size();
        let spec = self.home_records_spec();
        let detail_spec = self.home_record_detail_spec();
        self.home_records_form.sync(&spec);
        let mut ui = UiContext {
            buffers: self.display_buffers,
            render_policy: self.render_policy,
            gray2: None,
        };
        let count_text = format!("{}", self.home_records.len());
        let mut status = StatusBarView {
            battery_percent: self.system.battery_percent,
            right_text: Some(count_text.as_str()),
            home: StatusBarActionState {
                enabled: true,
                focused: self.records_status_bar_focus == Some(StatusBarHit::Home),
            },
            menu: StatusBarActionState {
                enabled: false,
                focused: false,
            },
            palm_fonts: self.home_system_fonts.as_slice(),
        };
        status.render(
            &mut ui,
            Rect::new(0, 0, size.width as i32, StatusBarView::HEIGHT),
            &mut RenderQueue::default(),
        );
        let mut form_view = ModalFormView {
            spec: &spec,
            fonts: self.home_system_fonts.as_slice(),
            focused_id: self.home_records_form.focused_id(),
        };
        form_view.render(&mut ui, spec.bounds, &mut RenderQueue::default());
        if let Some(detail_spec) = detail_spec {
            self.home_record_detail_form.sync(&detail_spec);
            let mut detail_view = ModalFormView {
                spec: &detail_spec,
                fonts: self.home_system_fonts.as_slice(),
                focused_id: self.home_record_detail_form.focused_id(),
            };
            detail_view.render(&mut ui, detail_spec.bounds, &mut RenderQueue::default());
        }
        let mut rq = RenderQueue::default();
        rq.push(
            Rect::new(0, 0, size.width as i32, size.height as i32),
            RefreshMode::Half,
        );
        flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Half);
    }

    fn ensure_home_system_fonts(&mut self) {
        if self.home_system_fonts.is_empty() {
            self.home_system_fonts = self.source.load_home_system_fonts();
            if self.home_system_fonts.is_empty() {
                self.home_system_fonts = self.source.load_prc_system_fonts();
            }
        }
    }


    fn draw_error(&mut self, display: &mut impl crate::display::Display) {
        self.display_buffers.clear(BinaryColor::On).ok();
        const ERROR_MODAL_FORM_ID: u16 = 0x4552;
        const ERROR_MESSAGE_ID: ObjectId = 1;
        const ERROR_HINT_ID: ObjectId = 2;
        const ERROR_OK_ID: ObjectId = 3;

        let width = self.display_buffers.size().width as i32;
        let form_x = 18;
        let form_w = (width - 36).max(1);
        let form_h = 184;
        let form_y = 108;
        let button_w = 96;
        let button_h = 34;
        let button_x = form_x + form_w - button_w - 16;
        let button_y = form_y + form_h - button_h - 14;
        let message = self
            .error_message
            .as_deref()
            .unwrap_or("Unknown error");

        let mut widgets = Vec::new();
        widgets.push(ModalWidget::Label {
            id: ERROR_MESSAGE_ID,
            bounds: Rect::new(form_x + 18, form_y + 56, form_w - 36, 26),
            text: message.to_string(),
            font_id: 0,
        });
        widgets.push(ModalWidget::Label {
            id: ERROR_HINT_ID,
            bounds: Rect::new(form_x + 18, button_y - 30, form_w - 36, 20),
            text: "Press Back or OK".to_string(),
            font_id: 0,
        });
        widgets.push(ModalWidget::Button {
            id: ERROR_OK_ID,
            bounds: Rect::new(button_x, button_y, button_w, button_h),
            text: "OK".to_string(),
            font_id: 0,
            style: 1,
            no_frame: false,
        });

        let spec = ModalFormSpec {
            form_id: ERROR_MODAL_FORM_ID,
            bounds: Rect::new(form_x, form_y, form_w, form_h),
            chrome: crate::ternos::ui::ModalChrome::Alert,
            title: "Error".to_string(),
            widgets,
            default_focus: Some(ERROR_OK_ID),
        };

        let fonts = if !self.home_system_fonts.is_empty() {
            self.home_system_fonts.as_slice()
        } else if !self.prc_system_fonts.is_empty() {
            self.prc_system_fonts.as_slice()
        } else {
            &[]
        };

        if fonts.is_empty() {
            let header_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
            Text::new("Error", Point::new(LIST_MARGIN_X, HEADER_Y), header_style)
                .draw(self.display_buffers)
                .ok();
            Text::new(message, Point::new(LIST_MARGIN_X, 60), header_style)
                .draw(self.display_buffers)
                .ok();
            Text::new(
                "Press Back to return",
                Point::new(LIST_MARGIN_X, 100),
                header_style,
            )
            .draw(self.display_buffers)
            .ok();
        } else {
            let mut controller = ModalFormController::default();
            controller.sync(&spec);
            let mut ui = UiContext {
                buffers: self.display_buffers,
                render_policy: self.render_policy,
                gray2: None,
            };
            let mut rq = RenderQueue::default();
            ModalFormView {
                spec: &spec,
                fonts,
                focused_id: controller.focused_id(),
            }
            .render(&mut ui, spec.bounds, &mut rq);
        }
        let size = self.display_buffers.size();
        let mut rq = RenderQueue::default();
        rq.push(
            Rect::new(0, 0, size.width as i32, size.height as i32),
            RefreshMode::Full,
        );
        flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Full);
    }

    fn draw_prc_viewer(&mut self, display: &mut impl crate::display::Display) {
        const STATUS_H: i32 = StatusBarView::HEIGHT;
        self.ensure_home_system_fonts();
        self.display_buffers.clear(BinaryColor::On).ok();
        let size = self.display_buffers.size();
        let mut shell_ui = UiContext {
            buffers: self.display_buffers,
            render_policy: self.render_policy,
            gray2: None,
        };
        let mut status_bar = StatusBarView::new(self.home_system_fonts.as_slice());
        status_bar.battery_percent = self.system.battery_percent;
        status_bar.home = StatusBarActionState {
            enabled: true,
            focused: self.prc_status_bar_focus == Some(PrcStatusBarFocus::Home),
        };
        status_bar.menu = StatusBarActionState {
            enabled: self.prc_menu_controller.menu_count() > 0,
            focused: self.prc_status_bar_focus == Some(PrcStatusBarFocus::Menu),
        };
        status_bar.render(
            &mut shell_ui,
            Rect::new(0, 0, size.width as i32, STATUS_H),
            &mut RenderQueue::default(),
        );

        let active_form = self
            .runtime_prc_form()
            .or_else(|| self.prc_forms.get(self.prc_form_index).cloned())
            .or_else(|| self.prc_forms.first().cloned());
        if let Some(form) = active_form {
            let outline = PrimitiveStyle::with_stroke(BinaryColor::Off, 1);
            let clear = PrimitiveStyle::with_fill(BinaryColor::On);
            let max_scale_w = ((size.width as i32) / 160).max(1);
            let content_top = STATUS_H + 5;
            let content_h = (size.height as i32 - content_top).max(1);
            let max_scale_h = (content_h / 160).max(1);
            let max_scale = max_scale_w.min(max_scale_h).max(1);
            let scale = if max_scale >= 3 { 3 } else { max_scale };
            let pane_w = 160 * scale;
            let pane_h = 160 * scale;
            let pane_x = ((size.width as i32 - pane_w) / 2).max(0);
            let pane_y = content_top;
            Rectangle::new(
                Point::new(pane_x, pane_y),
                Size::new(pane_w as u32, pane_h as u32),
            )
            .into_styled(clear)
            .draw(self.display_buffers)
            .ok();
            let dialog_framed = form.frame_type != 0 || (form.window_flags & 0x2000) != 0;
            if dialog_framed
                && let Some(underlay_id) = self.prc_runtime_underlay_form_id
                && let Some(underlay_form) = self.prc_form_by_id(underlay_id)
            {
                palm::ui::draw_form_preview(
                    self.display_buffers,
                    &underlay_form,
                    &self.prc_system_fonts,
                    &self.prc_bitmaps,
                    &self.prc_runtime_bitmap_draws,
                    &self.prc_runtime_button_labels,
                    &self.prc_runtime_selected_controls,
                    &self.prc_runtime_field_draws,
                    &self.prc_runtime_table_draws,
                    None,
                    None,
                    None,
                    None,
                    None,
                    pane_x,
                    pane_y,
                    pane_w,
                    pane_h,
                    scale.max(1),
                    true,
                    outline,
                );
            }
            palm::ui::draw_form_preview(
                self.display_buffers,
                &form,
                &self.prc_system_fonts,
                &self.prc_bitmaps,
                &self.prc_runtime_bitmap_draws,
                &self.prc_runtime_button_labels,
                &self.prc_runtime_selected_controls,
                &self.prc_runtime_field_draws,
                &self.prc_runtime_table_draws,
                if self.prc_status_bar_focus.is_some() {
                    None
                } else {
                    self.prc_ui_controller.focused_control_id()
                },
                self.prc_runtime_focused_field_id,
                self.prc_menu_controller.overlay(),
                self.prc_session
                    .as_ref()
                    .and_then(|session| session.help_dialog())
                    .as_ref(),
                self.prc_session
                    .as_ref()
                    .and_then(|session| session.help_dialog())
                    .as_ref()
                    .map(|_| self.prc_help_controller.focused_control()),
                pane_x,
                pane_y,
                pane_w,
                pane_h,
                scale.max(1),
                !dialog_framed,
                outline,
            );
        }

        let content_top = STATUS_H + 5;
        let max_scale_w = ((size.width as i32) / 160).max(1);
        let max_scale_h = ((size.height as i32 - content_top) / 160).max(1);
        let max_scale = max_scale_w.min(max_scale_h).max(1);
        let scale = if max_scale >= 3 { 3 } else { max_scale };
        let mode = self.render_policy.refresh_mode(self.system.full_refresh);
        let mut rq = RenderQueue::default();
        let pane_h = 160 * scale;
        let update_h = (content_top + pane_h).clamp(0, size.height as i32);
        rq.push(Rect::new(0, 0, size.width as i32, update_h), mode);
        flush_queue(display, self.display_buffers, &mut rq, mode);
    }

    pub fn draw_usb_modal(
        &mut self,
        display: &mut impl crate::display::Display,
        title: &str,
        message: &str,
        status: Option<&str>,
        footer: &str,
    ) {
        self.display_buffers.clear(BinaryColor::On).ok();
        const USB_MODAL_FORM_ID: u16 = 0x5542;
        const USB_MODAL_STATUS_ID: ObjectId = 1;
        const USB_MODAL_MESSAGE_ID: ObjectId = 2;
        const USB_MODAL_FOOTER_ID: ObjectId = 3;
        const USB_MODAL_DISCONNECT_ID: ObjectId = 4;

        let width = self.display_buffers.size().width as i32;
        let form_x = 12;
        let form_w = (width - 24).max(1);
        let form_h = if status.is_some() { 244 } else { 212 };
        let form_y = 74;
        let button_w = 132;
        let button_h = 34;
        let button_x = form_x + form_w - button_w - 16;
        let button_y = form_y + form_h - button_h - 14;

        let mut widgets = Vec::new();
        widgets.push(ModalWidget::Label {
            id: USB_MODAL_MESSAGE_ID,
            bounds: Rect::new(form_x + 18, form_y + 52, form_w - 36, 28),
            text: message.to_string(),
            font_id: 0,
        });
        widgets.push(ModalWidget::Label {
            id: USB_MODAL_FOOTER_ID,
            bounds: Rect::new(form_x + 18, button_y - 34, form_w - 36, 24),
            text: footer.to_string(),
            font_id: 0,
        });
        widgets.push(ModalWidget::Button {
            id: USB_MODAL_DISCONNECT_ID,
            bounds: Rect::new(button_x, button_y, button_w, button_h),
            text: "Disconnect".to_string(),
            font_id: 0,
            style: 1,
            no_frame: false,
        });
        if let Some(status) = status {
            widgets.push(ModalWidget::Label {
                id: USB_MODAL_STATUS_ID,
                bounds: Rect::new(form_x + 18, form_y + 86, form_w - 36, 44),
                text: status.to_string(),
                font_id: 0,
            });
        }

        let spec = ModalFormSpec {
            form_id: USB_MODAL_FORM_ID,
            bounds: Rect::new(form_x, form_y, form_w, form_h),
            chrome: crate::ternos::ui::ModalChrome::Alert,
            title: title.to_string(),
            widgets,
            default_focus: Some(USB_MODAL_DISCONNECT_ID),
        };
        let fonts = if !self.home_system_fonts.is_empty() {
            self.home_system_fonts.as_slice()
        } else if !self.prc_system_fonts.is_empty() {
            self.prc_system_fonts.as_slice()
        } else {
            &[]
        };
        if fonts.is_empty() {
            let header_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
            let outline = PrimitiveStyle::with_stroke(BinaryColor::Off, 2);
            let clear = PrimitiveStyle::with_fill(BinaryColor::On);
            Rectangle::new(
                Point::new(form_x, form_y),
                Size::new(form_w as u32, form_h as u32),
            )
            .into_styled(clear)
            .draw(self.display_buffers)
            .ok();
            Rectangle::new(
                Point::new(form_x, form_y),
                Size::new(form_w as u32, form_h as u32),
            )
            .into_styled(outline)
            .draw(self.display_buffers)
            .ok();
            Text::new(title, Point::new(form_x + 18, form_y + 30), header_style)
                .draw(self.display_buffers)
                .ok();
            Text::new(message, Point::new(form_x + 18, form_y + 68), header_style)
                .draw(self.display_buffers)
                .ok();
            if let Some(status) = status {
                Text::new(status, Point::new(form_x + 18, form_y + 98), header_style)
                    .draw(self.display_buffers)
                    .ok();
            }
            Text::new(footer, Point::new(form_x + 18, button_y - 12), header_style)
                .draw(self.display_buffers)
                .ok();
            Rectangle::new(
                Point::new(button_x, button_y),
                Size::new(button_w as u32, button_h as u32),
            )
            .into_styled(outline)
            .draw(self.display_buffers)
            .ok();
            Text::new(
                "Disconnect",
                Point::new(button_x + 10, button_y + 22),
                header_style,
            )
            .draw(self.display_buffers)
            .ok();
        } else {
            let mut controller = ModalFormController::default();
            controller.sync(&spec);
            let mut ui = UiContext {
                buffers: self.display_buffers,
                render_policy: self.render_policy,
                gray2: None,
            };
            let mut rq = RenderQueue::default();
            ModalFormView {
                spec: &spec,
                fonts,
                focused_id: controller.focused_id(),
            }
            .render(&mut ui, spec.bounds, &mut rq);
        }
        display.display(self.display_buffers, RefreshMode::Full);
    }


    fn draw_image_viewer(&mut self, display: &mut impl crate::display::Display) {
        self.ensure_gray2_buffers();
        let mut ctx = ImageViewerContext {
            display_buffers: self.display_buffers,
            gray2_lsb: self.gray2_lsb.as_mut_slice(),
            gray2_msb: self.gray2_msb.as_mut_slice(),
            source: self.source,
            wake_restore_only: &mut self.system.wake_restore_only,
            render_policy: self.render_policy,
        };
        if let Err(err) = self.image_viewer.draw(&mut ctx, display) {
            self.set_error(err);
        }
    }



    fn draw_book_reader(&mut self, display: &mut impl crate::display::Display) {
        self.ensure_home_system_fonts();
        self.ensure_gray2_buffers();
        let mut ctx = BookReaderContext {
            display_buffers: self.display_buffers,
            gray2_lsb: self.gray2_lsb.as_mut_slice(),
            gray2_msb: self.gray2_msb.as_mut_slice(),
            source: self.source,
            full_refresh: &mut self.system.full_refresh,
            render_policy: self.render_policy,
            battery_percent: self.system.battery_percent,
            palm_fonts: self.home_system_fonts.as_slice(),
        };
        let home_focused = self.reader_status_bar_focus == Some(ReaderStatusBarFocus::Home);
        let menu_focused = self.reader_status_bar_focus == Some(ReaderStatusBarFocus::Menu);
        if let Some(overlay) = self.reader_menu_controller.overlay() {
            if let Err(err) = self.book_reader.render_book_frame(&mut ctx, home_focused, menu_focused) {
                self.set_error(err);
                return;
            }
            let rect = draw_reader_menu_overlay(
                self.display_buffers,
                self.home_system_fonts.as_slice(),
                overlay,
            );
            let dirty = if let Some(prev) = self.reader_menu_last_rect {
                let x0 = prev.x.min(rect.x);
                let y0 = prev.y.min(rect.y);
                let x1 = (prev.x + prev.w).max(rect.x + rect.w);
                let y1 = (prev.y + prev.h).max(rect.y + rect.h);
                Rect::new(x0, y0, x1 - x0, y1 - y0)
            } else {
                rect
            };
            self.reader_menu_last_rect = Some(rect);
            let mut rq = RenderQueue::default();
            rq.push(dirty, RefreshMode::Fast);
            flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Fast);
        } else if self.book_reader.has_overlay() {
            self.reader_menu_last_rect = None;
            if let Err(err) = self.book_reader.render_book_frame(&mut ctx, home_focused, menu_focused) {
                self.set_error(err);
                return;
            }
            let help_focus = self.book_reader.help_dialog().map(|dialog| {
                self.reader_help_controller.sync(&dialog);
                self.reader_help_controller.focused_control()
            });
            draw_reader_overlay(
                &mut self.book_reader,
                &mut ctx,
                display,
                help_focus,
            );
        } else if let Some(prev) = self.reader_menu_last_rect.take() {
            if let Err(err) = self.book_reader.render_book_frame(&mut ctx, home_focused, menu_focused) {
                self.set_error(err);
                return;
            }
            let mut rq = RenderQueue::default();
            rq.push(prev, RefreshMode::Fast);
            flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Fast);
        } else if let Err(err) = self.book_reader.draw_book(
            &mut ctx,
            display,
            home_focused,
            menu_focused,
        ) {
            self.set_error(err);
        }
    }

    fn draw_toc_view(&mut self, display: &mut impl crate::display::Display) {
        self.ensure_home_system_fonts();
        self.ensure_gray2_buffers();
        let mut ctx = BookReaderContext {
            display_buffers: self.display_buffers,
            gray2_lsb: self.gray2_lsb.as_mut_slice(),
            gray2_msb: self.gray2_msb.as_mut_slice(),
            source: self.source,
            full_refresh: &mut self.system.full_refresh,
            render_policy: self.render_policy,
            battery_percent: self.system.battery_percent,
            palm_fonts: self.home_system_fonts.as_slice(),
        };
        let home_focused = self.reader_status_bar_focus == Some(ReaderStatusBarFocus::Home);
        let menu_focused = self.reader_status_bar_focus == Some(ReaderStatusBarFocus::Menu);
        if let Some(overlay) = self.reader_menu_controller.overlay() {
            if let Err(err) = self.book_reader.draw_toc(
                &mut ctx,
                display,
                home_focused,
                menu_focused,
            ) {
                self.set_error(err);
                return;
            }
            let rect = draw_reader_menu_overlay(
                self.display_buffers,
                self.home_system_fonts.as_slice(),
                overlay,
            );
            let dirty = if let Some(prev) = self.reader_menu_last_rect {
                let x0 = prev.x.min(rect.x);
                let y0 = prev.y.min(rect.y);
                let x1 = (prev.x + prev.w).max(rect.x + rect.w);
                let y1 = (prev.y + prev.h).max(rect.y + rect.h);
                Rect::new(x0, y0, x1 - x0, y1 - y0)
            } else {
                rect
            };
            self.reader_menu_last_rect = Some(rect);
            let mut rq = RenderQueue::default();
            rq.push(dirty, RefreshMode::Fast);
            flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Fast);
        } else if self.book_reader.has_overlay() {
            self.reader_menu_last_rect = None;
            if let Err(err) = self.book_reader.draw_toc(
                &mut ctx,
                display,
                home_focused,
                menu_focused,
            ) {
                self.set_error(err);
                return;
            }
            let help_focus = self.book_reader.help_dialog().map(|dialog| {
                self.reader_help_controller.sync(&dialog);
                self.reader_help_controller.focused_control()
            });
            draw_reader_overlay(
                &mut self.book_reader,
                &mut ctx,
                display,
                help_focus,
            );
        } else if let Some(prev) = self.reader_menu_last_rect.take() {
            if let Err(err) = self.book_reader.draw_toc(
                &mut ctx,
                display,
                home_focused,
                menu_focused,
            ) {
                self.set_error(err);
                return;
            }
            let mut rq = RenderQueue::default();
            rq.push(prev, RefreshMode::Fast);
            flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Fast);
        } else {
            self.reader_menu_last_rect = None;
            if let Err(err) = self.book_reader.draw_toc(
                &mut ctx,
                display,
                home_focused,
                menu_focused,
            ) {
                self.set_error(err);
            }
        }
    }


    fn draw_page_turn_indicator(
        &mut self,
        display: &mut impl crate::display::Display,
        indicator: PageTurnIndicator,
    ) {
        let size = self.display_buffers.size();
        // Ensure we draw over the last displayed frame (active buffer may be stale).
        let inactive = *self.display_buffers.get_inactive_buffer();
        self.display_buffers
            .get_active_buffer_mut()
            .copy_from_slice(&inactive);
        let symbol = match indicator {
            PageTurnIndicator::Forward => ">",
            PageTurnIndicator::Backward => "<",
        };
        let text_w = (symbol.len() as i32) * 10;
        let x = match indicator {
            PageTurnIndicator::Forward => (size.width as i32 - PAGE_INDICATOR_MARGIN - text_w)
                .max(PAGE_INDICATOR_MARGIN),
            PageTurnIndicator::Backward => PAGE_INDICATOR_MARGIN,
        };
        let y = PAGE_INDICATOR_Y;
        let style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        Text::new(symbol, Point::new(x, y), style)
            .draw(self.display_buffers)
            .ok();
        Text::new(symbol, Point::new(x + 1, y), style)
            .draw(self.display_buffers)
            .ok();

        let mut rq = RenderQueue::default();
        rq.push(Rect::new(x - 2, y - 2, text_w + 4, 22), RefreshMode::Fast);
        flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Fast);
    }

    fn draw_sleeping_indicator(&mut self, display: &mut impl crate::display::Display) {
        let size = self.display_buffers.size();
        // Ensure we draw over the last displayed frame.
        let inactive = *self.display_buffers.get_inactive_buffer();
        self.display_buffers
            .get_active_buffer_mut()
            .copy_from_slice(&inactive);

        let text = "Zz";
        let text_w = (text.len() as i32) * 10;
        let x = (size.width as i32 - PAGE_INDICATOR_MARGIN - text_w)
            .max(PAGE_INDICATOR_MARGIN);
        let y = PAGE_INDICATOR_Y;
        let style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        Text::new(text, Point::new(x, y), style)
            .draw(self.display_buffers)
            .ok();
        Text::new(text, Point::new(x + 1, y), style)
            .draw(self.display_buffers)
            .ok();

        let mut rq = RenderQueue::default();
        rq.push(Rect::new(x - 2, y - 2, text_w + 4, 22), RefreshMode::Fast);
        flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Fast);
    }

    fn draw_exiting_overlay(&mut self, display: &mut impl crate::display::Display) {
        let size = self.display_buffers.size();
        let text = "Exiting...";
        let text_w = (text.len() as i32) * 10;
        let padding_x = 10;
        let padding_y = 6;
        let rect_w = text_w + (padding_x * 2);
        let rect_h = 20 + (padding_y * 2);
        let x = (size.width as i32 - rect_w) / 2;
        let y = (size.height as i32 - rect_h) / 2;

        embedded_graphics::primitives::Rectangle::new(
            Point::new(x, y),
            embedded_graphics::geometry::Size::new(rect_w as u32, rect_h as u32),
        )
        .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
            BinaryColor::Off,
        ))
        .draw(self.display_buffers)
        .ok();
        let text_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        Text::new(text, Point::new(x + padding_x, y + 20), text_style)
            .draw(self.display_buffers)
            .ok();

        let mut rq = RenderQueue::default();
        rq.push(Rect::new(x, y, rect_w, rect_h), RefreshMode::Fast);
        flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Fast);
    }

    fn draw_sleep_overlay(&mut self, display: &mut impl crate::display::Display) {
        let logo = SleepWallpaperIcons {
            logo_w: generated_icons::LOGO_WIDTH as i32,
            logo_h: generated_icons::LOGO_HEIGHT as i32,
            logo_dark: generated_icons::LOGO_DARK_MASK,
            logo_light: generated_icons::LOGO_LIGHT_MASK,
        };
        let is_start_menu = self.state == AppState::StartMenu;
        let last_viewed_entry = &self.last_viewed_entry;
        let mut ctx = SystemRenderContext {
            display_buffers: self.display_buffers,
            gray2_lsb: self.gray2_lsb.as_mut_slice(),
            gray2_msb: self.gray2_msb.as_mut_slice(),
            render_policy: self.render_policy,
            source: self.source,
            image_viewer: &mut self.image_viewer,
            book_reader: &mut self.book_reader,
            last_viewed_entry,
            is_start_menu,
            logo,
        };
        self.system.process_sleep_overlay(&mut ctx, display);
    }

    fn try_resume(&mut self) {
        let outcome = self.system.try_resume();
        let outcome = self
            .system
            .apply_resume(outcome, self.source);
        match outcome {
            ApplyResumeOutcome::None => {}
            ApplyResumeOutcome::Missing => {}
            ApplyResumeOutcome::Ready {
                path,
                page,
                refreshed,
            } => {
                if refreshed {
                    self.image_viewer.clear();
                    self.book_reader.clear();
                    self.state = AppState::StartMenu;
                    self.error_message = None;
                    self.dirty = true;
                }
                let _ = self.open_path(&path);
                if let Some(page) = page
                    && let Some(book) = &self.book_reader.current_book
                    && page < book.page_count
                {
                    self.book_reader.current_page = page;
                    self.book_reader.current_page_ops =
                        self.source.trbk_page(self.book_reader.current_page).ok();
                    self.system.full_refresh = true;
                    self.book_reader.book_turns_since_full = 0;
                    self.dirty = true;
                }
            }
        }
    }

    pub fn idle_ms(&self) -> u32 {
        self.system.idle_ms
    }

    pub fn is_sleeping_pending(&self) -> bool {
        self.state == AppState::SleepingPending
    }

    pub fn is_sleeping(&self) -> bool {
        self.state == AppState::Sleeping
    }

    fn start_sleep_request(&mut self) {
        if self.state == AppState::Sleeping || self.state == AppState::SleepingPending {
            return;
        }
        self.system.start_sleep_request(self.state == AppState::StartMenu);
        self.state = AppState::SleepingPending;
        self.dirty = true;
    }
}
