extern crate alloc;

use alloc::{format, string::{String, ToString}, vec, vec::Vec};

use embedded_graphics::{
    mono_font::{ascii::{FONT_10X20, FONT_6X10}, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, OriginDimensions, Point, Primitive, Size},
    primitives::Rectangle,
    text::Text,
    Drawable,
};

use crate::display::{Display, RefreshMode};
use crate::framebuffer::{DisplayBuffers, Rotation, BUFFER_SIZE, HEIGHT as FB_HEIGHT, WIDTH as FB_WIDTH};
use crate::image_viewer::{AppSource, ImageData, ImageEntry, InstalledAppEntry};
use crate::platform::ButtonId;
use crate::platform::PlatformInputEvent;
use crate::render_policy::RenderPolicy;
use crate::ternos::ui::{flush_queue, prc_alert, prc_components::{auto_button_layout_for_label, draw_form_title_bar, draw_palm_pull_down_box, draw_palm_text, draw_palm_text_scaled, palm_text_height, palm_text_height_scaled, palm_text_width, palm_text_width_scaled}, FormResource, ObjectId, ObjectResource, Rect, RenderQueue, TableCellRenderer, TableHit, TableScrollBarHit, TableScrollBarView, TableView, UiContext, UiEvent, UiRuntime, UiTableCell, UiTableColumn, UiTableModel, UiTableRow, View};

const START_MENU_MARGIN: i32 = 16;
const START_MENU_RECENT_THUMB: i32 = 74;
const START_MENU_STATUS_H: i32 = 34;
const START_MENU_FORM_Y: i32 = START_MENU_STATUS_H + 2;
const HEADER_Y: i32 = START_MENU_FORM_Y + 22;
const LIST_TOP: i32 = 72;
const APP_GRID_COLS: usize = 3;
const HOME_FORM_ID: u16 = 1;
const HOME_OBJ_CATEGORY_TRIGGER: ObjectId = 100;
const HOME_OBJ_RECENTS_TABLE: ObjectId = 110;
const HOME_OBJ_APPS_TABLE: ObjectId = 111;
const HOME_OBJ_CONTENT_BASE: ObjectId = 1000;
const HOME_OBJ_CATEGORY_MENU_BASE: ObjectId = 2000;
const HOME_OBJ_DIALOG_DISMISS: ObjectId = 3000;

#[derive(Clone, Copy, Debug)]
struct PalmChromeMetrics {
    scale: i32,
}

impl PalmChromeMetrics {
    const fn palm3() -> Self {
        Self { scale: 3 }
    }

    const fn px(self, base: i32) -> i32 {
        base * self.scale
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartMenuSection {
    Recents,
    Actions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LauncherCategory {
    Recents,
    Apps,
    Books,
    Images,
}

pub struct RecentPreview {
    pub path: String,
    pub title: String,
    pub image: Option<ImageData>,
}

pub struct InstallDialogState {
    pub title: String,
    pub message: String,
}

pub struct HomeState {
    pub ui_runtime: UiRuntime,
    pub prev_focus_object_id: Option<ObjectId>,
    pub start_menu_section: StartMenuSection,
    pub launcher_category: LauncherCategory,
    pub start_menu_index: usize,
    pub category_menu_open: bool,
    pub category_menu_index: usize,
    pub start_menu_cache: Vec<RecentPreview>,
    pub books_cache: Vec<RecentPreview>,
    pub images_cache: Vec<RecentPreview>,
    pub installed_apps: Vec<InstalledAppEntry>,
    pub apps_top_row: usize,
    pub books_top_row: usize,
    pub images_top_row: usize,
    pub touch_pressed_index: Option<usize>,
    pub last_table_touch_rect: Option<Rect>,
    pub last_scrollbar_rect: Option<Rect>,
    pub last_category_trigger_rect: Option<Rect>,
    pub last_category_popup_rect: Option<Rect>,
    pub start_menu_nav_pending: bool,
    pub start_menu_need_base_refresh: bool,
    pub install_dialog: Option<InstallDialogState>,
}

pub enum HomeAction {
    None,
    OpenRecent(String),
}

pub struct HomeIcons<'a> {
    pub icon_size: i32,
    pub folder_dark: &'a [u8],
    pub folder_light: &'a [u8],
    pub gear_dark: &'a [u8],
    pub gear_light: &'a [u8],
    pub battery_dark: &'a [u8],
    pub battery_light: &'a [u8],
}

pub type DrawTrbkImageFn = fn(
    &mut DisplayBuffers,
    &ImageData,
    &mut Option<(&mut [u8], &mut [u8], &mut bool)>,
    RenderPolicy,
    i32,
    i32,
    i32,
    i32,
);

pub struct HomeRenderContext<'a, S: AppSource> {
    pub display_buffers: &'a mut DisplayBuffers,
    pub gray2_lsb: &'a mut [u8],
    pub gray2_msb: &'a mut [u8],
    pub source: &'a mut S,
    pub full_refresh: bool,
    pub battery_percent: Option<u8>,
    pub render_policy: RenderPolicy,
    pub palm_fonts: &'a [crate::palm::runtime::PalmFont],
    pub icons: HomeIcons<'a>,
    pub draw_trbk_image: DrawTrbkImageFn,
}

struct RecentTableRenderer<'a> {
    previews: &'a [RecentPreview],
    thumb_size: i32,
    palm_fonts: &'a [crate::palm::runtime::PalmFont],
    render_policy: RenderPolicy,
    draw_trbk_image: DrawTrbkImageFn,
}

impl TableCellRenderer for RecentTableRenderer<'_> {
    fn render_cell(
        &self,
        ctx: &mut UiContext<'_>,
        cell_rect: Rect,
        _row: &UiTableRow,
        cell: &UiTableCell,
        row_index: usize,
        _col_index: usize,
        selected: bool,
    ) {
        let Some(preview) = self.previews.get(row_index) else {
            return;
        };
        let thumb_x = cell_rect.x + 4;
        let thumb_y = cell_rect.y + ((cell_rect.h - self.thumb_size) / 2).max(0);
        Rectangle::new(
            Point::new(thumb_x, thumb_y),
            Size::new(self.thumb_size as u32, self.thumb_size as u32),
        )
        .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_stroke(
            if selected { BinaryColor::On } else { BinaryColor::Off },
            1,
        ))
        .draw(ctx.buffers)
        .ok();
        if let Some(image) = preview.image.as_ref() {
            let mut gray2_ctx = None;
            (self.draw_trbk_image)(
                ctx.buffers,
                image,
                &mut gray2_ctx,
                self.render_policy,
                thumb_x + 2,
                thumb_y + 2,
                self.thumb_size - 4,
                self.thumb_size - 4,
            );
        }
        let text_color = if selected { BinaryColor::On } else { BinaryColor::Off };
        let title_x = thumb_x + self.thumb_size + 12;
        let title_max_w = (cell_rect.w - (title_x - cell_rect.x) - 6).max(20);
        let lines = wrap_home_title_lines(
            &cell.text,
            title_max_w,
            self.palm_fonts,
            0,
            6,
            5,
            2,
        );
        if !self.palm_fonts.is_empty() {
            let line_h = palm_text_height_scaled(0, self.palm_fonts, 6, 5).max(8) + 2;
            for (line_idx, line) in lines.iter().enumerate() {
                draw_palm_text_scaled(
                    ctx.buffers,
                    line.as_str(),
                    title_x,
                    cell_rect.y + 10 + (line_idx as i32 * line_h),
                    0,
                    self.palm_fonts,
                    6,
                    5,
                    text_color,
                );
            }
        } else {
            let label_style = MonoTextStyle::new(&FONT_10X20, text_color);
            for (line_idx, line) in lines.iter().enumerate() {
                Text::new(
                    line.as_str(),
                    Point::new(title_x, cell_rect.y + 26 + (line_idx as i32 * 18)),
                    label_style,
                )
                .draw(ctx.buffers)
                .ok();
            }
        }
    }
}

struct AppsTableRenderer<'a> {
    apps: &'a [InstalledAppEntry],
    palm_fonts: &'a [crate::palm::runtime::PalmFont],
    render_policy: RenderPolicy,
    draw_trbk_image: DrawTrbkImageFn,
}

impl TableCellRenderer for AppsTableRenderer<'_> {
    fn render_cell(
        &self,
        ctx: &mut UiContext<'_>,
        cell_rect: Rect,
        row: &UiTableRow,
        cell: &UiTableCell,
        _row_index: usize,
        col_index: usize,
        selected: bool,
    ) {
        let item_index = row.data as usize + col_index;
        let Some(app) = self.apps.get(item_index) else {
            return;
        };
        let icon_size = 60i32;
        let icon_x = cell_rect.x + ((cell_rect.w - icon_size) / 2).max(0);
        let icon_y = cell_rect.y + 2;
        if let Some(image) = app.icon.as_ref() {
            let mut gray2_ctx = None;
            (self.draw_trbk_image)(
                ctx.buffers,
                image,
                &mut gray2_ctx,
                self.render_policy,
                icon_x,
                icon_y,
                icon_size,
                icon_size,
            );
        } else {
            Rectangle::new(
                Point::new(icon_x, icon_y),
                Size::new(icon_size as u32, icon_size as u32),
            )
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_stroke(
                BinaryColor::Off,
                1,
            ))
            .draw(ctx.buffers)
            .ok();
        }
        let title_max_w = (cell_rect.w - 6).max(20);
        let title_lines = wrap_home_title_lines(
            &cell.text,
            title_max_w,
            self.palm_fonts,
            0,
            6,
            5,
            2,
        );
        let title_color = if selected { BinaryColor::On } else { BinaryColor::Off };
        let title_bg = if selected { BinaryColor::Off } else { BinaryColor::On };
        let line_h = palm_text_height_scaled(0, self.palm_fonts, 6, 5).max(10);
        let title_block_h = (title_lines.len() as i32 * line_h).max(line_h);
        let title_top = icon_y + icon_size + 6;
        if selected {
            Rectangle::new(
                Point::new(cell_rect.x + 2, title_top - 2),
                Size::new((cell_rect.w - 4).max(1) as u32, (title_block_h + 4).max(1) as u32),
            )
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(title_bg))
            .draw(ctx.buffers)
            .ok();
        }
        if !self.palm_fonts.is_empty() {
            for (line_idx, line) in title_lines.iter().enumerate() {
                let tw = palm_text_width_scaled(line, 0, self.palm_fonts, 6, 5);
                let tx = cell_rect.x + ((cell_rect.w - tw) / 2).max(0);
                let ty = title_top + (line_idx as i32 * line_h);
                draw_palm_text_scaled(
                    ctx.buffers,
                    line.as_str(),
                    tx,
                    ty,
                    0,
                    self.palm_fonts,
                    6,
                    5,
                    title_color,
                );
            }
        } else {
            Text::new(
                app.title.as_str(),
                Point::new(cell_rect.x + 4, title_top + 12),
                MonoTextStyle::new(&FONT_10X20, title_color),
            )
            .draw(ctx.buffers)
            .ok();
        }
    }
}

