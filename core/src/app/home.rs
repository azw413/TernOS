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

use crate::display::{Display, GrayscaleMode, RefreshMode};
use crate::framebuffer::{DisplayBuffers, Rotation, BUFFER_SIZE, HEIGHT as FB_HEIGHT, WIDTH as FB_WIDTH};
use crate::image_viewer::{AppSource, ImageData, ImageEntry, ImageError, InstalledAppEntry};
use crate::ui::{flush_queue, prc_alert, prc_components::{auto_button_layout_for_label, draw_form_title_bar, draw_palm_pull_down_box, draw_palm_text, draw_palm_text_scaled, palm_text_height, palm_text_height_scaled, palm_text_width, palm_text_width_scaled}, ListItem, ListView, Rect, RenderQueue, UiContext, View};

const START_MENU_MARGIN: i32 = 16;
const START_MENU_RECENT_THUMB: i32 = 74;
const START_MENU_STATUS_H: i32 = 34;
const START_MENU_FORM_Y: i32 = START_MENU_STATUS_H + 2;
const HEADER_Y: i32 = START_MENU_FORM_Y + 22;
const LIST_TOP: i32 = 72;
const LINE_HEIGHT: i32 = 30;
const LIST_MARGIN_X: i32 = 18;
const APP_GRID_COLS: usize = 3;

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
    pub entries: Vec<ImageEntry>,
    pub selected: usize,
    pub path: Vec<String>,
    pub start_menu_section: StartMenuSection,
    pub launcher_category: LauncherCategory,
    pub start_menu_index: usize,
    pub start_menu_prev_section: StartMenuSection,
    pub start_menu_prev_index: usize,
    pub category_menu_open: bool,
    pub category_menu_index: usize,
    pub start_menu_cache: Vec<RecentPreview>,
    pub installed_apps: Vec<InstalledAppEntry>,
    pub start_menu_nav_pending: bool,
    pub start_menu_need_base_refresh: bool,
    pub install_dialog: Option<InstallDialogState>,
}

#[derive(Debug)]
pub enum HomeOpenError {
    Empty,
}

#[derive(Debug)]
pub enum HomeOpen {
    EnterDir,
    OpenFile(ImageEntry),
}

pub enum HomeAction {
    None,
    OpenRecent(String),
}

pub enum MenuAction {
    None,
    OpenSelected,
    Back,
    Dirty,
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
    pub palm_fonts: &'a [crate::prc_app::runtime::PalmFont],
    pub icons: HomeIcons<'a>,
    pub draw_trbk_image: DrawTrbkImageFn,
}

