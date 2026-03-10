extern crate alloc;

use alloc::{format, string::{String, ToString}};
use alloc::vec::Vec;
use alloc::vec;

use embedded_graphics::{
    Drawable,
    Pixel,
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, OriginDimensions, Point, Primitive, Size},
    primitives::{Circle, PrimitiveStyle, Rectangle},
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
        book_reader::{draw_trbk_image, BookReaderContext, BookReaderState, PageTurnIndicator},
        home::{
            HomeAction,
            HomeIcons,
            HomeOpen,
            HomeOpenError,
            HomeRenderContext,
            HomeState,
            MenuAction,
        },
        image_viewer::{ImageViewerContext, ImageViewerState},
        settings::{draw_settings, SettingsContext},
        system::{ApplyResumeOutcome, ResumeContext, SleepWallpaperIcons, SystemRenderContext, SystemState},
    },
    build_info,
    display::{GrayscaleMode, RefreshMode},
    framebuffer::{DisplayBuffers, Rotation},
    image_viewer::{AppSource, EntryKind, ImageEntry, ImageError},
    input,
    prc_app,
    ui::{flush_queue, Rect, RenderQueue},
};

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
    prc_forms: Vec<prc_app::form_preview::FormPreview>,
    prc_bitmaps: Vec<prc_app::bitmap::PrcBitmap>,
    prc_runtime_form_id: Option<u16>,
    prc_ui_controller: prc_app::controller::PrcUiController,
    prc_runtime_bitmap_draws: Vec<prc_app::runner::RuntimeBitmapDraw>,
    prc_runtime_field_draws: Vec<prc_app::runner::RuntimeFieldDraw>,
    prc_runtime_table_draws: Vec<prc_app::runner::RuntimeTableDraw>,
    prc_system_fonts: Vec<prc_app::runtime::PalmFont>,
    home_system_fonts: Vec<prc_app::runtime::PalmFont>,
    prc_menu_controller: prc_app::controller::PrcMenuController,
    prc_help_controller: prc_app::controller::PrcHelpDialogController,
    prc_active_entry: Option<ImageEntry>,
    prc_session: Option<prc_app::runner::PrcRuntimeSession>,
    prc_blocked_timeout_ticks: u32,
    prc_blocked_elapsed_ms: u32,
    prc_soft_menu_focused: bool,
    prc_soft_menu_last_control: Option<u16>,
    prc_return_to_start_menu: bool,
    prc_reserved_gray_initialized: bool,
    install_scan_elapsed_ms: u32,
    install_last_summary: Option<(u32, u32, u32, u32, u32)>,
    gray2_lsb: Vec<u8>,
    gray2_msb: Vec<u8>,
    exit_from: ExitFrom,
    exit_overlay_drawn: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AppState {
    StartMenu,
    Settings,
    Menu,
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
    fn best_prc_form_index(&self) -> Option<usize> {
        self.prc_forms.iter().enumerate().max_by_key(|(_, f)| {
            let area = (f.w.max(0) as i32) * (f.h.max(0) as i32);
            let objs = f.objects.len() as i32;
            area.saturating_mul(4).saturating_add(objs.saturating_mul(100))
        }).map(|(idx, _)| idx)
    }

    fn runtime_prc_form(&self) -> Option<prc_app::form_preview::FormPreview> {
        let fid = self.prc_runtime_form_id?;
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

    fn draw_prc_soft_menu_button(&mut self, rect: Rect) {
        let focused = self.prc_soft_menu_focused;
        let circle_d = (rect.w - 12).max(24);
        let cx = rect.x + (rect.w - circle_d) / 2;
        let cy = rect.y + (rect.h - circle_d) / 2;
        let _ = Circle::new(Point::new(cx, cy), circle_d as u32)
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(self.display_buffers);
        let _ = Circle::new(Point::new(cx, cy), circle_d as u32)
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 1))
            .draw(self.display_buffers);

        let mut draw_symbol_px = |x: i32, y: i32| {
            if focused || ((x + y) & 1) == 0 {
                let _ = Pixel(Point::new(x, y), BinaryColor::Off).draw(self.display_buffers);
            }
        };

        let lx = cx + 9;
        let ly = cy + 10;
        for x in (lx - 2)..(lx + 20) {
            draw_symbol_px(x, ly - 5);
        }
        for i in 0..3 {
            let y = ly + i * 9;
            for x in lx..(lx + 14) {
                draw_symbol_px(x, y);
                draw_symbol_px(x, y + 1);
            }
            draw_symbol_px(lx, y + 4);
            draw_symbol_px(lx + 1, y + 4);
            draw_symbol_px(lx, y + 5);
            draw_symbol_px(lx + 1, y + 5);
        }

        let ax = cx + circle_d - 17;
        let ay = cy + 17;
        for y in ay..(ay + 12) {
            draw_symbol_px(ax + 2, y);
            draw_symbol_px(ax + 3, y);
        }
        for row in 0..5 {
            let start = ax - row;
            let w = 6 + row * 2;
            let y = ay + 12 + row;
            for x in start..(start + w) {
                draw_symbol_px(x, y);
            }
        }
    }

    pub fn new(display_buffers: &'a mut DisplayBuffers, source: &'a mut S) -> Self {
        display_buffers.set_rotation(Rotation::Rotate90);
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
            prc_ui_controller: prc_app::controller::PrcUiController::default(),
            prc_runtime_bitmap_draws: Vec::new(),
            prc_runtime_field_draws: Vec::new(),
            prc_runtime_table_draws: Vec::new(),
            prc_system_fonts: Vec::new(),
            home_system_fonts: Vec::new(),
            prc_menu_controller: prc_app::controller::PrcMenuController::default(),
            prc_help_controller: prc_app::controller::PrcHelpDialogController::default(),
            prc_active_entry: None,
            prc_session: None,
            prc_blocked_timeout_ticks: 0,
            prc_blocked_elapsed_ms: 0,
            prc_soft_menu_focused: false,
            prc_soft_menu_last_control: None,
            prc_return_to_start_menu: false,
            prc_reserved_gray_initialized: false,
            install_scan_elapsed_ms: 0,
            install_last_summary: None,
            gray2_lsb: vec![0u8; crate::framebuffer::BUFFER_SIZE],
            gray2_msb: vec![0u8; crate::framebuffer::BUFFER_SIZE],
            exit_from: ExitFrom::Image,
            exit_overlay_drawn: false,
        };
        app.refresh_entries();
        app.try_resume();
        app
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
            let mut resumed_viewer = false;
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
                resumed_viewer = true;
            } else {
                self.set_state_start_menu(true);
            }
            self.system.on_wake();
            self.dirty = true;
            if !resumed_viewer {
                self.refresh_entries();
            }
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

        match self.state {
            AppState::StartMenu => {
                let recents = self.system.collect_recent_paths(self.last_viewed_entry.as_ref());
                match self.home.handle_start_menu_input(&recents, buttons) {
                    HomeAction::OpenRecent(path) => {
                        if path.to_ascii_lowercase().ends_with(".tdb")
                            || path.to_ascii_lowercase().ends_with(".prc")
                        {
                            if let Err(err) = self.open_prc_path(&path) {
                                self.set_error(err);
                            }
                        } else {
                            match self.home.open_recent_path(self.source, &path) {
                                Ok(()) => {
                                    let index = self.home.selected;
                                    self.open_index(index);
                                }
                                Err(err) => {
                                    if self.system.remove_recent(&path) {
                                        if self.last_viewed_entry.as_deref() == Some(path.as_str())
                                        {
                                            self.last_viewed_entry = None;
                                        }
                                        self.system.save_recent_entries_now(self.source);
                                    }
                                    self.set_error(err);
                                }
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
                if !Self::has_input(buttons) {
                    if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                }
            }
            AppState::Menu => {
                match self.home.handle_menu_input(buttons) {
                    MenuAction::OpenSelected => {
                        self.open_selected();
                    }
                    MenuAction::Back => {
                        if !self.home.path.is_empty() {
                            self.home.path.pop();
                            self.refresh_entries();
                        } else {
                            self.set_state_start_menu(true);
                        }
                    }
                    MenuAction::Dirty => {
                        self.dirty = true;
                    }
                    MenuAction::None => {
                        if self.system.add_idle(elapsed_ms) {
                            self.start_sleep_request();
                        }
                    }
                }
            }
            AppState::Settings => {
                if buttons.is_pressed(input::Buttons::Back)
                    || buttons.is_pressed(input::Buttons::Confirm)
                {
                    self.set_state_start_menu(true);
                } else {
                    if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                }
            }
            AppState::Viewing => {
                if buttons.is_pressed(input::Buttons::Left) {
                    if !self.home.entries.is_empty() {
                        let next = self.home.selected.saturating_sub(1);
                        self.open_index(next);
                    }
                } else if buttons.is_pressed(input::Buttons::Right) {
                    if !self.home.entries.is_empty() {
                        let next = (self.home.selected + 1).min(self.home.entries.len() - 1);
                        self.open_index(next);
                    }
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
                let result = self
                    .book_reader
                    .handle_view_input(self.source, buttons);
                if result.exit {
                    self.exit_from = ExitFrom::Book;
                    self.exit_overlay_drawn = false;
                    self.state = AppState::ExitingPending;
                    self.dirty = true;
                } else if result.open_toc {
                    self.set_state_toc();
                } else if result.dirty {
                    self.dirty = true;
                } else {
                    if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                }
            }
            AppState::Toc => {
                let result = self.book_reader.handle_toc_input(buttons);
                if result.exit {
                    self.set_state_book_viewing();
                } else if result.jumped {
                    self.set_state_book_viewing();
                } else if result.dirty {
                    self.dirty = true;
                } else {
                    if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                }
            }
            AppState::PrcViewing => {
                {
                    let form = self.runtime_prc_form();
                    if self.prc_ui_controller.sync_with_form(form.as_ref()) {
                        self.dirty = true;
                    }
                }
                if self
                    .prc_session
                    .as_ref()
                    .map(|s| s.has_help_dialog())
                    .unwrap_or(false)
                {
                    let event = if buttons.is_pressed(input::Buttons::Up) {
                        Some(prc_app::ui_component::UiNavEvent::Up)
                    } else if buttons.is_pressed(input::Buttons::Down) {
                        Some(prc_app::ui_component::UiNavEvent::Down)
                    } else if buttons.is_pressed(input::Buttons::Back) {
                        Some(prc_app::ui_component::UiNavEvent::Back)
                    } else if buttons.is_pressed(input::Buttons::Confirm) {
                        Some(prc_app::ui_component::UiNavEvent::Confirm)
                    } else {
                        None
                    };
                    if let Some(event) = event {
                        match self.prc_help_controller.on_event(event) {
                            prc_app::controller::HelpDialogAction::Scroll(delta) => {
                                if let Some(session) = self.prc_session.as_mut() {
                                    if session.scroll_help_dialog(delta) {
                                        self.dirty = true;
                                    }
                                }
                            }
                            prc_app::controller::HelpDialogAction::Dismiss => {
                                if let Some(session) = self.prc_session.as_mut() {
                                    let _ = session.dismiss_help_dialog();
                                    self.resume_prc_runtime_session();
                                }
                            }
                            prc_app::controller::HelpDialogAction::None => {}
                        }
                    } else if self.system.add_idle(elapsed_ms) {
                        self.start_sleep_request();
                    }
                    return;
                }
                if self.prc_menu_controller.is_active() {
                    let event = if buttons.is_pressed(input::Buttons::Back) {
                        Some(prc_app::ui_component::UiNavEvent::Back)
                    } else if buttons.is_pressed(input::Buttons::Left) {
                        Some(prc_app::ui_component::UiNavEvent::Left)
                    } else if buttons.is_pressed(input::Buttons::Right) {
                        Some(prc_app::ui_component::UiNavEvent::Right)
                    } else if buttons.is_pressed(input::Buttons::Up) {
                        Some(prc_app::ui_component::UiNavEvent::Up)
                    } else if buttons.is_pressed(input::Buttons::Down) {
                        Some(prc_app::ui_component::UiNavEvent::Down)
                    } else if buttons.is_pressed(input::Buttons::Confirm) {
                        Some(prc_app::ui_component::UiNavEvent::Confirm)
                    } else {
                        None
                    };
                    if let Some(event) = event {
                        match self.prc_menu_controller.on_event(event) {
                            prc_app::controller::MenuAction::Activate(item_id) => {
                                if let Some(session) = self.prc_session.as_mut() {
                                    session.inject_event_now(
                                        prc_app::runtime::EVT_MENU,
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
                            prc_app::controller::MenuAction::Redraw
                            | prc_app::controller::MenuAction::Closed => {
                                self.dirty = true;
                            }
                            prc_app::controller::MenuAction::None => {}
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
                if buttons.is_pressed(input::Buttons::Left) {
                    if !self.prc_soft_menu_focused {
                        let form = self.runtime_prc_form();
                        if self.prc_ui_controller.move_focus_direction(
                            form.as_ref(),
                            prc_app::controller::FocusDirection::Left,
                        ) {
                            self.dirty = true;
                        }
                    }
                } else if buttons.is_pressed(input::Buttons::Right) {
                    if !self.prc_soft_menu_focused {
                        let form = self.runtime_prc_form();
                        if self.prc_ui_controller.move_focus_direction(
                            form.as_ref(),
                            prc_app::controller::FocusDirection::Right,
                        ) {
                            self.dirty = true;
                        }
                    }
                } else if buttons.is_pressed(input::Buttons::Up) {
                    let form = self.runtime_prc_form();
                    if self.prc_soft_menu_focused {
                        self.prc_soft_menu_focused = false;
                        let restored = self
                            .prc_soft_menu_last_control
                            .and_then(|id| {
                                if self.prc_ui_controller.select_control_id(form.as_ref(), id) {
                                    Some(())
                                } else {
                                    None
                                }
                            })
                            .is_some();
                        if !restored {
                            let _ = self.prc_ui_controller.move_focus_direction(
                                form.as_ref(),
                                prc_app::controller::FocusDirection::Up,
                            );
                        }
                        self.dirty = true;
                    } else if self.prc_ui_controller.move_focus_direction(
                        form.as_ref(),
                        prc_app::controller::FocusDirection::Up,
                    ) {
                        self.dirty = true;
                    }
                } else if buttons.is_pressed(input::Buttons::Down) {
                    let form = self.runtime_prc_form();
                    if self.prc_soft_menu_focused {
                        // Single soft button in this bar; nothing further down.
                    } else if self.prc_ui_controller.move_focus_direction(
                        form.as_ref(),
                        prc_app::controller::FocusDirection::Down,
                    ) {
                        self.dirty = true;
                    } else {
                        self.prc_soft_menu_last_control = self.prc_ui_controller.focused_control_id();
                        self.prc_soft_menu_focused = true;
                        self.dirty = true;
                    }
                } else if buttons.is_pressed(input::Buttons::Confirm) {
                    if self.prc_soft_menu_focused {
                        if self.prc_menu_controller.open() {
                            self.dirty = true;
                        }
                    } else {
                        if let (Some(control_id), Some(session)) =
                            (self.prc_ui_controller.focused_control_id(), self.prc_session.as_mut())
                        {
                            session.inject_control_select_now(control_id);
                            self.prc_blocked_elapsed_ms = 0;
                            self.prc_blocked_timeout_ticks = 0;
                            self.resume_prc_runtime_session();
                        } else {
                            self.set_state_menu();
                        }
                    }
                } else if buttons.is_pressed(input::Buttons::Back) {
                    self.exit_prc_viewer_to_origin();
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

    pub fn draw(&mut self, display: &mut impl crate::display::Display) {
        if !self.dirty {
            return;
        }

        self.dirty = false;
        match self.state {
            AppState::StartMenu => self.draw_start_menu(display),
            AppState::Settings => self.draw_settings(display),
            AppState::Menu => self.draw_menu(display),
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
                    "state={:?} current_entry={:?} last_viewed_entry={:?} path={:?} selected={} has_book={} current_page={} last_rendered={:?}",
                    self.state,
                    self.current_entry,
                    self.last_viewed_entry,
                    self.home.path,
                    self.home.selected,
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
                    home_current_entry: self.home.current_entry_name_owned(),
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

    fn open_selected(&mut self) {
        let action = match self.home.open_selected() {
            Ok(action) => action,
            Err(HomeOpenError::Empty) => {
                self.error_message = Some("No entries found.".into());
                self.state = AppState::Error;
                self.dirty = true;
                return;
            }
        };
        match action {
            HomeOpen::EnterDir => {
                self.refresh_entries();
                if matches!(self.state, AppState::Error) {
                    self.home.path.pop();
                    self.refresh_entries();
                    self.set_error(ImageError::Message("Folder open failed.".into()));
                }
            }
            HomeOpen::OpenFile(entry) => {
                self.open_file_entry(entry);
            }
        }
    }

    fn open_index(&mut self, index: usize) {
        let Some(action) = self.home.open_index(index) else {
            return;
        };
        match action {
            HomeOpen::EnterDir => {}
            HomeOpen::OpenFile(entry) => self.open_file_entry(entry),
        }
    }

    fn open_file_entry(&mut self, entry: ImageEntry) {
        if is_trbk(&entry.name) {
            self.open_book_entry(entry);
            return;
        }
        if is_epub(&entry.name) {
            self.set_error(ImageError::Message(
                "EPUB files must be converted to .trbk.".into(),
            ));
            return;
        }
        if is_prc(&entry.name) {
            self.open_prc_entry(entry);
            return;
        }
        self.open_image_entry(entry);
    }

    fn open_book_entry(&mut self, entry: ImageEntry) {
        let entry_name = self.home.entry_path_string(&entry);
        match self.book_reader.open(
            self.source,
            &self.home.path,
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

    fn open_image_entry(&mut self, entry: ImageEntry) {
        match self.image_viewer.open(self.source, &self.home.path, &entry) {
            Ok(()) => {
                let entry_name = self.home.entry_path_string(&entry);
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

    fn open_prc_entry(&mut self, entry: ImageEntry) {
        match self.source.load_prc_info(&self.home.path, &entry) {
            Ok(info) => {
                let entry_name = self.home.entry_path_string(&entry);
                self.current_entry = Some(entry_name.clone());
                self.last_viewed_entry = Some(entry_name.clone());
                self.mark_recent_now(entry_name);
                self.prc_return_to_start_menu = matches!(self.state, AppState::StartMenu);
                self.prc_active_entry = Some(entry.clone());
                self.prc_session = None;
                self.prc_blocked_timeout_ticks = 0;
                self.prc_blocked_elapsed_ms = 0;
                self.prc_lines = prc_app::format_info_lines(&info);
                let runtime_snapshot = self.log_prc_info(&entry, &info);
                self.prc_runtime_form_id = runtime_snapshot.form_id;
                self.prc_ui_controller.reset();
                self.prc_runtime_bitmap_draws = runtime_snapshot.bitmap_draws;
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
                if let Ok(prc_raw) = self.source.load_prc_bytes(&self.home.path, &entry) {
                    self.prc_forms = prc_app::form_preview::parse_form_previews(&prc_raw);
                    self.prc_bitmaps = prc_app::bitmap::parse_prc_bitmaps(&prc_raw);
                    let menu_bar = prc_app::menu_preview::parse_menu_bar_preview(&prc_raw);
                    log::info!(
                        "PRC parsed previews forms={} bitmaps={} menus={}",
                        self.prc_forms.len(),
                        self.prc_bitmaps.len(),
                        menu_bar.as_ref().map(|m| m.menus.len()).unwrap_or(0)
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
                if let Ok(session) = prc_app::runner::PrcRuntimeSession::from_source(
                    self.source,
                    &self.home.path,
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
                self.prc_lines = prc_app::format_info_lines(&info);
                let runtime_snapshot = self.log_prc_info(&entry, &info);
                self.prc_runtime_form_id = runtime_snapshot.form_id;
                self.prc_ui_controller.reset();
                self.prc_runtime_bitmap_draws = runtime_snapshot.bitmap_draws;
                self.prc_runtime_field_draws = runtime_snapshot.field_draws;
                self.prc_runtime_table_draws = runtime_snapshot.table_draws;
                self.prc_system_fonts = self.source.load_prc_system_fonts();
                self.prc_forms.clear();
                self.prc_bitmaps.clear();
                self.prc_menu_controller.set_menu_bar(None);
                if let Ok(prc_raw) = self.source.load_prc_bytes(&path, &entry) {
                    self.prc_forms = prc_app::form_preview::parse_form_previews(&prc_raw);
                    self.prc_bitmaps = prc_app::bitmap::parse_prc_bitmaps(&prc_raw);
                    let menu_bar = prc_app::menu_preview::parse_menu_bar_preview(&prc_raw);
                    self.prc_menu_controller.set_menu_bar(menu_bar);
                }
                if let Ok(session) = prc_app::runner::PrcRuntimeSession::from_source(
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
    }

    fn resume_prc_runtime_session(&mut self) {
        let Some(session) = self.prc_session.as_mut() else {
            return;
        };
        let prev_help_dialog = session.help_dialog();
        let runtime_out = session.resume();
        let runtime_snapshot = runtime_out.snapshot;
        let changed = self.prc_runtime_form_id != runtime_snapshot.form_id
            || self.prc_runtime_bitmap_draws.len() != runtime_snapshot.bitmap_draws.len()
            || self
                .prc_runtime_bitmap_draws
                .iter()
                .zip(runtime_snapshot.bitmap_draws.iter())
                .any(|(a, b)| a.resource_id != b.resource_id || a.x != b.x || a.y != b.y)
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
        self.prc_runtime_bitmap_draws = runtime_snapshot.bitmap_draws;
        self.prc_runtime_field_draws = runtime_snapshot.field_draws;
        self.prc_runtime_table_draws = runtime_snapshot.table_draws;
        {
            let form = self.runtime_prc_form();
            if self.prc_ui_controller.sync_with_form(form.as_ref()) {
                self.dirty = true;
            }
        }
        self.prc_blocked_timeout_ticks = match runtime_out.state {
            prc_app::runner::RuntimeRunState::BlockedOnEvent { timeout_ticks } => {
                log::info!(
                    "PRC runtime blocked on EvtGetEvent timeout={} ticks steps={}",
                    timeout_ticks,
                    runtime_out.steps
                );
                timeout_ticks
            }
            prc_app::runner::RuntimeRunState::Stopped(reason) => {
                log::info!(
                    "PRC runtime stopped reason={:?} steps={}",
                    reason,
                    runtime_out.steps
                );
                0
            }
            prc_app::runner::RuntimeRunState::Running => {
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
        entry: &ImageEntry,
        info: &prc_app::PrcInfo,
    ) -> prc_app::runner::RuntimeUiSnapshot {
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
                let meta = prc_app::traps::table::lookup(*trap);
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
                    let meta = prc_app::traps::table::lookup(*trap);
                    log::info!(
                        "PRC code_scan id={} trap=0x{:04X} group={} name={}",
                        scan.resource_id,
                        trap,
                        meta.group.as_str(),
                        meta.name
                    );
                }
            }

            let dry_run = prc_app::runtime::dry_run_default(info);
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

            let dry_run_no_lib = prc_app::runtime::dry_run_ignore_lib(info);
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

            let dry_run_bootstrap = prc_app::runtime::dry_run_ignore_bootstrap_lib(info);
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
            prc_app::runner::log_prc_runtime_first_trap(
                self.source,
                &self.home.path,
                entry,
                info,
                true,
            )
        } else {
            prc_app::runner::RuntimeUiSnapshot::default()
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

    fn refresh_entries(&mut self) {
        match self.home.refresh_entries(self.source) {
            Ok(()) => {
                self.image_viewer.clear();
                self.book_reader.clear();
                if self.state != AppState::StartMenu {
                    self.set_state_menu();
                }
                self.error_message = None;
                self.dirty = true;
            }
            Err(err) => self.set_error(err),
        }
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
        self.dirty = true;
    }

    fn set_state_settings(&mut self) {
        self.state = AppState::Settings;
        self.dirty = true;
    }

    fn release_prc_resources(&mut self) {
        self.prc_active_entry = None;
        self.prc_session = None;
        self.prc_runtime_form_id = None;
        self.prc_blocked_timeout_ticks = 0;
        self.prc_blocked_elapsed_ms = 0;
        self.prc_soft_menu_focused = false;
        self.prc_soft_menu_last_control = None;
        self.prc_scroll = 0;
        self.prc_form_index = 0;
        self.prc_ui_controller.reset();
        self.prc_lines = Vec::new();
        self.prc_forms = Vec::new();
        self.prc_bitmaps = Vec::new();
        self.prc_runtime_bitmap_draws = Vec::new();
        self.prc_runtime_field_draws = Vec::new();
        self.prc_runtime_table_draws = Vec::new();
        self.prc_system_fonts = Vec::new();
        self.prc_menu_controller.reset();
        self.prc_reserved_gray_initialized = false;
    }

    fn exit_prc_viewer_to_origin(&mut self) {
        let to_start_menu = self.prc_return_to_start_menu;
        self.prc_return_to_start_menu = false;
        self.release_prc_resources();
        self.system.full_refresh = true;
        if to_start_menu {
            self.set_state_start_menu(true);
        } else {
            self.state = AppState::Menu;
            self.dirty = true;
        }
    }

    fn set_state_menu(&mut self) {
        if matches!(self.state, AppState::PrcViewing) {
            self.release_prc_resources();
            self.prc_return_to_start_menu = false;
            self.system.full_refresh = true;
        }
        self.state = AppState::Menu;
        self.dirty = true;
    }

    fn set_state_viewing(&mut self) {
        self.state = AppState::Viewing;
        self.system.full_refresh = true;
        self.dirty = true;
    }

    fn set_state_book_viewing(&mut self) {
        self.state = AppState::BookViewing;
        self.system.full_refresh = true;
        self.dirty = true;
    }

    fn set_state_toc(&mut self) {
        self.state = AppState::Toc;
        self.dirty = true;
    }

    fn set_state_prc_viewing(&mut self) {
        self.state = AppState::PrcViewing;
        self.prc_soft_menu_focused = false;
        self.prc_soft_menu_last_control = None;
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
        if self.home_system_fonts.is_empty() {
            self.home_system_fonts = self.source.load_home_system_fonts();
            if self.home_system_fonts.is_empty() {
                self.home_system_fonts = self.source.load_prc_system_fonts();
            }
        }
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
            palm_fonts: self.home_system_fonts.as_slice(),
            icons,
            draw_trbk_image,
        };
        self.home.draw_start_menu(&mut ctx, display, &recents);
    }



    fn draw_menu(&mut self, display: &mut impl crate::display::Display) {
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
            palm_fonts: self.prc_system_fonts.as_slice(),
            icons,
            draw_trbk_image,
        };
        self.home.draw_menu(&mut ctx, display);
    }


    fn draw_error(&mut self, display: &mut impl crate::display::Display) {
        const ERROR_LIST_TOP: i32 = 60;
        self.display_buffers.clear(BinaryColor::On).ok();
        let header_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        Text::new("Error", Point::new(LIST_MARGIN_X, HEADER_Y), header_style)
            .draw(self.display_buffers)
            .ok();
        if let Some(message) = &self.error_message {
            Text::new(message, Point::new(LIST_MARGIN_X, ERROR_LIST_TOP), header_style)
                .draw(self.display_buffers)
                .ok();
        }
        Text::new(
            "Press Back to return",
            Point::new(LIST_MARGIN_X, ERROR_LIST_TOP + 40),
            header_style,
        )
        .draw(self.display_buffers)
        .ok();
        let size = self.display_buffers.size();
        let mut rq = RenderQueue::default();
        rq.push(
            Rect::new(0, 0, size.width as i32, size.height as i32),
            RefreshMode::Full,
        );
        flush_queue(display, self.display_buffers, &mut rq, RefreshMode::Full);
    }

    fn draw_prc_viewer(&mut self, display: &mut impl crate::display::Display) {
        const STATUS_H: i32 = 34;
        self.display_buffers.clear(BinaryColor::On).ok();
        let size = self.display_buffers.size();
        Rectangle::new(
            Point::new(0, 0),
            Size::new(size.width, STATUS_H as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(self.display_buffers)
        .ok();
        let battery = self.system.battery_percent.unwrap_or(100);
        let battery_text = format!("{}%", battery);
        let batt_w = 34 * 3;
        let batt_h = 8 * 3;
        let cap_w = 2 * 3;
        let cap_h = 4 * 3;
        let batt_total_w = batt_w + cap_w;
        let batt_x = (size.width as i32 - batt_total_w) / 2;
        let batt_y = ((STATUS_H - batt_h) / 2) + 2;
        Rectangle::new(
            Point::new(batt_x, batt_y),
            Size::new(batt_w as u32, batt_h as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(self.display_buffers)
        .ok();
        Rectangle::new(
            Point::new(batt_x + batt_w, batt_y + (batt_h - cap_h) / 2),
            Size::new(cap_w as u32, cap_h as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(self.display_buffers)
        .ok();
        if !self.home_system_fonts.is_empty() {
            let tw = crate::ui::prc_components::palm_text_width(
                &battery_text,
                0,
                self.home_system_fonts.as_slice(),
                1,
            );
            let th = crate::ui::prc_components::palm_text_height(
                0,
                self.home_system_fonts.as_slice(),
                1,
            );
            let tx = batt_x + (batt_w - tw) / 2;
            let ty = batt_y + (batt_h - th) / 2;
            crate::ui::prc_components::draw_palm_text(
                self.display_buffers,
                &battery_text,
                tx,
                ty,
                0,
                self.home_system_fonts.as_slice(),
                1,
                BinaryColor::On,
            );
        }

        let draw_form = self
            .runtime_prc_form()
            .or_else(|| self.prc_forms.get(self.prc_form_index).cloned())
            .or_else(|| self.prc_forms.first().cloned());
        if let Some(form) = draw_form {
            let outline = PrimitiveStyle::with_stroke(BinaryColor::Off, 1);
            let clear = PrimitiveStyle::with_fill(BinaryColor::On);
            let max_scale_w = ((size.width as i32) / 160).max(1);
            let content_top = STATUS_H + 2;
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
            prc_app::ui::draw_form_preview(
                self.display_buffers,
                &form,
                &self.prc_system_fonts,
                &self.prc_bitmaps,
                &self.prc_runtime_bitmap_draws,
                &self.prc_runtime_field_draws,
                &self.prc_runtime_table_draws,
                self.prc_ui_controller.focused_control_id(),
                self.prc_menu_controller.overlay(),
                self.prc_session
                    .as_ref()
                    .and_then(|session| session.help_dialog())
                    .as_ref(),
                pane_x,
                pane_y,
                pane_w,
                pane_h,
                scale.max(1),
                outline,
            );
        }

        let content_top = STATUS_H + 2;
        let max_scale_w = ((size.width as i32) / 160).max(1);
        let max_scale_h = ((size.height as i32 - content_top) / 160).max(1);
        let max_scale = max_scale_w.min(max_scale_h).max(1);
        let scale = if max_scale >= 3 { 3 } else { max_scale };
        let pane_h = 160 * scale;
        let strip_top = (content_top + pane_h).clamp(0, size.height as i32);
        let strip_h = (size.height as i32 - strip_top).max(0);
        let soft_menu_rect = if strip_h > 0 {
            Some(self.prc_soft_menu_button_rect(strip_top, strip_h))
        } else {
            None
        };

        if let Some(rect) = soft_menu_rect {
            self.draw_prc_soft_menu_button(rect);
        }

        if strip_h > 0 && !self.prc_reserved_gray_initialized {
            self.gray2_lsb.fill(0);
            self.gray2_msb.fill(0);
            fill_gray2_rect(
                self.display_buffers.rotation(),
                self.gray2_lsb.as_mut_slice(),
                self.gray2_msb.as_mut_slice(),
                0,
                strip_top,
                size.width as i32,
                strip_h,
                true,
                false,
            );
            crate::app::home::merge_bw_into_gray2(
                self.display_buffers,
                self.gray2_lsb.as_mut_slice(),
                self.gray2_msb.as_mut_slice(),
            );
            let lsb: &[u8; crate::framebuffer::BUFFER_SIZE] =
                self.gray2_lsb.as_slice().try_into().unwrap();
            let msb: &[u8; crate::framebuffer::BUFFER_SIZE] =
                self.gray2_msb.as_slice().try_into().unwrap();
            display.copy_grayscale_buffers(lsb, msb);
            display.display_absolute_grayscale(GrayscaleMode::Fast);
            self.display_buffers.copy_active_to_inactive();
            self.prc_reserved_gray_initialized = true;
            return;
        }

        let mode = if self.system.full_refresh {
            RefreshMode::Full
        } else {
            RefreshMode::Fast
        };
        let mut rq = RenderQueue::default();
        let update_h = if strip_h > 0 {
            strip_top
        } else {
            size.height as i32
        };
        rq.push(Rect::new(0, 0, size.width as i32, update_h), mode);
        if let Some(rect) = soft_menu_rect {
            rq.push(rect, mode);
        }
        flush_queue(display, self.display_buffers, &mut rq, mode);
    }

    fn draw_settings(&mut self, display: &mut impl crate::display::Display) {
        let mut ctx = SettingsContext {
            display_buffers: self.display_buffers,
            gray2_lsb: self.gray2_lsb.as_mut_slice(),
            gray2_msb: self.gray2_msb.as_mut_slice(),
            logo_w: generated_icons::LOGO_WIDTH as i32,
            logo_h: generated_icons::LOGO_HEIGHT as i32,
            logo_dark: generated_icons::LOGO_DARK_MASK,
            logo_light: generated_icons::LOGO_LIGHT_MASK,
            version: build_info::VERSION,
            build_time: build_info::BUILD_TIME,
        };
        draw_settings(&mut ctx, display);
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
        let style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        Text::new(title, Point::new(16, 24), style)
            .draw(self.display_buffers)
            .ok();
        Text::new(message, Point::new(16, 60), style)
            .draw(self.display_buffers)
            .ok();
        let footer_y = if let Some(status) = status {
            Text::new(status, Point::new(16, 80), style)
                .draw(self.display_buffers)
                .ok();
            120
        } else {
            100
        };
        Text::new(footer, Point::new(16, footer_y), style)
            .draw(self.display_buffers)
            .ok();
        display.display(self.display_buffers, RefreshMode::Full);
    }


    fn draw_image_viewer(&mut self, display: &mut impl crate::display::Display) {
        let mut ctx = ImageViewerContext {
            display_buffers: self.display_buffers,
            gray2_lsb: self.gray2_lsb.as_mut_slice(),
            gray2_msb: self.gray2_msb.as_mut_slice(),
            source: self.source,
            wake_restore_only: &mut self.system.wake_restore_only,
        };
        if let Err(err) = self.image_viewer.draw(&mut ctx, display) {
            self.set_error(err);
        }
    }



    fn draw_book_reader(&mut self, display: &mut impl crate::display::Display) {
        let mut ctx = BookReaderContext {
            display_buffers: self.display_buffers,
            gray2_lsb: self.gray2_lsb.as_mut_slice(),
            gray2_msb: self.gray2_msb.as_mut_slice(),
            source: self.source,
            full_refresh: &mut self.system.full_refresh,
        };
        if let Err(err) = self.book_reader.draw_book(&mut ctx, display) {
            self.set_error(err);
        }
    }

    fn draw_toc_view(&mut self, display: &mut impl crate::display::Display) {
        let mut ctx = BookReaderContext {
            display_buffers: self.display_buffers,
            gray2_lsb: self.gray2_lsb.as_mut_slice(),
            gray2_msb: self.gray2_msb.as_mut_slice(),
            source: self.source,
            full_refresh: &mut self.system.full_refresh,
        };
        if let Err(err) = self.book_reader.draw_toc(&mut ctx, display) {
            self.set_error(err);
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
            .apply_resume(outcome, &mut self.home, self.source);
        match outcome {
            ApplyResumeOutcome::None => {}
            ApplyResumeOutcome::Missing => {}
            ApplyResumeOutcome::Ready {
                entry,
                page,
                refreshed,
            } => {
                if refreshed {
                    self.image_viewer.clear();
                    self.book_reader.clear();
                    if self.state != AppState::StartMenu {
                        self.state = AppState::Menu;
                    }
                    self.error_message = None;
                    self.dirty = true;
                }
                self.open_file_entry(entry);
                if let Some(page) = page {
                    if let Some(book) = &self.book_reader.current_book {
                        if page < book.page_count {
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
        }
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

fn fill_gray2_rect(
    rotation: Rotation,
    gray2_lsb: &mut [u8],
    gray2_msb: &mut [u8],
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    lsb_on: bool,
    msb_on: bool,
) {
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(crate::framebuffer::HEIGHT as i32);
    let y1 = (y + h).min(crate::framebuffer::WIDTH as i32);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    for py in y0..y1 {
        for px in x0..x1 {
            let Some((fx, fy)) = map_display_point(rotation, px, py) else {
                continue;
            };
            let idx = fy * crate::framebuffer::WIDTH + fx;
            let byte = idx / 8;
            let bit = 7 - (idx % 8);
            let mask = 1 << bit;
            if lsb_on {
                gray2_lsb[byte] |= mask;
            } else {
                gray2_lsb[byte] &= !mask;
            }
            if msb_on {
                gray2_msb[byte] |= mask;
            } else {
                gray2_msb[byte] &= !mask;
            }
        }
    }
}

fn map_display_point(rotation: Rotation, x: i32, y: i32) -> Option<(usize, usize)> {
    if x < 0 || y < 0 {
        return None;
    }
    let (x, y) = match rotation {
        Rotation::Rotate0 => (x as usize, y as usize),
        Rotation::Rotate90 => (y as usize, crate::framebuffer::HEIGHT - 1 - x as usize),
        Rotation::Rotate180 => (
            crate::framebuffer::WIDTH - 1 - x as usize,
            crate::framebuffer::HEIGHT - 1 - y as usize,
        ),
        Rotation::Rotate270 => (crate::framebuffer::WIDTH - 1 - y as usize, x as usize),
    };
    if x >= crate::framebuffer::WIDTH || y >= crate::framebuffer::HEIGHT {
        None
    } else {
        Some((x, y))
    }
}