impl HomeState {
    fn launcher_content_height() -> i32 {
        let mid_y = FB_HEIGHT as i32 - START_MENU_MARGIN;
        let list_top = HEADER_Y + 28;
        (mid_y - list_top).max(1)
    }

    fn launcher_table_row_height(category: LauncherCategory) -> i32 {
        match category {
            LauncherCategory::Apps => 120,
            LauncherCategory::Recents | LauncherCategory::Books | LauncherCategory::Images => 99,
        }
    }

    fn launcher_visible_rows_for_category(category: LauncherCategory) -> usize {
        let content_h = Self::launcher_content_height();
        let row_h = Self::launcher_table_row_height(category);
        (content_h / row_h).max(1) as usize
    }

    fn launcher_total_rows_for_category(&self, recents: &[String], category: LauncherCategory) -> usize {
        match category {
            LauncherCategory::Apps => self.installed_apps.len().div_ceil(APP_GRID_COLS),
            LauncherCategory::Recents => recents.len().min(Self::launcher_visible_rows_for_category(category)),
            LauncherCategory::Books => self.books_cache.len(),
            LauncherCategory::Images => self.images_cache.len(),
        }
    }

    fn launcher_top_row_for_category(&self, category: LauncherCategory) -> usize {
        match category {
            LauncherCategory::Recents => 0,
            LauncherCategory::Apps => self.apps_top_row,
            LauncherCategory::Books => self.books_top_row,
            LauncherCategory::Images => self.images_top_row,
        }
    }

    fn clamp_launcher_top_row_for_category(&mut self, recents: &[String], category: LauncherCategory) {
        let visible_rows = Self::launcher_visible_rows_for_category(category);
        let total_rows = self.launcher_total_rows_for_category(recents, category);
        let max_top = total_rows.saturating_sub(visible_rows);
        let top_row = self.launcher_top_row_for_category(category).min(max_top);
        self.set_launcher_top_row_for_category(category, top_row);
    }

    fn set_launcher_top_row_for_category(&mut self, category: LauncherCategory, top_row: usize) {
        match category {
            LauncherCategory::Recents => {}
            LauncherCategory::Apps => self.apps_top_row = top_row,
            LauncherCategory::Books => self.books_top_row = top_row,
            LauncherCategory::Images => self.images_top_row = top_row,
        }
    }

    fn ensure_launcher_selection_visible(&mut self, recents: &[String]) -> bool {
        let category = self.launcher_category;
        if category == LauncherCategory::Recents {
            return false;
        }
        let visible_rows = Self::launcher_visible_rows_for_category(category);
        let total_rows = self.launcher_total_rows_for_category(recents, category);
        if total_rows <= visible_rows {
            let changed = self.launcher_top_row_for_category(category) != 0;
            self.set_launcher_top_row_for_category(category, 0);
            return changed;
        }
        let selected_row = match category {
            LauncherCategory::Apps => self.start_menu_index / APP_GRID_COLS,
            LauncherCategory::Books | LauncherCategory::Images => self.start_menu_index,
            LauncherCategory::Recents => 0,
        };
        let mut top_row = self.launcher_top_row_for_category(category);
        if selected_row < top_row {
            top_row = selected_row;
        } else if selected_row >= top_row + visible_rows {
            top_row = selected_row + 1 - visible_rows;
        }
        let max_top = total_rows.saturating_sub(visible_rows);
        let clamped = top_row.min(max_top);
        let changed = clamped != self.launcher_top_row_for_category(category);
        self.set_launcher_top_row_for_category(category, clamped);
        changed
    }

    fn launcher_table_layout(
        category: LauncherCategory,
        width: i32,
        height: i32,
    ) -> (Rect, Option<Rect>) {
        let list_top = HEADER_Y + 28;
        let mid_y = height - START_MENU_MARGIN;
        let list_width = width - (START_MENU_MARGIN * 2);
        let content_h = (mid_y - list_top).max(1);
        let has_scrollbar = matches!(
            category,
            LauncherCategory::Apps | LauncherCategory::Books | LauncherCategory::Images
        );
        let scrollbar_w = if has_scrollbar { 12 } else { 0 };
        let table_rect = Rect::new(
            START_MENU_MARGIN - 4,
            list_top - 4,
            list_width + 8 - scrollbar_w,
            content_h,
        );
        let scrollbar_rect = has_scrollbar.then(|| {
            Rect::new(
                table_rect.x + table_rect.w + 1,
                list_top,
                scrollbar_w.saturating_sub(1),
                content_h - 8,
            )
        });
        (table_rect, scrollbar_rect)
    }

    fn launcher_category_trigger_rect(width: i32) -> Rect {
        Rect::new(width - 96, START_MENU_FORM_Y, 92, 24)
    }

    fn launcher_category_popup_rect(width: i32) -> Rect {
        Rect::new(width - 140, START_MENU_FORM_Y, 136, 84)
    }

    fn scroll_launcher_to(&mut self, recents: &[String], top_row: usize) {
        let category = self.launcher_category;
        let visible_rows = Self::launcher_visible_rows_for_category(category);
        let total_rows = self.launcher_total_rows_for_category(recents, category);
        let max_top = total_rows.saturating_sub(visible_rows);
        let top_row = top_row.min(max_top);
        self.set_launcher_top_row_for_category(category, top_row);
        self.start_menu_index = match category {
            LauncherCategory::Apps => (top_row * APP_GRID_COLS).min(self.installed_apps.len().saturating_sub(1)),
            LauncherCategory::Books => top_row.min(self.books_cache.len().saturating_sub(1)),
            LauncherCategory::Images => top_row.min(self.images_cache.len().saturating_sub(1)),
            LauncherCategory::Recents => self.start_menu_index,
        };
        self.start_menu_nav_pending = true;
        self.start_menu_need_base_refresh = true;
        self.sync_start_menu_focus(recents);
    }

    pub fn handle_start_menu_touch(
        &mut self,
        recents: &[String],
        event: &PlatformInputEvent,
        width: i32,
        height: i32,
    ) -> HomeAction {
        if self.install_dialog.is_some() {
            return HomeAction::None;
        }

        let category = self.launcher_category;
        let (table_rect, scrollbar_rect) = Self::launcher_table_layout(category, width, height);
        let (point, is_down, is_up) = match *event {
            PlatformInputEvent::TouchDown { x, y } => {
                (crate::ternos::ui::Point::new(x, y), true, false)
            }
            PlatformInputEvent::TouchUp { x, y } => {
                (crate::ternos::ui::Point::new(x, y), false, true)
            }
            _ => return HomeAction::None,
        };
        let categories = [
            LauncherCategory::Recents,
            LauncherCategory::Apps,
            LauncherCategory::Books,
            LauncherCategory::Images,
        ];
        let trigger_rect = self
            .last_category_trigger_rect
            .unwrap_or_else(|| Self::launcher_category_trigger_rect(width));
        let popup_rect = self
            .last_category_popup_rect
            .unwrap_or_else(|| Self::launcher_category_popup_rect(width));

        if self.category_menu_open {
            if popup_rect.contains(point) && is_down {
                let item_h = (popup_rect.h / categories.len() as i32).max(1);
                let idx = ((point.y - popup_rect.y) / item_h)
                    .clamp(0, categories.len() as i32 - 1) as usize;
                self.launcher_category = categories[idx];
                self.category_menu_index = idx;
                self.category_menu_open = false;
                self.touch_pressed_index = None;
                self.set_content_focus(recents, 0);
                self.start_menu_need_base_refresh = true;
                return HomeAction::None;
            }
            if is_down && !popup_rect.contains(point) {
                self.category_menu_open = false;
                self.start_menu_need_base_refresh = true;
                self.start_menu_nav_pending = true;
                self.sync_start_menu_focus(recents);
            }
            return HomeAction::None;
        }

        if trigger_rect.contains(point) && is_down {
            self.set_actions_focus();
            self.category_menu_index = categories
                .iter()
                .position(|c| *c == self.launcher_category)
                .unwrap_or(0);
            self.category_menu_open = true;
            self.start_menu_need_base_refresh = true;
            self.start_menu_nav_pending = true;
            self.sync_start_menu_focus(recents);
            return HomeAction::None;
        }

        let model = self.build_launcher_table_model(recents);
        let table = TableView::new(&model);
        let table_rect = self.last_table_touch_rect.unwrap_or(table_rect);
        let scrollbar_rect = self.last_scrollbar_rect.or(scrollbar_rect);

        if let Some(TableHit::Cell { row, col }) = table.hit_test(table_rect, point) {
            let item_index = match category {
                LauncherCategory::Apps => model
                    .rows
                    .get(row)
                    .map(|r| r.data as usize + col)
                    .unwrap_or(row * APP_GRID_COLS + col),
                LauncherCategory::Recents | LauncherCategory::Books | LauncherCategory::Images => row,
            };
            if item_index < self.start_menu_content_len(recents) {
                self.set_content_focus(recents, item_index);
                self.start_menu_nav_pending = true;
                if is_down {
                    self.touch_pressed_index = Some(item_index);
                    return HomeAction::None;
                }
                if is_up && self.touch_pressed_index == Some(item_index) {
                    self.touch_pressed_index = None;
                    return match category {
                        LauncherCategory::Recents => recents
                            .get(item_index)
                            .cloned()
                            .map(HomeAction::OpenRecent)
                            .unwrap_or(HomeAction::None),
                        LauncherCategory::Apps => self
                            .installed_apps
                            .get(item_index)
                            .map(|app| HomeAction::OpenRecent(app.path.clone()))
                            .unwrap_or(HomeAction::None),
                        LauncherCategory::Books => self
                            .books_cache
                            .get(item_index)
                            .map(|book| HomeAction::OpenRecent(book.path.clone()))
                            .unwrap_or(HomeAction::None),
                        LauncherCategory::Images => self
                            .images_cache
                            .get(item_index)
                            .map(|image| HomeAction::OpenRecent(image.path.clone()))
                            .unwrap_or(HomeAction::None),
                    };
                }
            }
        }

        if let Some(scrollbar_rect) = scrollbar_rect {
            let total_rows = self.launcher_total_rows_for_category(recents, category);
            let visible_rows =
                Self::launcher_visible_rows_for_category(category).min(total_rows.max(1));
            let top_row = self.launcher_top_row_for_category(category);
            let scrollbar = TableScrollBarView::new(top_row, visible_rows, total_rows);
            if let Some(hit) = scrollbar.hit_test(scrollbar_rect, point) {
                self.touch_pressed_index = None;
                match hit {
                    TableScrollBarHit::ArrowUp => {
                        self.scroll_launcher_to(recents, top_row.saturating_sub(1));
                    }
                    TableScrollBarHit::ArrowDown => {
                        self.scroll_launcher_to(recents, top_row.saturating_add(1));
                    }
                    TableScrollBarHit::Track { top_row } => {
                        self.scroll_launcher_to(recents, top_row);
                    }
                }
            }
        }

        if is_up {
            self.touch_pressed_index = None;
        }

        HomeAction::None
    }