impl HomeState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            path: Vec::new(),
            start_menu_section: StartMenuSection::Recents,
            launcher_category: LauncherCategory::Recents,
            start_menu_index: 0,
            start_menu_prev_section: StartMenuSection::Recents,
            start_menu_prev_index: 0,
            category_menu_open: false,
            category_menu_index: 0,
            start_menu_cache: Vec::new(),
            installed_apps: Vec::new(),
            start_menu_nav_pending: false,
            start_menu_need_base_refresh: true,
            install_dialog: None,
        }
    }

    pub fn show_install_summary_dialog(&mut self, summary: crate::palm_db::InstallSummary) {
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

    pub fn set_entries(&mut self, entries: Vec<ImageEntry>) {
        self.entries = entries;
        if self.selected >= self.entries.len() {
            self.selected = 0;
        }
    }

    pub fn refresh_entries<S: AppSource>(&mut self, source: &mut S) -> Result<(), ImageError> {
        let entries = source.refresh(&self.path)?;
        self.set_entries(entries);
        Ok(())
    }

    pub fn menu_title(&self) -> String {
        if self.path.is_empty() {
            "/".to_string()
        } else {
            let mut title = String::from("/");
            title.push_str(&self.path.join("/"));
            title
        }
    }

    pub fn entry_path_string(&self, entry: &ImageEntry) -> String {
        let mut parts = self.path.clone();
        parts.push(entry.name.clone());
        parts.join("/")
    }

    pub fn current_entry_name_owned(&self) -> Option<String> {
        let entry = self.entries.get(self.selected)?;
        if entry.kind != crate::image_viewer::EntryKind::File {
            return None;
        }
        Some(self.entry_path_string(entry))
    }

    pub fn open_selected(&mut self) -> Result<HomeOpen, HomeOpenError> {
        if self.entries.is_empty() {
            return Err(HomeOpenError::Empty);
        }
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Err(HomeOpenError::Empty);
        };
        match entry.kind {
            crate::image_viewer::EntryKind::Dir => {
                self.path.push(entry.name);
                Ok(HomeOpen::EnterDir)
            }
            crate::image_viewer::EntryKind::File => Ok(HomeOpen::OpenFile(entry)),
        }
    }

    pub fn open_index(&mut self, index: usize) -> Option<HomeOpen> {
        if self.entries.is_empty() {
            return None;
        }
        let index = index.min(self.entries.len().saturating_sub(1));
        let Some(entry) = self.entries.get(index).cloned() else {
            return None;
        };
        if entry.kind != crate::image_viewer::EntryKind::File {
            return None;
        }
        self.selected = index;
        Some(HomeOpen::OpenFile(entry))
    }

    pub fn open_recent_path<S: AppSource>(
        &mut self,
        source: &mut S,
        path: &str,
    ) -> Result<(), ImageError> {
        let mut parts: Vec<String> = path
            .split('/')
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .collect();
        if parts.is_empty() {
            return Ok(());
        }
        let file = parts.pop().unwrap_or_default();
        self.path = parts;
        self.refresh_entries(source)?;
        let idx = self.entries.iter().position(|entry| entry.name == file);
        if let Some(index) = idx {
            self.selected = index;
            Ok(())
        } else {
            Err(ImageError::Message("Recent entry not found.".into()))
        }
    }

    pub fn start_menu_cache_same(&self, recents: &[String]) -> bool {
        recents.len() == self.start_menu_cache.len()
            && recents
                .iter()
                .zip(self.start_menu_cache.iter())
                .all(|(path, cached)| path == &cached.path)
    }

    pub fn handle_start_menu_input(
        &mut self,
        recents: &[String],
        buttons: &crate::input::ButtonState,
    ) -> HomeAction {
        use crate::input::Buttons;
        let content_len = self.start_menu_content_len(recents);
        if content_len > 0 && self.start_menu_index >= content_len {
            self.start_menu_index = content_len.saturating_sub(1);
        }
        let categories = [
            LauncherCategory::Recents,
            LauncherCategory::Apps,
            LauncherCategory::Books,
            LauncherCategory::Images,
        ];

        if self.install_dialog.is_some() {
            if buttons.is_pressed(Buttons::Confirm) || buttons.is_pressed(Buttons::Back) {
                self.install_dialog = None;
                self.start_menu_need_base_refresh = true;
            }
            return HomeAction::None;
        }

        if self.category_menu_open {
            if buttons.is_pressed(Buttons::Up) {
                self.category_menu_index = self.category_menu_index.saturating_sub(1);
                self.start_menu_need_base_refresh = true;
                return HomeAction::None;
            }
            if buttons.is_pressed(Buttons::Down) {
                self.category_menu_index =
                    (self.category_menu_index + 1).min(categories.len().saturating_sub(1));
                self.start_menu_need_base_refresh = true;
                return HomeAction::None;
            }
            if buttons.is_pressed(Buttons::Confirm) {
                self.launcher_category = categories[self.category_menu_index];
                self.category_menu_open = false;
                self.start_menu_section = StartMenuSection::Recents;
                self.start_menu_index = 0;
                self.start_menu_need_base_refresh = true;
                return HomeAction::None;
            }
            if buttons.is_pressed(Buttons::Back) || buttons.is_pressed(Buttons::Left) {
                self.category_menu_open = false;
                self.start_menu_need_base_refresh = true;
                return HomeAction::None;
            }
            return HomeAction::None;
        }

        if buttons.is_pressed(Buttons::Up) {
            self.start_menu_prev_section = self.start_menu_section;
            self.start_menu_prev_index = self.start_menu_index;
            if self.start_menu_section == StartMenuSection::Recents {
                if self.launcher_category == LauncherCategory::Apps {
                    if content_len > 0 && self.start_menu_index >= APP_GRID_COLS {
                        self.start_menu_index -= APP_GRID_COLS;
                    } else {
                        self.start_menu_section = StartMenuSection::Actions;
                    }
                } else if content_len > 0 && self.start_menu_index > 0 {
                    self.start_menu_index -= 1;
                } else {
                    self.start_menu_section = StartMenuSection::Actions;
                }
            }
            self.start_menu_nav_pending = true;
            return HomeAction::None;
        }

        if buttons.is_pressed(Buttons::Down) {
            self.start_menu_prev_section = self.start_menu_section;
            self.start_menu_prev_index = self.start_menu_index;
            if self.start_menu_section == StartMenuSection::Actions {
                self.start_menu_section = StartMenuSection::Recents;
                self.start_menu_index = 0;
            } else if self.launcher_category == LauncherCategory::Apps {
                if content_len > 0 {
                    let next = self.start_menu_index + APP_GRID_COLS;
                    if next < content_len {
                        self.start_menu_index = next;
                    }
                }
            } else if content_len > 0 && self.start_menu_index + 1 < content_len {
                self.start_menu_index += 1;
            }
            self.start_menu_nav_pending = true;
            return HomeAction::None;
        }

        if buttons.is_pressed(Buttons::Left) {
            if self.start_menu_section == StartMenuSection::Recents {
                if self.launcher_category == LauncherCategory::Apps {
                    if content_len > 0 {
                        let row_start = (self.start_menu_index / APP_GRID_COLS) * APP_GRID_COLS;
                        if self.start_menu_index > row_start {
                            self.start_menu_prev_section = self.start_menu_section;
                            self.start_menu_prev_index = self.start_menu_index;
                            self.start_menu_index -= 1;
                            self.start_menu_nav_pending = true;
                        }
                    }
                } else {
                    self.start_menu_prev_section = self.start_menu_section;
                    self.start_menu_prev_index = self.start_menu_index;
                    self.start_menu_section = StartMenuSection::Actions;
                    self.start_menu_nav_pending = true;
                }
            }
            return HomeAction::None;
        }

        if buttons.is_pressed(Buttons::Right) {
            if self.start_menu_section == StartMenuSection::Actions {
                self.start_menu_prev_section = self.start_menu_section;
                self.start_menu_prev_index = self.start_menu_index;
                self.start_menu_section = StartMenuSection::Recents;
                self.start_menu_nav_pending = true;
            } else if self.start_menu_section == StartMenuSection::Recents
                && self.launcher_category == LauncherCategory::Apps
                && content_len > 0
            {
                let row_start = (self.start_menu_index / APP_GRID_COLS) * APP_GRID_COLS;
                let row_end = (row_start + APP_GRID_COLS).min(content_len).saturating_sub(1);
                if self.start_menu_index < row_end {
                    self.start_menu_prev_section = self.start_menu_section;
                    self.start_menu_prev_index = self.start_menu_index;
                    self.start_menu_index += 1;
                    self.start_menu_nav_pending = true;
                }
            }
            return HomeAction::None;
        }

        if buttons.is_pressed(Buttons::Confirm) {
            if self.start_menu_section == StartMenuSection::Actions {
                self.category_menu_open = true;
                self.category_menu_index = categories
                    .iter()
                    .position(|c| *c == self.launcher_category)
                    .unwrap_or(0);
                self.start_menu_need_base_refresh = true;
                return HomeAction::None;
            } else if self.launcher_category == LauncherCategory::Recents {
                if let Some(path) = recents.get(self.start_menu_index) {
                    return HomeAction::OpenRecent(path.clone());
                }
            } else if self.launcher_category == LauncherCategory::Apps {
                if let Some(app) = self.installed_apps.get(self.start_menu_index) {
                    return HomeAction::OpenRecent(app.path.clone());
                }
            }
        }

        HomeAction::None
    }

    fn start_menu_content_len(&self, recents: &[String]) -> usize {
        match self.launcher_category {
            LauncherCategory::Recents => recents.len(),
            LauncherCategory::Apps => self.installed_apps.len(),
            LauncherCategory::Books | LauncherCategory::Images => 0,
        }
    }

    pub fn handle_menu_input(
        &mut self,
        buttons: &crate::input::ButtonState,
    ) -> MenuAction {
        use crate::input::Buttons;

        if buttons.is_pressed(Buttons::Up) {
            if !self.entries.is_empty() {
                self.selected = self.selected.saturating_sub(1);
            }
            return MenuAction::Dirty;
        }
        if buttons.is_pressed(Buttons::Down) {
            if !self.entries.is_empty() {
                self.selected = (self.selected + 1).min(self.entries.len() - 1);
            }
            return MenuAction::Dirty;
        }
        if buttons.is_pressed(Buttons::Confirm) {
            return MenuAction::OpenSelected;
        }
        if buttons.is_pressed(Buttons::Back) {
            return MenuAction::Back;
        }

        MenuAction::None
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
        self.ensure_installed_apps_cache(ctx);

        let list_top = HEADER_Y + 28;
        let max_items = 6usize;
        let list_width = width - (START_MENU_MARGIN * 2);
        let item_height = 99;
        let thumb_size = 74;

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
                display.display_absolute_grayscale(GrayscaleMode::Fast);
                ctx.display_buffers.copy_active_to_inactive();
            } else {
                let mut rq = RenderQueue::default();
                rq.push(
                    Rect::new(0, 0, width, height),
                    if ctx.full_refresh {
                        RefreshMode::Full
                    } else {
                        RefreshMode::Fast
                    },
                );
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
            if self.launcher_category == LauncherCategory::Recents
                && self.start_menu_section == StartMenuSection::Recents
            {
                if self.start_menu_index < max_items {
                    let y = list_top + (self.start_menu_index as i32 * item_height);
                    if y + item_height <= mid_y {
                        let mut rq = RenderQueue::default();
                        rq.push(
                            Rect::new(START_MENU_MARGIN - 4, y - 4, list_width + 8, item_height - 4),
                            RefreshMode::Fast,
                        );
                        flush_queue(display, ctx.display_buffers, &mut rq, RefreshMode::Fast);
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
                if self.category_menu_open
                    || self.start_menu_section == StartMenuSection::Actions
                    || self.launcher_category != LauncherCategory::Recents
                {
                    rq.push(Rect::new(0, 0, width, height), RefreshMode::Fast);
                } else if self.launcher_category == LauncherCategory::Recents {
                    for idx in [self.start_menu_prev_index, self.start_menu_index] {
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
                                    RefreshMode::Fast,
                                );
                            }
                        }
                    }
                }
                flush_queue(display, ctx.display_buffers, &mut rq, RefreshMode::Fast);
                self.start_menu_nav_pending = false;
            } else {
                let mut rq = RenderQueue::default();
                rq.push(Rect::new(0, 0, width, height), RefreshMode::Fast);
                flush_queue(display, ctx.display_buffers, &mut rq, RefreshMode::Fast);
            }
        } else {
            let mut rq = RenderQueue::default();
            rq.push(
                Rect::new(0, 0, width, height),
                if ctx.full_refresh {
                    RefreshMode::Full
                } else {
                    RefreshMode::Fast
                },
            );
            flush_queue(display, ctx.display_buffers, &mut rq, RefreshMode::Full);
        }
    }

    pub fn draw_menu<S: AppSource>(
        &mut self,
        ctx: &mut HomeRenderContext<'_, S>,
        display: &mut impl Display,
    ) {
        let mut labels: Vec<String> = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            if entry.kind == crate::image_viewer::EntryKind::Dir {
                let mut label = entry.name.clone();
                label.push('/');
                labels.push(label);
            } else {
                labels.push(entry.name.clone());
            }
        }
        let items: Vec<ListItem<'_>> = labels
            .iter()
            .map(|label| ListItem { label: label.as_str() })
            .collect();

        let title = self.menu_title();
        let mut list = ListView::new(&items);
        list.title = Some(title.as_str());
        list.footer = Some("Up/Down: select  Confirm: open  Back: up");
        list.empty_label = Some("No files found.");
        list.selected = self.selected;
        list.margin_x = LIST_MARGIN_X;
        list.header_y = HEADER_Y;
        list.list_top = LIST_TOP;
        list.line_height = LINE_HEIGHT;

        let size = ctx.display_buffers.size();
        let rect = Rect::new(0, 0, size.width as i32, size.height as i32);
        let mut rq = RenderQueue::default();
        let mut ui = UiContext {
            buffers: ctx.display_buffers,
        };
        list.render(&mut ui, rect, &mut rq);

        let fallback = if ctx.full_refresh {
            RefreshMode::Full
        } else {
            RefreshMode::Fast
        };
        flush_queue(display, ctx.display_buffers, &mut rq, fallback);
    }

    fn render_start_menu_contents<S: AppSource>(
        &mut self,
        ctx: &mut HomeRenderContext<'_, S>,
        suppress_selection: bool,
        width: i32,
        mid_y: i32,
        list_top: i32,
        max_items: usize,
        list_width: i32,
        item_height: i32,
        thumb_size: i32,
    ) -> (bool, usize) {
        let chrome = PalmChromeMetrics::palm3();
        let header_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        ctx.display_buffers.clear(BinaryColor::On).ok();
        ctx.gray2_lsb.fill(0);
        ctx.gray2_msb.fill(0);
        let mut gray2_used = false;

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
        if self.start_menu_section == StartMenuSection::Actions && !self.category_menu_open {
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
        let arrow_color = if self.start_menu_section == StartMenuSection::Actions && !self.category_menu_open {
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
            draw_palm_pull_down_box(ctx.display_buffers, menu_x, menu_y, menu_w, menu_h);
            for (i, label) in menu_items.iter().enumerate() {
                let y = menu_y + 3 + (i as i32 * item_h);
                let selected = i == self.category_menu_index;
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

        let mut draw_count = 0usize;
        if self.launcher_category == LauncherCategory::Recents {
            for (idx, preview) in self.start_menu_cache.iter().take(max_items).enumerate() {
                let y = list_top + (idx as i32 * item_height);
                if y + item_height > mid_y {
                    break;
                }
                let is_selected = !suppress_selection
                    && self.start_menu_section == StartMenuSection::Recents
                    && self.start_menu_index == idx;
                if is_selected {
                    Rectangle::new(
                        Point::new(START_MENU_MARGIN - 4, y - 4),
                        Size::new((list_width + 8) as u32, (item_height - 4) as u32),
                    )
                    .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(
                        BinaryColor::Off,
                    ))
                    .draw(ctx.display_buffers)
                    .ok();
                }
                let thumb_x = START_MENU_MARGIN;
                let thumb_y = y + (item_height - thumb_size) / 2 - 2;
                Rectangle::new(
                    Point::new(thumb_x, thumb_y),
                    Size::new(thumb_size as u32, thumb_size as u32),
                )
                .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_stroke(
                    if is_selected {
                        BinaryColor::On
                    } else {
                        BinaryColor::Off
                    },
                    1,
                ))
                .draw(ctx.display_buffers)
                .ok();
                if let Some(image) = preview.image.as_ref() {
                    if let Some(mono) = thumbnail_to_mono(image) {
                        let mut gray2_ctx = None;
                        (ctx.draw_trbk_image)(
                            ctx.display_buffers,
                            &mono,
                            &mut gray2_ctx,
                            thumb_x + 2,
                            thumb_y + 2,
                            thumb_size - 4,
                            thumb_size - 4,
                        );
                    } else {
                        let gray2_lsb = &mut *ctx.gray2_lsb;
                        let gray2_msb = &mut *ctx.gray2_msb;
                        let mut gray2_ctx = Some((gray2_lsb, gray2_msb, &mut gray2_used));
                        (ctx.draw_trbk_image)(
                            ctx.display_buffers,
                            image,
                            &mut gray2_ctx,
                            thumb_x + 2,
                            thumb_y + 2,
                            thumb_size - 4,
                            thumb_size - 4,
                        );
                    }
                }
                let text_color = if is_selected {
                    BinaryColor::On
                } else {
                    BinaryColor::Off
                };
                let title_x = thumb_x + thumb_size + 12;
                let title_max_w = (list_width - (title_x - START_MENU_MARGIN) - 6).max(20);
                let lines = wrap_home_title_lines(
                    &preview.title,
                    title_max_w,
                    ctx.palm_fonts,
                    0,
                    6,
                    5,
                    2,
                );
                if !ctx.palm_fonts.is_empty() {
                    let line_h = palm_text_height_scaled(0, ctx.palm_fonts, 6, 5).max(8) + 2;
                    for (line_idx, line) in lines.iter().enumerate() {
                        draw_palm_text_scaled(
                            ctx.display_buffers,
                            line.as_str(),
                            title_x,
                            y + 10 + (line_idx as i32 * line_h),
                            0,
                            ctx.palm_fonts,
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
                            Point::new(title_x, y + 26 + (line_idx as i32 * 18)),
                            label_style,
                        )
                        .draw(ctx.display_buffers)
                        .ok();
                    }
                }
                draw_count += 1;
            }
            if draw_count == 0 {
                Text::new(
                    "No recent items.",
                    Point::new(START_MENU_MARGIN, list_top + 24),
                    header_style,
                )
                .draw(ctx.display_buffers)
                .ok();
            }
        } else if self.launcher_category == LauncherCategory::Apps {
            let cols = APP_GRID_COLS;
            let cell_w = (list_width / APP_GRID_COLS as i32).max(1);
            let cell_h = 120i32;
            let icon_size = 60i32;
            let row_count = ((mid_y - list_top) / cell_h).max(0) as usize;
            let visible = row_count * cols;
            let start = if visible > 0 && self.start_menu_index >= visible {
                (self.start_menu_index / cols + 1 - row_count) * cols
            } else {
                0
            };
            let end = (start + visible).min(self.installed_apps.len());
            for (idx, app) in self.installed_apps.iter().enumerate().skip(start).take(end.saturating_sub(start)) {
                let local = idx - start;
                let col = (local % cols) as i32;
                let row = (local / cols) as i32;
                let cell_x = START_MENU_MARGIN + col * cell_w;
                let cell_y = list_top + row * cell_h;
                let selected = self.start_menu_section == StartMenuSection::Recents
                    && !suppress_selection
                    && idx == self.start_menu_index;
                let icon_x = cell_x + ((cell_w - icon_size) / 2);
                let icon_y = cell_y + 2;
                if let Some(image) = app.icon.as_ref() {
                    let mut gray2_ctx = None;
                    (ctx.draw_trbk_image)(
                        ctx.display_buffers,
                        image,
                        &mut gray2_ctx,
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
                    .draw(ctx.display_buffers)
                    .ok();
                }
                let title_max_w = (cell_w - 6).max(20);
                let title_lines = wrap_home_title_lines(
                    app.title.as_str(),
                    title_max_w,
                    ctx.palm_fonts,
                    0,
                    6,
                    5,
                    2,
                );
                let title_color = if selected { BinaryColor::On } else { BinaryColor::Off };
                let title_bg = if selected { BinaryColor::Off } else { BinaryColor::On };
                let line_h = palm_text_height_scaled(0, ctx.palm_fonts, 6, 5).max(10);
                let title_block_h = (title_lines.len() as i32 * line_h).max(line_h);
                let title_top = icon_y + icon_size + 6;
                if selected {
                    Rectangle::new(
                        Point::new(cell_x + 2, title_top - 2),
                        Size::new((cell_w - 4).max(1) as u32, (title_block_h + 4).max(1) as u32),
                    )
                    .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(title_bg))
                    .draw(ctx.display_buffers)
                    .ok();
                }
                if !ctx.palm_fonts.is_empty() {
                    for (line_idx, line) in title_lines.iter().enumerate() {
                        let tw = palm_text_width_scaled(line, 0, ctx.palm_fonts, 6, 5);
                        let tx = cell_x + ((cell_w - tw) / 2).max(0);
                        let ty = title_top + (line_idx as i32 * line_h);
                        draw_palm_text_scaled(
                            ctx.display_buffers,
                            line.as_str(),
                            tx,
                            ty,
                            0,
                            ctx.palm_fonts,
                            6,
                            5,
                            title_color,
                        );
                    }
                } else {
                    Text::new(
                        app.title.as_str(),
                        Point::new(cell_x + 4, title_top + 12),
                        MonoTextStyle::new(
                            &FONT_10X20,
                            title_color,
                        ),
                    )
                    .draw(ctx.display_buffers)
                    .ok();
                }
                draw_count += 1;
            }
            if draw_count == 0 {
                Text::new(
                    "No installed apps yet.",
                    Point::new(START_MENU_MARGIN, list_top + 24),
                    header_style,
                )
                .draw(ctx.display_buffers)
                .ok();
            }
        } else {
            let msg = match self.launcher_category {
                LauncherCategory::Apps => "No installed apps yet.",
                LauncherCategory::Books => "No books in launcher yet.",
                LauncherCategory::Images => "No images in launcher yet.",
                LauncherCategory::Recents => "No recent items.",
            };
            Text::new(msg, Point::new(START_MENU_MARGIN, list_top + 24), header_style)
                .draw(ctx.display_buffers)
                .ok();
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
        fonts: &[crate::prc_app::runtime::PalmFont],
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
    fonts: &[crate::prc_app::runtime::PalmFont],
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