    pub fn new() -> Self {
        Self {
            ui_runtime: UiRuntime::default(),
            prev_focus_object_id: None,
            start_menu_section: StartMenuSection::Recents,
            launcher_category: LauncherCategory::Recents,
            start_menu_index: 0,
            category_menu_open: false,
            category_menu_index: 0,
            start_menu_cache: Vec::new(),
            books_cache: Vec::new(),
            images_cache: Vec::new(),
            installed_apps: Vec::new(),
            apps_top_row: 0,
            books_top_row: 0,
            images_top_row: 0,
            touch_pressed_index: None,
            last_table_touch_rect: None,
            last_scrollbar_rect: None,
            last_category_trigger_rect: None,
            last_category_popup_rect: None,
            start_menu_nav_pending: false,
            start_menu_need_base_refresh: true,
            install_dialog: None,
        }
    }

    pub fn show_install_summary_dialog(
        &mut self,
        summary: crate::ternos::services::db::InstallSummary,
    ) {
        let mut msg = format!(
            "Scanned {} files.\nInstalled: {}\nUpgraded: {}\nSkipped: {}\nFailed: {}",
            summary.scanned, summary.installed, summary.upgraded, summary.skipped, summary.failed
        );
        if summary.failed > 0 {
            msg.push_str("\n\nCheck logs for install errors.");
        }
        self.install_dialog = Some(InstallDialogState {
            title: "Install".to_string(),
            message: msg,
        });
        self.start_menu_need_base_refresh = true;
    }

    pub fn start_menu_cache_same(&self, recents: &[String]) -> bool {
        recents.len() == self.start_menu_cache.len()
            && recents
                .iter()
                .zip(self.start_menu_cache.iter())
                .all(|(path, cached)| path == &cached.path)
    }

    fn launcher_object_id_for_index(index: usize) -> ObjectId {
        HOME_OBJ_CONTENT_BASE.saturating_add(index as u16)
    }

    fn category_menu_object_id(index: usize) -> ObjectId {
        HOME_OBJ_CATEGORY_MENU_BASE.saturating_add(index as u16)
    }

    fn content_index_from_object_id(object_id: ObjectId) -> Option<usize> {
        if object_id >= HOME_OBJ_CONTENT_BASE {
            Some((object_id - HOME_OBJ_CONTENT_BASE) as usize)
        } else {
            None
        }
    }

    fn category_index_from_object_id(object_id: ObjectId) -> Option<usize> {
        if object_id >= HOME_OBJ_CATEGORY_MENU_BASE && object_id < HOME_OBJ_CATEGORY_MENU_BASE + 16 {
            Some((object_id - HOME_OBJ_CATEGORY_MENU_BASE) as usize)
        } else {
            None
        }
    }

    fn sync_start_menu_ui(&mut self, recents: &[String]) {
        let content_len = self.start_menu_content_len(recents);
        let mut objects = vec![ObjectResource::Button {
            id: HOME_OBJ_CATEGORY_TRIGGER,
            bounds: Rect::new(0, START_MENU_FORM_Y, 160, 24),
        }];
        if self.install_dialog.is_some() {
            objects.push(ObjectResource::Button {
                id: HOME_OBJ_DIALOG_DISMISS,
                bounds: Rect::new(40, 80, 80, 24),
            });
        } else if self.category_menu_open {
            for idx in 0..4usize {
                objects.push(ObjectResource::Button {
                    id: Self::category_menu_object_id(idx),
                    bounds: Rect::new(100, START_MENU_FORM_Y + (idx as i32 * 18), 60, 16),
                });
            }
        } else {
            let table_model = self.build_launcher_table_model(recents);
            let table_object_id = match self.launcher_category {
                LauncherCategory::Recents => HOME_OBJ_RECENTS_TABLE,
                LauncherCategory::Apps => HOME_OBJ_APPS_TABLE,
                LauncherCategory::Books | LauncherCategory::Images => HOME_OBJ_RECENTS_TABLE,
            };
            objects.push(ObjectResource::Table {
                id: table_object_id,
                bounds: Rect::new(
                    START_MENU_MARGIN,
                    LIST_TOP,
                    120,
                    (content_len.max(1) as i32) * 18,
                ),
                model: table_model,
            });
            for idx in 0..content_len {
                objects.push(ObjectResource::Button {
                    id: Self::launcher_object_id_for_index(idx),
                    bounds: Rect::new(START_MENU_MARGIN, LIST_TOP + (idx as i32 * 18), 120, 16),
                });
            }
        }
        self.ui_runtime.upsert_form(FormResource {
            form_id: HOME_FORM_ID,
            title: Some("Launcher".to_string()),
            objects,
        }
        .into_ui_form());
        self.ui_runtime.set_active_form(HOME_FORM_ID);
        self.sync_start_menu_focus(recents);
    }

    fn build_launcher_table_model(&self, recents: &[String]) -> UiTableModel {
        match self.launcher_category {
            LauncherCategory::Recents => {
                let visible = Self::launcher_visible_rows_for_category(LauncherCategory::Recents);
                let rows = self
                    .start_menu_cache
                    .iter()
                    .take(visible)
                    .enumerate()
                    .map(|(idx, preview)| UiTableRow {
                        id: idx as u16,
                        height: 99,
                        usable: true,
                        selectable: true,
                        data: idx as u32,
                        cells: vec![UiTableCell {
                            text: preview.title.clone(),
                        }],
                    })
                    .collect();
                UiTableModel {
                    cols: 1,
                    columns: vec![UiTableColumn {
                        width: 0,
                        spacing: 0,
                        usable: true,
                    }],
                    top_row: 0,
                    selected_row: self.selected_content_index(recents).map(|idx| idx as u16),
                    selected_col: Some(0),
                    rows,
                }
            }
            LauncherCategory::Apps => {
                let selected_index = self.selected_content_index(recents).unwrap_or(0);
                let selected_row = (selected_index / APP_GRID_COLS) as u16;
                let selected_col = (selected_index % APP_GRID_COLS) as u16;
                let rows = self
                    .installed_apps
                    .chunks(APP_GRID_COLS)
                    .enumerate()
                    .map(|(row_idx, apps)| UiTableRow {
                        id: row_idx as u16,
                        height: 120,
                        usable: true,
                        selectable: true,
                        data: (row_idx * APP_GRID_COLS) as u32,
                        cells: apps
                            .iter()
                            .map(|app| UiTableCell {
                                text: app.title.clone(),
                            })
                            .collect(),
                    })
                    .collect();
                UiTableModel {
                    cols: APP_GRID_COLS as u16,
                    columns: (0..APP_GRID_COLS)
                        .map(|_| UiTableColumn {
                            width: 0,
                            spacing: 0,
                            usable: true,
                        })
                        .collect(),
                    rows,
                    top_row: self.apps_top_row as u16,
                    selected_row: Some(selected_row),
                    selected_col: Some(selected_col),
                }
            }
            LauncherCategory::Books => {
                let rows = self
                    .books_cache
                    .iter()
                    .enumerate()
                    .map(|(idx, book)| UiTableRow {
                        id: idx as u16,
                        height: 99,
                        usable: true,
                        selectable: true,
                        data: idx as u32,
                        cells: vec![UiTableCell {
                            text: book.title.clone(),
                        }],
                    })
                    .collect();
                UiTableModel {
                    cols: 1,
                    columns: vec![UiTableColumn {
                        width: 0,
                        spacing: 0,
                        usable: true,
                    }],
                    top_row: self.books_top_row as u16,
                    selected_row: self.selected_content_index(recents).map(|idx| idx as u16),
                    selected_col: Some(0),
                    rows,
                }
            }
            LauncherCategory::Images => {
                let rows = self
                    .images_cache
                    .iter()
                    .enumerate()
                    .map(|(idx, image)| UiTableRow {
                        id: idx as u16,
                        height: 99,
                        usable: true,
                        selectable: true,
                        data: idx as u32,
                        cells: vec![UiTableCell {
                            text: image.title.clone(),
                        }],
                    })
                    .collect();
                UiTableModel {
                    cols: 1,
                    columns: vec![UiTableColumn {
                        width: 0,
                        spacing: 0,
                        usable: true,
                    }],
                    top_row: self.images_top_row as u16,
                    selected_row: self.selected_content_index(recents).map(|idx| idx as u16),
                    selected_col: Some(0),
                    rows,
                }
            }
        }
    }

    fn sync_start_menu_focus(&mut self, recents: &[String]) {
        let target = if self.install_dialog.is_some() {
            Some(HOME_OBJ_DIALOG_DISMISS)
        } else if self.category_menu_open {
            Some(Self::category_menu_object_id(self.category_menu_index.min(3)))
        } else if self.start_menu_section == StartMenuSection::Actions {
            Some(HOME_OBJ_CATEGORY_TRIGGER)
        } else if self.start_menu_content_len(recents) > 0 {
            Some(Self::launcher_object_id_for_index(
                self.start_menu_index.min(self.start_menu_content_len(recents).saturating_sub(1)),
            ))
        } else {
            Some(HOME_OBJ_CATEGORY_TRIGGER)
        };
        self.prev_focus_object_id = self.ui_runtime.focus.object_id;
        self.ui_runtime.set_focus(HOME_FORM_ID, target);
        self.sync_selection_from_ui_focus(recents);
    }

    fn sync_selection_from_ui_focus(&mut self, recents: &[String]) {
        let Some(object_id) = self.ui_runtime.focus.object_id else {
            return;
        };
        if object_id == HOME_OBJ_DIALOG_DISMISS {
            return;
        }
        if object_id == HOME_OBJ_CATEGORY_TRIGGER {
            self.start_menu_section = StartMenuSection::Actions;
            return;
        }
        if let Some(index) = Self::category_index_from_object_id(object_id) {
            self.start_menu_section = StartMenuSection::Actions;
            self.category_menu_index = index.min(3);
            return;
        }
        if let Some(index) = Self::content_index_from_object_id(object_id) {
            let content_len = self.start_menu_content_len(recents);
            if index < content_len {
                self.start_menu_section = StartMenuSection::Recents;
                self.start_menu_index = index;
            }
        }
    }

    fn set_content_focus(&mut self, recents: &[String], index: usize) {
        self.start_menu_section = StartMenuSection::Recents;
        self.start_menu_index = index.min(self.start_menu_content_len(recents).saturating_sub(1));
        if self.ensure_launcher_selection_visible(recents) {
            self.start_menu_need_base_refresh = true;
        }
        self.prev_focus_object_id = self.ui_runtime.focus.object_id;
        self.ui_runtime
            .set_focus(HOME_FORM_ID, Some(Self::launcher_object_id_for_index(self.start_menu_index)));
    }

    fn set_actions_focus(&mut self) {
        self.start_menu_section = StartMenuSection::Actions;
        self.prev_focus_object_id = self.ui_runtime.focus.object_id;
        self.ui_runtime
            .set_focus(HOME_FORM_ID, Some(HOME_OBJ_CATEGORY_TRIGGER));
    }

    fn focused_object_id(&self) -> Option<ObjectId> {
        self.ui_runtime.focus.object_id
    }

    fn action_trigger_focused(&self) -> bool {
        self.focused_object_id() == Some(HOME_OBJ_CATEGORY_TRIGGER) && !self.category_menu_open
    }

    fn selected_category_menu_index(&self) -> Option<usize> {
        self.focused_object_id().and_then(Self::category_index_from_object_id)
    }

    fn selected_content_index(&self, recents: &[String]) -> Option<usize> {
        let object_id = self.focused_object_id()?;
        let index = Self::content_index_from_object_id(object_id)?;
        let content_len = self.start_menu_content_len(recents);
        (index < content_len).then_some(index)
    }

    fn previous_content_index(&self, recents: &[String]) -> Option<usize> {
        let object_id = self.prev_focus_object_id?;
        let index = Self::content_index_from_object_id(object_id)?;
        let content_len = self.start_menu_content_len(recents);
        (index < content_len).then_some(index)
    }

    fn launcher_event_from_buttons(
        buttons: &crate::input::ButtonState,
    ) -> Option<UiEvent> {
        use crate::input::Buttons;
        if buttons.is_pressed(Buttons::Up) {
            Some(UiEvent::ButtonDown { button: ButtonId::Up })
        } else if buttons.is_pressed(Buttons::Down) {
            Some(UiEvent::ButtonDown { button: ButtonId::Down })
        } else if buttons.is_pressed(Buttons::Left) {
            Some(UiEvent::ButtonDown { button: ButtonId::Left })
        } else if buttons.is_pressed(Buttons::Right) {
            Some(UiEvent::ButtonDown { button: ButtonId::Right })
        } else if buttons.is_pressed(Buttons::Confirm) {
            Some(UiEvent::ButtonDown { button: ButtonId::Confirm })
        } else if buttons.is_pressed(Buttons::Back) {
            Some(UiEvent::ButtonDown { button: ButtonId::Back })
        } else {
            None
        }
    }

    pub fn handle_start_menu_input(
        &mut self,
        recents: &[String],
        buttons: &crate::input::ButtonState,
    ) -> HomeAction {
        self.sync_start_menu_ui(recents);
        let content_len = self.start_menu_content_len(recents);
        if content_len > 0 && self.start_menu_index >= content_len {
            self.start_menu_index = content_len.saturating_sub(1);
        }
        self.sync_selection_from_ui_focus(recents);
        let categories = [
            LauncherCategory::Recents,
            LauncherCategory::Apps,
            LauncherCategory::Books,
            LauncherCategory::Images,
        ];
        let event = Self::launcher_event_from_buttons(buttons);

        if self.install_dialog.is_some() {
            if matches!(
                event,
                Some(UiEvent::ButtonDown {
                    button: ButtonId::Confirm | ButtonId::Back
                })
            ) {
                self.install_dialog = None;
                self.start_menu_need_base_refresh = true;
                self.sync_start_menu_ui(recents);
            }
            return HomeAction::None;
        }

        if self.category_menu_open {
            if matches!(event, Some(UiEvent::ButtonDown { button: ButtonId::Up })) {
                self.category_menu_index = self.category_menu_index.saturating_sub(1);
                self.ui_runtime.set_focus(
                    HOME_FORM_ID,
                    Some(Self::category_menu_object_id(self.category_menu_index)),
                );
                self.start_menu_nav_pending = true;
                return HomeAction::None;
            }
            if matches!(event, Some(UiEvent::ButtonDown { button: ButtonId::Down })) {
                self.category_menu_index =
                    (self.category_menu_index + 1).min(categories.len().saturating_sub(1));
                self.ui_runtime.set_focus(
                    HOME_FORM_ID,
                    Some(Self::category_menu_object_id(self.category_menu_index)),
                );
                self.start_menu_nav_pending = true;
                return HomeAction::None;
            }
            if matches!(event, Some(UiEvent::ButtonDown { button: ButtonId::Confirm })) {
                self.launcher_category = categories[self.category_menu_index];
                self.category_menu_open = false;
                self.set_content_focus(recents, 0);
                self.start_menu_need_base_refresh = true;
                self.sync_start_menu_ui(recents);
                return HomeAction::None;
            }
            if matches!(
                event,
                Some(UiEvent::ButtonDown {
                    button: ButtonId::Back | ButtonId::Left
                })
            ) {
                self.category_menu_open = false;
                self.set_actions_focus();
                self.start_menu_nav_pending = true;
                return HomeAction::None;
            }
            return HomeAction::None;
        }

        if matches!(event, Some(UiEvent::ButtonDown { button: ButtonId::Up })) {
            if self.start_menu_section == StartMenuSection::Recents {
                if self.launcher_category == LauncherCategory::Apps {
                    if content_len > 0 && self.start_menu_index >= APP_GRID_COLS {
                        self.set_content_focus(recents, self.start_menu_index - APP_GRID_COLS);
                    } else {
                        self.set_actions_focus();
                    }
                } else if content_len > 0 && self.start_menu_index > 0 {
                    self.set_content_focus(recents, self.start_menu_index - 1);
                } else {
                    self.set_actions_focus();
                }
            }
            self.start_menu_nav_pending = true;
            return HomeAction::None;
        }

        if matches!(event, Some(UiEvent::ButtonDown { button: ButtonId::Down })) {
            if self.start_menu_section == StartMenuSection::Actions {
                self.set_content_focus(recents, 0);
            } else if self.launcher_category == LauncherCategory::Apps {
                if content_len > 0 {
                    let next = self.start_menu_index + APP_GRID_COLS;
                    if next < content_len {
                        self.set_content_focus(recents, next);
                    }
                }
            } else if content_len > 0 && self.start_menu_index + 1 < content_len {
                self.set_content_focus(recents, self.start_menu_index + 1);
            }
            self.start_menu_nav_pending = true;
            return HomeAction::None;
        }

        if matches!(event, Some(UiEvent::ButtonDown { button: ButtonId::Left })) {
            if self.start_menu_section == StartMenuSection::Recents {
                if self.launcher_category == LauncherCategory::Apps {
                    if content_len > 0 {
                        let row_start = (self.start_menu_index / APP_GRID_COLS) * APP_GRID_COLS;
                        if self.start_menu_index > row_start {
                            self.set_content_focus(recents, self.start_menu_index - 1);
                            self.start_menu_nav_pending = true;
                        }
                    }
                } else {
                    self.set_actions_focus();
                    self.start_menu_nav_pending = true;
                }
            }
            return HomeAction::None;
        }

        if matches!(event, Some(UiEvent::ButtonDown { button: ButtonId::Right })) {
            if self.start_menu_section == StartMenuSection::Actions {
                self.set_content_focus(recents, self.start_menu_index);
                self.start_menu_nav_pending = true;
            } else if self.start_menu_section == StartMenuSection::Recents
                && self.launcher_category == LauncherCategory::Apps
                && content_len > 0
            {
                let row_start = (self.start_menu_index / APP_GRID_COLS) * APP_GRID_COLS;
                let row_end = (row_start + APP_GRID_COLS).min(content_len).saturating_sub(1);
                if self.start_menu_index < row_end {
                    self.set_content_focus(recents, self.start_menu_index + 1);
                    self.start_menu_nav_pending = true;
                }
            }
            return HomeAction::None;
        }

        if matches!(event, Some(UiEvent::ButtonDown { button: ButtonId::Confirm })) {
            if self.start_menu_section == StartMenuSection::Actions {
                self.category_menu_open = true;
                self.category_menu_index = categories
                    .iter()
                    .position(|c| *c == self.launcher_category)
                    .unwrap_or(0);
                self.sync_start_menu_ui(recents);
                self.start_menu_nav_pending = true;
                return HomeAction::None;
            } else if self.launcher_category == LauncherCategory::Recents {
                if let Some(path) = recents.get(self.start_menu_index) {
                    return HomeAction::OpenRecent(path.clone());
                }
            } else if self.launcher_category == LauncherCategory::Apps {
                if let Some(app) = self.installed_apps.get(self.start_menu_index) {
                    return HomeAction::OpenRecent(app.path.clone());
                }
            } else if self.launcher_category == LauncherCategory::Books {
                if let Some(book) = self.books_cache.get(self.start_menu_index) {
                    return HomeAction::OpenRecent(book.path.clone());
                }
            } else if self.launcher_category == LauncherCategory::Images {
                if let Some(image) = self.images_cache.get(self.start_menu_index) {
                    return HomeAction::OpenRecent(image.path.clone());
                }
            }
        }

        HomeAction::None
    }

    fn start_menu_content_len(&self, recents: &[String]) -> usize {
        match self.launcher_category {
            LauncherCategory::Recents => recents
                .len()
                .min(Self::launcher_visible_rows_for_category(LauncherCategory::Recents)),
            LauncherCategory::Apps => self.installed_apps.len(),
            LauncherCategory::Books => self.books_cache.len(),
            LauncherCategory::Images => self.images_cache.len(),
        }
    }

    pub fn draw_start_menu<S: AppSource>(
        &mut self,
        ctx: &mut HomeRenderContext<'_, S>,
        display: &mut impl Display,
        recents: &[String],
    ) {
        let size = ctx.display_buffers.size();
        let width = size.width as i32;
        let height = size.height as i32;
        let mid_y = height - START_MENU_MARGIN;

        self.ensure_start_menu_cache(ctx, recents);
        if self.launcher_category == LauncherCategory::Books {
            self.ensure_books_cache(ctx);
        } else if self.launcher_category == LauncherCategory::Images {
            self.ensure_images_cache(ctx);
        }
        self.ensure_installed_apps_cache(ctx);
        self.clamp_launcher_top_row_for_category(recents, self.launcher_category);
        self.sync_start_menu_ui(recents);

        let list_top = HEADER_Y + 28;
        let max_items = 6usize;
        let list_width = width - (START_MENU_MARGIN * 2);
        let item_height = 99;
        let thumb_size = 74;
        let header_rect = Rect::new(0, 0, width, (list_top + 8).min(height));
        let content_rect = Rect::new(
            START_MENU_MARGIN - 4,
            list_top - 4,
            list_width + 8,
            (mid_y - list_top + 8).max(1),
        );
        let menu_refresh_rect = Rect::new(0, 0, width, mid_y.max(1));
        let category_popup_rect = Rect::new(width - 140, START_MENU_FORM_Y, 136, 84);

        if self.start_menu_need_base_refresh {
            let (gray2_used, draw_count) = self.render_start_menu_contents(
                ctx,
                true,
                width,
                mid_y,
                list_top,
                max_items,
                list_width,
                item_height,
                thumb_size,
            );
            log::info!(
                "Start menu base render: recents={}, cache={}",
                draw_count,
                self.start_menu_cache.len()
            );
            if gray2_used {
                merge_bw_into_gray2(ctx.display_buffers, ctx.gray2_lsb, ctx.gray2_msb);
                let lsb: &[u8; BUFFER_SIZE] = ctx.gray2_lsb.as_ref().try_into().unwrap();
                let msb: &[u8; BUFFER_SIZE] = ctx.gray2_msb.as_ref().try_into().unwrap();
                display.copy_grayscale_buffers(lsb, msb);
                display.display_absolute_grayscale(ctx.render_policy.absolute_grayscale_mode);
                ctx.display_buffers.copy_active_to_inactive();
            } else {
                let mut rq = RenderQueue::default();
                rq.push(menu_refresh_rect, ctx.render_policy.refresh_mode(ctx.full_refresh));
                flush_queue(display, ctx.display_buffers, &mut rq, RefreshMode::Full);
            }
            self.start_menu_need_base_refresh = false;
            self.render_start_menu_contents(
                ctx,
                false,
                width,
                mid_y,
                list_top,
                max_items,
                list_width,
                item_height,
                thumb_size,
            );
            if self.launcher_category == LauncherCategory::Recents {
                if let Some(selected_index) = self.selected_content_index(recents) {
                    if selected_index < max_items {
                        let y = list_top + (selected_index as i32 * item_height);
                        if y + item_height <= mid_y {
                            let mut rq = RenderQueue::default();
                            rq.push(
                                Rect::new(START_MENU_MARGIN - 4, y - 4, list_width + 8, item_height - 4),
                                ctx.render_policy.partial_refresh_mode(),
                            );
                            flush_queue(
                                display,
                                ctx.display_buffers,
                                &mut rq,
                                ctx.render_policy.partial_refresh_mode(),
                            );
                        }
                    }
                }
            }
            return;
        }

        let (gray2_used, draw_count) = self.render_start_menu_contents(
            ctx,
            false,
            width,
            mid_y,
            list_top,
            max_items,
            list_width,
            item_height,
            thumb_size,
        );
        log::info!(
            "Start menu render: recents={}, cache={}",
            draw_count,
            self.start_menu_cache.len()
        );
        if gray2_used {
            if self.start_menu_nav_pending {
                let mut rq = RenderQueue::default();
                if self.category_menu_open {
                    rq.push(category_popup_rect, ctx.render_policy.partial_refresh_mode());
                } else if self.action_trigger_focused() {
                    rq.push(header_rect, ctx.render_policy.partial_refresh_mode());
                } else if self.launcher_category != LauncherCategory::Recents {
                    rq.push(content_rect, ctx.render_policy.partial_refresh_mode());
                } else if self.launcher_category == LauncherCategory::Recents {
                    for idx in [self.previous_content_index(recents), self.selected_content_index(recents)]
                        .into_iter()
                        .flatten()
                    {
                        if idx < max_items {
                            let y = list_top + (idx as i32 * item_height);
                            if y + item_height <= mid_y {
                                rq.push(
                                    Rect::new(
                                        START_MENU_MARGIN - 4,
                                        y - 4,
                                        list_width + 8,
                                        item_height - 4,
                                    ),
                                    ctx.render_policy.partial_refresh_mode(),
                                );
                            }
                        }
                    }
                }
                flush_queue(
                    display,
                    ctx.display_buffers,
                    &mut rq,
                    ctx.render_policy.partial_refresh_mode(),
                );
                self.start_menu_nav_pending = false;
            } else {
                let mut rq = RenderQueue::default();
                if self.category_menu_open {
                    rq.push(category_popup_rect, ctx.render_policy.partial_refresh_mode());
                } else if self.action_trigger_focused() {
                    rq.push(header_rect, ctx.render_policy.partial_refresh_mode());
                } else {
                    rq.push(header_rect, ctx.render_policy.partial_refresh_mode());
                    rq.push(content_rect, ctx.render_policy.partial_refresh_mode());
                }
                flush_queue(
                    display,
                    ctx.display_buffers,
                    &mut rq,
                    ctx.render_policy.partial_refresh_mode(),
                );
            }
        } else {
            let mut rq = RenderQueue::default();
            rq.push(header_rect, ctx.render_policy.refresh_mode(ctx.full_refresh));
            rq.push(content_rect, ctx.render_policy.refresh_mode(ctx.full_refresh));
            flush_queue(display, ctx.display_buffers, &mut rq, RefreshMode::Full);
        }
    }

    fn render_start_menu_contents<S: AppSource>(
        &mut self,
        ctx: &mut HomeRenderContext<'_, S>,
        _suppress_selection: bool,
        width: i32,
        mid_y: i32,
        list_top: i32,
        _max_items: usize,
        list_width: i32,
        _item_height: i32,
        thumb_size: i32,
    ) -> (bool, usize) {
        let chrome = PalmChromeMetrics::palm3();
        let header_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        ctx.display_buffers.clear(BinaryColor::On).ok();
        ctx.gray2_lsb.fill(0);
        ctx.gray2_msb.fill(0);
        let gray2_used = false;

        Rectangle::new(
            Point::new(0, 0),
            Size::new(width as u32, START_MENU_STATUS_H as u32),
        )
        .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
            BinaryColor::On,
        ))
        .draw(ctx.display_buffers)
        .ok();
        let battery = ctx.battery_percent.unwrap_or(100);
        let battery_text = format!("{}%", battery);
        let batt_w = chrome.px(34);
        let batt_h = chrome.px(8);
        let cap_w = chrome.px(2);
        let cap_h = chrome.px(4);
        let batt_total_w = batt_w + cap_w;
        let batt_x = (width - batt_total_w) / 2;
        let batt_y = ((START_MENU_STATUS_H - batt_h) / 2) + 2;
        Rectangle::new(
            Point::new(batt_x, batt_y),
            Size::new(batt_w as u32, batt_h as u32),
        )
        .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
            BinaryColor::Off,
        ))
        .draw(ctx.display_buffers)
        .ok();
        Rectangle::new(
            Point::new(batt_x + batt_w, batt_y + (batt_h - cap_h) / 2),
            Size::new(cap_w as u32, cap_h as u32),
        )
        .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
            BinaryColor::Off,
        ))
        .draw(ctx.display_buffers)
        .ok();
        if !ctx.palm_fonts.is_empty() {
            let battery_font_id = 0u8; // Palm standard
            let battery_scale = 1;
            let battery_text_w =
                palm_text_width(&battery_text, battery_font_id, ctx.palm_fonts, battery_scale);
            let battery_text_h =
                palm_text_height(battery_font_id, ctx.palm_fonts, battery_scale);
            let battery_text_x = batt_x + (batt_w - battery_text_w) / 2;
            let battery_text_y = batt_y + (batt_h - battery_text_h) / 2;
            draw_palm_text(
                ctx.display_buffers,
                &battery_text,
                battery_text_x,
                battery_text_y,
                battery_font_id,
                ctx.palm_fonts,
                battery_scale,
                BinaryColor::On,
            );
        } else {
            let status_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
            let battery_text_x = batt_x + (batt_w - (battery_text.len() as i32 * 10)) / 2;
            let battery_text_y = batt_y + batt_h - 7;
            Text::new(
                &battery_text,
                Point::new(battery_text_x, battery_text_y),
                status_style,
            )
            .draw(ctx.display_buffers)
            .ok();
        }

        let form_x = 2;
        let form_y = START_MENU_FORM_Y;
        let form_w = (width - 4).max(1);
        let title_font_id = 1u8;
        let category_font_id = 0u8;
        let ui_scale_num = 6;
        let ui_scale_den = 5;
        let home_text_w = palm_text_width_scaled(
            "Home",
            title_font_id,
            ctx.palm_fonts,
            ui_scale_num,
            ui_scale_den,
        );
        let home_text_h = palm_text_height_scaled(
            title_font_id,
            ctx.palm_fonts,
            ui_scale_num,
            ui_scale_den,
        );
        let title_pad_x = chrome.px(3);
        let title_pad_top = 2;
        let title_pad_bottom = 5;
        let tab_w = home_text_w + title_pad_x * 2;
        let tab_h = home_text_h + title_pad_top + title_pad_bottom;
        let title_layout = draw_form_title_bar(
            ctx.display_buffers,
            form_x,
            form_y,
            form_w,
            tab_w,
            tab_h,
            4,
        );
        let home_x = title_layout.tab_x + title_pad_x;
        let home_y = title_layout.tab_y + title_pad_top;
        if !ctx.palm_fonts.is_empty() {
            draw_palm_text_scaled(
                ctx.display_buffers,
                "Home",
                home_x,
                home_y,
                title_font_id,
                ctx.palm_fonts,
                ui_scale_num,
                ui_scale_den,
                BinaryColor::On,
            );
        } else {
            Text::new(
                "Home",
                Point::new(title_layout.tab_x + title_pad_x, title_layout.tab_y + title_pad_top + 11),
                MonoTextStyle::new(&FONT_10X20, BinaryColor::On),
            )
            .draw(ctx.display_buffers)
            .ok();
        }

        // Palm-style right-justified category selector: arrow + plain text.
        let trigger_y = home_y;
        let category_label = match self.launcher_category {
            LauncherCategory::Recents => "Recents",
            LauncherCategory::Apps => "Apps",
            LauncherCategory::Books => "Books",
            LauncherCategory::Images => "Images",
        };
        let cat_w = if !ctx.palm_fonts.is_empty() {
            palm_text_width_scaled(
                category_label,
                category_font_id,
                ctx.palm_fonts,
                ui_scale_num,
                ui_scale_den,
            )
        } else {
            (category_label.len() as i32) * 10
        };
        let cat_h = if !ctx.palm_fonts.is_empty() {
            palm_text_height_scaled(
                category_font_id,
                ctx.palm_fonts,
                ui_scale_num,
                ui_scale_den,
            )
        } else {
            20
        };
        let right_edge = form_x + form_w - 6;
        let arrow_x = right_edge - cat_w - 14;
        let text_x = right_edge - cat_w;
        self.last_category_trigger_rect = Some(Rect::new(
            arrow_x - 4,
            trigger_y - 1,
            cat_w + 18,
            cat_h + 2,
        ));
        if self.action_trigger_focused() {
            Rectangle::new(
                Point::new(arrow_x - 4, trigger_y - 1),
                Size::new((cat_w + 18) as u32, (cat_h + 2) as u32),
            )
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                BinaryColor::Off,
            ))
            .draw(ctx.display_buffers)
            .ok();
        }
        let arrow_color = if self.action_trigger_focused() {
            BinaryColor::On
        } else {
            BinaryColor::Off
        };
        if !ctx.palm_fonts.is_empty() {
            draw_palm_text_scaled(
                ctx.display_buffers,
                category_label,
                text_x,
                trigger_y,
                category_font_id,
                ctx.palm_fonts,
                ui_scale_num,
                ui_scale_den,
                arrow_color,
            );
        } else {
            Text::new(
                category_label,
                Point::new(text_x, trigger_y + 11),
                MonoTextStyle::new(
                    &FONT_10X20,
                    arrow_color,
                ),
            )
            .draw(ctx.display_buffers)
            .ok();
        }
        let arrow_y = trigger_y + (cat_h / 2) - 1;
        Rectangle::new(Point::new(arrow_x, arrow_y), Size::new(7, 1))
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                arrow_color,
            ))
            .draw(ctx.display_buffers)
            .ok();
        Rectangle::new(Point::new(arrow_x + 1, arrow_y + 1), Size::new(5, 1))
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                arrow_color,
            ))
            .draw(ctx.display_buffers)
            .ok();
        Rectangle::new(Point::new(arrow_x + 2, arrow_y + 2), Size::new(3, 1))
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                arrow_color,
            ))
            .draw(ctx.display_buffers)
            .ok();
        Rectangle::new(Point::new(arrow_x + 3, arrow_y + 3), Size::new(1, 1))
            .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                arrow_color,
            ))
            .draw(ctx.display_buffers)
            .ok();

        let mut draw_count = 0usize;
        if matches!(
            self.launcher_category,
            LauncherCategory::Recents
                | LauncherCategory::Apps
                | LauncherCategory::Books
                | LauncherCategory::Images
        ) {
            let model = self.build_launcher_table_model(&self.start_menu_cache.iter().map(|p| p.path.clone()).collect::<Vec<_>>());
            draw_count = model.rows.len();
            if draw_count == 0 {
                let msg = match self.launcher_category {
                    LauncherCategory::Apps => "No installed apps yet.",
                    LauncherCategory::Books => "No books found.",
                    LauncherCategory::Images => "No images found.",
                    _ => "No recent items.",
                };
                Text::new(msg, Point::new(START_MENU_MARGIN, list_top + 24), header_style)
                    .draw(ctx.display_buffers)
                    .ok();
            } else {
                let content_h = (mid_y - list_top).max(1);
                let has_scrollbar = matches!(
                    self.launcher_category,
                    LauncherCategory::Apps | LauncherCategory::Books | LauncherCategory::Images
                );
                let scrollbar_w = if has_scrollbar { 12 } else { 0 };
                let table_rect = Rect::new(
                    START_MENU_MARGIN - 4,
                    list_top - 4,
                    list_width + 8 - scrollbar_w,
                    content_h,
                );
                let scrollbar_rect = Rect::new(
                    table_rect.x + table_rect.w + 1,
                    list_top,
                    scrollbar_w.saturating_sub(1),
                    content_h - 8,
                );
                self.last_table_touch_rect = Some(table_rect);
                self.last_scrollbar_rect = if has_scrollbar {
                    Some(scrollbar_rect)
                } else {
                    None
                };
                let mut ui = UiContext {
                    buffers: ctx.display_buffers,
                    render_policy: ctx.render_policy,
                };
                let mut table = TableView::new(&model);
                table.clear = false;
                if matches!(
                    self.launcher_category,
                    LauncherCategory::Recents | LauncherCategory::Books | LauncherCategory::Images
                ) {
                    let previews = match self.launcher_category {
                        LauncherCategory::Recents => &self.start_menu_cache,
                        LauncherCategory::Books => &self.books_cache,
                        LauncherCategory::Images => &self.images_cache,
                        LauncherCategory::Apps => &self.start_menu_cache,
                    };
                    let renderer = RecentTableRenderer {
                        previews,
                        thumb_size,
                        palm_fonts: ctx.palm_fonts,
                        render_policy: ctx.render_policy,
                        draw_trbk_image: ctx.draw_trbk_image,
                    };
                    table.renderer = Some(&renderer);
                    table.render(&mut ui, table_rect, &mut RenderQueue::default());
                } else {
                    let renderer = AppsTableRenderer {
                        apps: &self.installed_apps,
                        palm_fonts: ctx.palm_fonts,
                        render_policy: ctx.render_policy,
                        draw_trbk_image: ctx.draw_trbk_image,
                    };
                    table.renderer = Some(&renderer);
                    table.render(&mut ui, table_rect, &mut RenderQueue::default());
                }
                if has_scrollbar {
                    let total_rows =
                        self.launcher_total_rows_for_category(&self.start_menu_cache.iter().map(|p| p.path.clone()).collect::<Vec<_>>(), self.launcher_category);
                    let visible_rows = Self::launcher_visible_rows_for_category(self.launcher_category)
                        .min(total_rows.max(1));
                    let top_row = self.launcher_top_row_for_category(self.launcher_category);
                    if total_rows > visible_rows {
                        let mut scrollbar = TableScrollBarView::new(top_row, visible_rows, total_rows);
                        scrollbar.render(&mut ui, scrollbar_rect, &mut RenderQueue::default());
                    }
                }
            }
        } else {
            let msg = match self.launcher_category {
                LauncherCategory::Apps => "No installed apps yet.",
                LauncherCategory::Books => "No books in launcher yet.",
                LauncherCategory::Images => "No images found.",
                LauncherCategory::Recents => "No recent items.",
            };
            Text::new(msg, Point::new(START_MENU_MARGIN, list_top + 24), header_style)
                .draw(ctx.display_buffers)
                .ok();
        }

        if self.category_menu_open {
            let menu_items = ["Recents", "Apps", "Books", "Images"];
            let item_font_id = 0u8;
            let item_scale_num = 6;
            let item_scale_den = 5;
            let item_text_h = if !ctx.palm_fonts.is_empty() {
                palm_text_height_scaled(
                    item_font_id,
                    ctx.palm_fonts,
                    item_scale_num,
                    item_scale_den,
                )
            } else {
                10
            };
            let max_text_w = if !ctx.palm_fonts.is_empty() {
                menu_items
                    .iter()
                    .map(|label| {
                        palm_text_width_scaled(
                            label,
                            item_font_id,
                            ctx.palm_fonts,
                            item_scale_num,
                            item_scale_den,
                        )
                    })
                    .max()
                    .unwrap_or(0)
            } else {
                menu_items.iter().map(|label| (label.len() as i32) * 6).max().unwrap_or(0)
            };
            let item_h = item_text_h + 6;
            let menu_w = (max_text_w + 14).max(48);
            let menu_x = form_x + form_w - menu_w - 4;
            let menu_y = form_y + 1;
            let menu_h = item_h * menu_items.len() as i32 + 4;
            self.last_category_popup_rect = Some(Rect::new(menu_x, menu_y, menu_w, menu_h));
            draw_palm_pull_down_box(ctx.display_buffers, menu_x, menu_y, menu_w, menu_h);
            for (i, label) in menu_items.iter().enumerate() {
                let y = menu_y + 3 + (i as i32 * item_h);
                let selected = self.selected_category_menu_index() == Some(i);
                if selected {
                    Rectangle::new(
                        Point::new(menu_x + 1, y - 1),
                        Size::new((menu_w - 2) as u32, (item_h - 1) as u32),
                    )
                    .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                        BinaryColor::Off,
                    ))
                    .draw(ctx.display_buffers)
                    .ok();
                }
                if !ctx.palm_fonts.is_empty() {
                    draw_palm_text_scaled(
                        ctx.display_buffers,
                        label,
                        menu_x + 6,
                        y + 2,
                        item_font_id,
                        ctx.palm_fonts,
                        item_scale_num,
                        item_scale_den,
                        if selected { BinaryColor::On } else { BinaryColor::Off },
                    );
                } else {
                    Text::new(
                        label,
                        Point::new(menu_x + 6, y + 10),
                        MonoTextStyle::new(
                            &FONT_6X10,
                            if selected { BinaryColor::On } else { BinaryColor::Off },
                        ),
                    )
                    .draw(ctx.display_buffers)
                    .ok();
                }
            }
        }

        if let Some(dialog) = self.install_dialog.as_ref() {
            self.draw_install_dialog(ctx.display_buffers, dialog, ctx.palm_fonts, width, mid_y);
        }

        (gray2_used, draw_count)
    }

    fn draw_install_dialog(
        &self,
        target: &mut DisplayBuffers,
        dialog: &InstallDialogState,
        fonts: &[crate::palm::runtime::PalmFont],
        width: i32,
        content_bottom: i32,
    ) {
        let w = ((width * 9) / 10).max(120);
        let x = (width - w) / 2;
        let y = 26;
        let title_font = 1u8;
        let body_font = 1u8;
        let title_h = if !fonts.is_empty() {
            palm_text_height(title_font, fonts, 1).max(10)
        } else {
            10
        };
        let line_h = if !fonts.is_empty() {
            (palm_text_height(body_font, fonts, 1) + 2).max(12)
        } else {
            12
        };
        let header_h = (title_h + 8).max(16);
        let body_lines = wrap_home_title_lines(&dialog.message, w - 12, fonts, body_font, 1, 1, 20);
        let body_h = (body_lines.len() as i32 * line_h).max(line_h * 2);
        let btn_label = "Done";
        let (btn_tw, btn_th) = if !fonts.is_empty() {
            (
                palm_text_width(btn_label, body_font, fonts, 1),
                palm_text_height(body_font, fonts, 1),
            )
        } else {
            (24, 10)
        };
        let btn_layout = auto_button_layout_for_label(x + 8, 0, btn_tw, btn_th, 36, 10, 7, 2);
        let h = (header_h + 8 + body_h + 8 + btn_layout.h + 10)
            .min(content_bottom - y - 2)
            .max(86);
        prc_alert::draw_alert_frame(target, x, y, w, h, header_h);

        let title_w = if !fonts.is_empty() {
            palm_text_width(&dialog.title, title_font, fonts, 1)
        } else {
            dialog.title.len() as i32 * 6
        };
        let title_x = x + ((w - title_w) / 2).max(4);
        let title_y = y + ((header_h - title_h) / 2).max(1);
        if !fonts.is_empty() {
            draw_palm_text(
                target,
                &dialog.title,
                title_x,
                title_y,
                title_font,
                fonts,
                1,
                BinaryColor::On,
            );
        } else {
            Text::new(
                &dialog.title,
                Point::new(title_x, y + header_h - 3),
                MonoTextStyle::new(&FONT_6X10, BinaryColor::On),
            )
            .draw(target)
            .ok();
        }

        let body_x = x + 6;
        let mut body_y = y + header_h + 6;
        for line in body_lines {
            if body_y + line_h > y + h - btn_layout.h - 12 {
                break;
            }
            if !fonts.is_empty() {
                draw_palm_text(target, &line, body_x, body_y, body_font, fonts, 1, BinaryColor::Off);
            } else {
                Text::new(
                    &line,
                    Point::new(body_x, body_y + 9),
                    MonoTextStyle::new(&FONT_6X10, BinaryColor::Off),
                )
                .draw(target)
                .ok();
            }
            body_y += line_h;
        }

        let btn_x = x + 8;
        let btn_y = y + h - btn_layout.h - 5;
        prc_alert::draw_done_button(target, btn_x, btn_y, btn_layout.w, btn_layout.h);
        let text_x = btn_x + ((btn_layout.w - btn_tw) / 2).max(1);
        let text_y = btn_y + ((btn_layout.h - btn_th) / 2).max(1);
        if !fonts.is_empty() {
            draw_palm_text(
                target,
                btn_label,
                text_x,
                text_y,
                body_font,
                fonts,
                1,
                BinaryColor::Off,
            );
        } else {
            Text::new(
                btn_label,
                Point::new(text_x, text_y + 9),
                MonoTextStyle::new(&FONT_6X10, BinaryColor::Off),
            )
            .draw(target)
            .ok();
        }
    }

    fn ensure_start_menu_cache<S: AppSource>(
        &mut self,
        ctx: &mut HomeRenderContext<'_, S>,
        recents: &[String],
    ) {
        if self.start_menu_cache_same(recents) {
            return;
        }
        self.start_menu_cache.clear();
        for path in recents {
            let (title, image) = self.load_recent_preview(ctx, path);
            self.start_menu_cache.push(RecentPreview {
                path: path.clone(),
                title,
                image,
            });
        }
        self.start_menu_need_base_refresh = true;
    }

    fn ensure_installed_apps_cache<S: AppSource>(&mut self, ctx: &mut HomeRenderContext<'_, S>) {
        let apps = ctx.source.list_installed_apps();
        if apps.len() != self.installed_apps.len()
            || apps
                .iter()
                .zip(self.installed_apps.iter())
                .any(|(a, b)| a.title != b.title || a.path != b.path)
        {
            self.installed_apps = apps;
            self.start_menu_need_base_refresh = true;
        }
    }

    fn ensure_books_cache<S: AppSource>(&mut self, ctx: &mut HomeRenderContext<'_, S>) {
        let mut paths = Vec::new();
        let mut stack = vec![Vec::<String>::new()];
        while let Some(path) = stack.pop() {
            let Ok(entries) = ctx.source.refresh(&path) else {
                continue;
            };
            for entry in entries {
                match entry.kind {
                    crate::image_viewer::EntryKind::Dir => {
                        let mut next = path.clone();
                        next.push(entry.name);
                        stack.push(next);
                    }
                    crate::image_viewer::EntryKind::File => {
                        let lower = entry.name.to_ascii_lowercase();
                        if lower.ends_with(".trbk") || lower.ends_with(".tbk") {
                            let mut full = String::new();
                            if !path.is_empty() {
                                full.push_str(&path.join("/"));
                                full.push('/');
                            }
                            full.push_str(&entry.name);
                            paths.push(full);
                        }
                    }
                }
            }
        }
        paths.sort();
        let signature = paths.join("\n");
        let cached_catalog = ctx.source.load_book_catalog();
        let mut cached_titles = alloc::collections::BTreeMap::new();
        if let Some((cached_sig, entries)) = cached_catalog {
            if cached_sig == signature {
                for (path, title) in entries {
                    cached_titles.insert(path, title);
                }
            }
        }

        let mut books = Vec::with_capacity(paths.len());
        let mut catalog_entries = Vec::with_capacity(paths.len());
        for path in paths {
            let (title, image) = if let Some(title) = cached_titles.get(&path) {
                let image = ctx.source.load_thumbnail(&path);
                (title.clone(), image)
            } else {
                self.load_recent_preview(ctx, &path)
            };
            catalog_entries.push((path.clone(), title.clone()));
            books.push(RecentPreview { path, title, image });
        }
        books.sort_by(|a, b| {
            let ta = a.title.to_ascii_lowercase();
            let tb = b.title.to_ascii_lowercase();
            ta.cmp(&tb).then_with(|| a.path.cmp(&b.path))
        });
        ctx.source.save_book_catalog(&signature, &catalog_entries);
        let changed = books.len() != self.books_cache.len()
            || books
                .iter()
                .zip(self.books_cache.iter())
                .any(|(a, b)| a.path != b.path || a.title != b.title);
        if changed {
            self.books_cache = books;
            self.start_menu_need_base_refresh = true;
        }
    }

    fn ensure_images_cache<S: AppSource>(&mut self, ctx: &mut HomeRenderContext<'_, S>) {
        let mut paths = Vec::new();
        let mut stack = vec![Vec::<String>::new()];
        while let Some(path) = stack.pop() {
            let Ok(entries) = ctx.source.refresh(&path) else {
                continue;
            };
            for entry in entries {
                match entry.kind {
                    crate::image_viewer::EntryKind::Dir => {
                        let mut next = path.clone();
                        next.push(entry.name);
                        stack.push(next);
                    }
                    crate::image_viewer::EntryKind::File => {
                        if is_launcher_image_name(&entry.name) {
                            let mut full = String::new();
                            if !path.is_empty() {
                                full.push_str(&path.join("/"));
                                full.push('/');
                            }
                            full.push_str(&entry.name);
                            paths.push(full);
                        }
                    }
                }
            }
        }
        paths.sort();
        let signature = paths.join("\n");
        let cached_catalog = ctx.source.load_image_catalog();
        let mut cached_labels = alloc::collections::BTreeMap::new();
        if let Some((cached_sig, entries)) = cached_catalog {
            if cached_sig == signature {
                for (path, label) in entries {
                    cached_labels.insert(path, label);
                }
            }
        }

        let mut images = Vec::with_capacity(paths.len());
        let mut catalog_entries = Vec::with_capacity(paths.len());
        for path in paths {
            let label = cached_labels
                .get(&path)
                .cloned()
                .unwrap_or_else(|| basename_from_path(&path));
            let image = if let Some(image) = ctx.source.load_thumbnail(&path) {
                Some(image)
            } else {
                let (_, image) = self.load_recent_preview(ctx, &path);
                image
            };
            catalog_entries.push((path.clone(), label.clone()));
            images.push(RecentPreview {
                path,
                title: label,
                image,
            });
        }
        images.sort_by(|a, b| {
            let ta = a.title.to_ascii_lowercase();
            let tb = b.title.to_ascii_lowercase();
            ta.cmp(&tb).then_with(|| a.path.cmp(&b.path))
        });
        ctx.source.save_image_catalog(&signature, &catalog_entries);
        let changed = images.len() != self.images_cache.len()
            || images
                .iter()
                .zip(self.images_cache.iter())
                .any(|(a, b)| a.path != b.path || a.title != b.title);
        if changed {
            self.images_cache = images;
            self.start_menu_need_base_refresh = true;
        }
    }

    fn load_recent_preview<S: AppSource>(
        &mut self,
        ctx: &mut HomeRenderContext<'_, S>,
        path: &str,
    ) -> (String, Option<ImageData>) {
        let label_fallback = basename_from_path(path);
        if let Some(image) = ctx.source.load_thumbnail(path) {
            let title = ctx
                .source
                .load_thumbnail_title(path)
                .filter(|value| !value.is_empty())
                .unwrap_or(label_fallback);
            if let Some(mono) = thumbnail_to_mono(&image) {
                if !matches!(image, ImageData::Mono1 { .. }) {
                    ctx.source.save_thumbnail(path, &mono);
                }
                return (title, Some(mono));
            }
            let needs_resize = match &image {
                ImageData::Mono1 { width, height, .. }
                | ImageData::Gray8 { width, height, .. }
                | ImageData::Gray2 { width, height, .. }
                | ImageData::Gray2Stream { width, height, .. } => {
                    *width != START_MENU_RECENT_THUMB as u32
                        || *height != START_MENU_RECENT_THUMB as u32
                }
            };
            if needs_resize {
                if let Some(thumb) = thumbnail_from_image(&image, START_MENU_RECENT_THUMB as u32) {
                    ctx.source.save_thumbnail(path, &thumb);
                    return (title, Some(thumb));
                }
            }
            return (title, Some(image));
        }
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".tri") || lower.ends_with(".trimg") {
            let mut parts: Vec<String> = path
                .split('/')
                .filter(|part| !part.is_empty())
                .map(|part| part.to_string())
                .collect();
            if parts.is_empty() {
                return (label_fallback, None);
            }
            let file = parts.pop().unwrap_or_default();
            let entry = ImageEntry {
                name: file,
                kind: crate::image_viewer::EntryKind::File,
            };
            if let Ok(image) = ctx.source.load(&parts, &entry) {
                if let ImageData::Gray2Stream { width, height, key } = &image {
                    if let Some(thumb) = ctx.source.load_gray2_stream_thumbnail(
                        key,
                        *width,
                        *height,
                        74,
                        74,
                    ) {
                        ctx.source.save_thumbnail(path, &thumb);
                        return (label_fallback, Some(thumb));
                    }
                }
                if let Some(thumb) = thumbnail_from_image(&image, 74) {
                    ctx.source.save_thumbnail(path, &thumb);
                    return (label_fallback, Some(thumb));
                }
            }
            return (label_fallback, None);
        }
        if !lower.ends_with(".trbk") && !lower.ends_with(".tbk") {
            if lower.ends_with(".prc") || lower.ends_with(".tdb") {
                let apps = ctx.source.list_installed_apps();
                if let Some(app) = apps
                    .iter()
                    .find(|app| same_launcher_path(&app.path, path))
                    .cloned()
                {
                    let title = if app.title.is_empty() {
                        label_fallback
                    } else {
                        app.title
                    };
                    if let Some(image) = app.icon.as_ref() {
                        ctx.source.save_thumbnail(path, image);
                        ctx.source.save_thumbnail_title(path, &title);
                        return (title, Some(image.clone()));
                    }
                    return (title, None);
                }
            }
            return (label_fallback, None);
        }
        let mut parts: Vec<String> = path
            .split('/')
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .collect();
        if parts.is_empty() {
            return (label_fallback, None);
        }
        let file = parts.pop().unwrap_or_default();
        let entry = ImageEntry {
            name: file,
            kind: crate::image_viewer::EntryKind::File,
        };
        let info = match ctx.source.open_trbk(&parts, &entry) {
            Ok(info) => info,
            Err(_) => {
                ctx.source.close_trbk();
                return (label_fallback, None);
            }
        };
        let title = if info.metadata.title.is_empty() {
            label_fallback
        } else {
            info.metadata.title.clone()
        };
        let preview = if !info.images.is_empty() {
            ctx.source.trbk_image(0).ok().and_then(|image| {
                if let ImageData::Gray2Stream { width, height, key } = &image {
                    if let Some(thumb) = ctx.source.load_gray2_stream_thumbnail(
                        key,
                        *width,
                        *height,
                        START_MENU_RECENT_THUMB as u32,
                        START_MENU_RECENT_THUMB as u32,
                    ) {
                        return Some(thumb);
                    }
                }
                thumbnail_from_image(&image, START_MENU_RECENT_THUMB as u32)
            })
        } else {
            None
        };
        ctx.source.close_trbk();
        if let Some(image) = preview.as_ref() {
            ctx.source.save_thumbnail(path, image);
            ctx.source.save_thumbnail_title(path, &title);
        }
        (title, preview)
    }
}


pub fn draw_icon_gray2(
    buffers: &mut DisplayBuffers,
    gray2_lsb: &mut [u8],
    gray2_msb: &mut [u8],
    gray2_used: &mut bool,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    dark_mask: &[u8],
    light_mask: &[u8],
) {
    if width <= 0 || height <= 0 {
        return;
    }
    let width_u = width as usize;
    let height_u = height as usize;
    let expected = (width_u * height_u + 7) / 8;
    if dark_mask.len() != expected || light_mask.len() != expected {
        return;
    }
    for yy in 0..height_u {
        for xx in 0..width_u {
            let idx = yy * width_u + xx;
            let byte = idx / 8;
            let bit = 7 - (idx % 8);
            let dark = (dark_mask[byte] >> bit) & 1 == 1;
            let light = (light_mask[byte] >> bit) & 1 == 1;
            if !dark && !light {
                continue;
            }
            *gray2_used = true;
            let dst_x = x + xx as i32;
            let dst_y = y + yy as i32;
            if dark {
                buffers.set_pixel(dst_x, dst_y, BinaryColor::Off);
            } else {
                buffers.set_pixel(dst_x, dst_y, BinaryColor::On);
            }
            let Some((fx, fy)) = map_display_point(buffers.rotation(), dst_x, dst_y) else {
                continue;
            };
            let dst_idx = fy * FB_WIDTH + fx;
            let dst_byte = dst_idx / 8;
            let dst_bit = 7 - (dst_idx % 8);
            if light {
                gray2_lsb[dst_byte] |= 1 << dst_bit;
            }
            if dark {
                gray2_msb[dst_byte] |= 1 << dst_bit;
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
        Rotation::Rotate90 => (y as usize, FB_HEIGHT - 1 - x as usize),
        Rotation::Rotate180 => (FB_WIDTH - 1 - x as usize, FB_HEIGHT - 1 - y as usize),
        Rotation::Rotate270 => (FB_WIDTH - 1 - y as usize, x as usize),
    };
    if x >= FB_WIDTH || y >= FB_HEIGHT {
        None
    } else {
        Some((x, y))
    }
}

fn basename_from_path(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn is_launcher_image_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".tri")
        || lower.ends_with(".trimg")
        || lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
}

fn same_launcher_path(a: &str, b: &str) -> bool {
    let na = a.trim_start_matches('/');
    let nb = b.trim_start_matches('/');
    na.eq_ignore_ascii_case(nb)
}

fn thumbnail_from_image(image: &ImageData, size: u32) -> Option<ImageData> {
    let (src_w, src_h) = match image {
        ImageData::Mono1 { width, height, .. } => (*width, *height),
        ImageData::Gray8 { width, height, .. } => (*width, *height),
        ImageData::Gray2 { width, height, .. } => (*width, *height),
        ImageData::Gray2Stream { width, height, .. } => (*width, *height),
    };
    if src_w == 0 || src_h == 0 {
        return None;
    }
    let dst_w = size;
    let dst_h = size;
    let dst_len = ((dst_w as usize * dst_h as usize) + 7) / 8;
    let mut bits = vec![0xFF; dst_len];
    for y in 0..dst_h {
        for x in 0..dst_w {
            let sx = (x * src_w) / dst_w;
            let sy = (y * src_h) / dst_h;
            let lum = match image {
                ImageData::Mono1 { width, bits, .. } => {
                    let idx = (sy * (*width) + sx) as usize;
                    let byte = bits[idx / 8];
                    let bit = 7 - (idx % 8);
                    if (byte >> bit) & 1 == 1 { 255 } else { 0 }
                }
                ImageData::Gray8 { width, pixels, .. } => {
                    let idx = (sy * (*width) + sx) as usize;
                    pixels.get(idx).copied().unwrap_or(255)
                }
                ImageData::Gray2 { width, height, data, .. } => {
                    let idx = (sy * (*width) + sx) as usize;
                    let byte = idx / 8;
                    let bit = 7 - (idx % 8);
                    let plane_len = (((*width) as usize * (*height) as usize) + 7) / 8;
                    if data.len() < plane_len * 3 {
                        255
                    } else {
                        let bw = (data[byte] >> bit) & 1;
                        let l = (data[plane_len + byte] >> bit) & 1;
                        let m = (data[plane_len * 2 + byte] >> bit) & 1;
                        match (m, l, bw) {
                            (0, 0, 1) => 255,
                            (0, 1, 1) => 192,
                            (1, 0, 0) => 128,
                            (1, 1, 0) => 64,
                            _ => 0,
                        }
                    }
                }
                ImageData::Gray2Stream { .. } => 255,
            };
            let dst_idx = (y * dst_w + x) as usize;
            let dst_byte = dst_idx / 8;
            let dst_bit = 7 - (dst_idx % 8);
            let lum = adjust_thumbnail_luma(lum);
            if lum >= 128 {
                bits[dst_byte] |= 1 << dst_bit;
            } else {
                bits[dst_byte] &= !(1 << dst_bit);
            }
        }
    }
    Some(ImageData::Mono1 {
        width: dst_w,
        height: dst_h,
        bits,
    })
}

fn thumbnail_to_mono(image: &ImageData) -> Option<ImageData> {
    match image {
        ImageData::Mono1 { .. } => Some(image.clone()),
        ImageData::Gray8 { width, height, pixels } => {
            let plane = ((*width as usize * *height as usize) + 7) / 8;
            let mut bits = vec![0xFF; plane];
            for idx in 0..(*width as usize * *height as usize) {
                let byte = idx / 8;
                let bit = 7 - (idx % 8);
                let lum = pixels.get(idx).copied().unwrap_or(255);
                let lum = adjust_thumbnail_luma(lum);
                if lum >= 128 {
                    bits[byte] |= 1 << bit;
                } else {
                    bits[byte] &= !(1 << bit);
                }
            }
            Some(ImageData::Mono1 {
                width: *width,
                height: *height,
                bits,
            })
        }
        ImageData::Gray2 { width, height, data } => {
            let plane = ((*width as usize * *height as usize) + 7) / 8;
            if data.len() < plane * 3 {
                return None;
            }
            let mut bits = vec![0xFF; plane];
            for idx in 0..(*width as usize * *height as usize) {
                let byte = idx / 8;
                let bit = 7 - (idx % 8);
                let bw = (data[byte] >> bit) & 1;
                let l = (data[plane + byte] >> bit) & 1;
                let m = (data[plane * 2 + byte] >> bit) & 1;
                let lum = match (m, l, bw) {
                    (0, 0, 1) => 255,
                    (0, 1, 1) => 192,
                    (1, 0, 0) => 128,
                    (1, 1, 0) => 64,
                    _ => 0,
                };
                let lum = adjust_thumbnail_luma(lum);
                if lum >= 128 {
                    bits[byte] |= 1 << bit;
                } else {
                    bits[byte] &= !(1 << bit);
                }
            }
            Some(ImageData::Mono1 {
                width: *width,
                height: *height,
                bits,
            })
        }
        ImageData::Gray2Stream { .. } => None,
    }
}

fn adjust_thumbnail_luma(lum: u8) -> u8 {
    let mut value = ((lum as i32 - 128) * 13) / 10 + 128;
    if value < 0 {
        value = 0;
    } else if value > 255 {
        value = 255;
    }
    value as u8
}

fn wrap_home_title_lines(
    text: &str,
    max_width: i32,
    fonts: &[crate::palm::runtime::PalmFont],
    font_id: u8,
    scale_num: i32,
    scale_den: i32,
    max_lines: usize,
) -> Vec<String> {
    if max_lines == 0 || max_width <= 0 {
        return Vec::new();
    }
    let width_of = |s: &str| -> i32 {
        if fonts.is_empty() {
            (s.chars().count() as i32) * 10
        } else {
            palm_text_width_scaled(s, font_id, fonts, scale_num, scale_den)
        }
    };

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in words {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current, word)
        };
        if width_of(&candidate) <= max_width {
            current = candidate;
            continue;
        }

        if !current.is_empty() {
            lines.push(current);
            if lines.len() >= max_lines {
                return lines;
            }
            current = String::new();
        }

        if width_of(word) <= max_width {
            current = word.to_string();
            continue;
        }

        let mut clipped = String::new();
        for ch in word.chars() {
            let mut probe = clipped.clone();
            probe.push(ch);
            if width_of(&probe) > max_width {
                break;
            }
            clipped.push(ch);
        }
        if clipped.is_empty() {
            clipped.push('?');
        }
        lines.push(clipped);
        if lines.len() >= max_lines {
            return lines;
        }
    }

    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }
    lines
}

pub fn merge_bw_into_gray2(
    display_buffers: &mut DisplayBuffers,
    gray2_lsb: &mut [u8],
    gray2_msb: &mut [u8],
) {
    let size = display_buffers.size();
    let width = size.width as i32;
    let height = size.height as i32;
    for y in 0..height {
        for x in 0..width {
            if read_pixel(display_buffers, x, y) {
                continue;
            }
            let Some((fx, fy)) = map_display_point(display_buffers.rotation(), x, y) else {
                continue;
            };
            let idx = fy * FB_WIDTH + fx;
            let byte = idx / 8;
            let bit = 7 - (idx % 8);
            gray2_lsb[byte] |= 1 << bit;
            gray2_msb[byte] |= 1 << bit;
        }
    }
}

fn read_pixel(display_buffers: &DisplayBuffers, x: i32, y: i32) -> bool {
    let size = display_buffers.size();
    if x < 0 || y < 0 || x as u32 >= size.width || y as u32 >= size.height {
        return true;
    }
    let (x, y) = match display_buffers.rotation() {
        Rotation::Rotate0 => (x as usize, y as usize),
        Rotation::Rotate90 => (y as usize, FB_HEIGHT - 1 - x as usize),
        Rotation::Rotate180 => (FB_WIDTH - 1 - x as usize, FB_HEIGHT - 1 - y as usize),
        Rotation::Rotate270 => (FB_WIDTH - 1 - y as usize, x as usize),
    };
    if x >= FB_WIDTH || y >= FB_HEIGHT {
        return true;
    }
    let index = y * FB_WIDTH + x;
    let byte_index = index / 8;
    let bit_index = 7 - (index % 8);
    let buffer = display_buffers.get_active_buffer();
    (buffer[byte_index] >> bit_index) & 0x01 == 1
}
