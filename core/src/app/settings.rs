extern crate alloc;

use alloc::{format, string::String, vec};

use crate::{
    image_viewer::InstalledDatabaseEntry,
    palm::menu_preview::{MenuBarPreview, MenuItemPreview, MenuPullDownPreview},
    ternos::ui::{
        ModalChrome, ModalFormSpec, ModalTableCellStyle, ModalWidget, ObjectId, Rect,
        StatusBarView, UiTableCell, UiTableColumn, UiTableModel, UiTableRow,
    },
};
const HOME_MENU_SETTINGS: u16 = 1;
const HOME_MENU_HELP: u16 = 2;
const HOME_CMD_RECORDS: u16 = 1000;
const HOME_CMD_ABOUT: u16 = 1001;
const ABOUT_FORM_ID: u16 = 0x4142;
const RECORDS_FORM_ID: u16 = 0x4152;
const RECORD_DETAIL_FORM_ID: u16 = 0x4153;
const ABOUT_VERSION_ID: ObjectId = 1;
const ABOUT_BUILD_ID: ObjectId = 2;
const ABOUT_OK_ID: ObjectId = 3;
const RECORDS_TABLE_ID: ObjectId = 11;
const RECORDS_SCROLL_ID: ObjectId = 12;
const RECORD_DETAIL_TYPE_ID: ObjectId = 21;
const RECORD_DETAIL_TITLE_ID: ObjectId = 22;
const RECORD_DETAIL_CREATOR_ID: ObjectId = 23;
const RECORD_DETAIL_KIND_ID: ObjectId = 24;
const RECORD_DETAIL_SIZE_ID: ObjectId = 25;
const RECORD_DETAIL_OK_ID: ObjectId = 26;
const RECORD_DETAIL_DELETE_ID: ObjectId = 27;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeMenuCommand {
    Records,
    About,
}

pub fn home_menu_bar() -> MenuBarPreview {
    MenuBarPreview {
        resource_id: 1,
        menus: vec![
            MenuPullDownPreview {
                resource_id: HOME_MENU_SETTINGS,
                title: "Settings".into(),
                items: vec![MenuItemPreview {
                    id: HOME_CMD_RECORDS,
                    text: "Records".into(),
                    shortcut: None,
                }],
            },
            MenuPullDownPreview {
                resource_id: HOME_MENU_HELP,
                title: "Help".into(),
                items: vec![MenuItemPreview {
                    id: HOME_CMD_ABOUT,
                    text: "About".into(),
                    shortcut: None,
                }],
            },
        ],
    }
}

pub fn home_menu_command(item_id: u16) -> Option<HomeMenuCommand> {
    match item_id {
        HOME_CMD_RECORDS => Some(HomeMenuCommand::Records),
        HOME_CMD_ABOUT => Some(HomeMenuCommand::About),
        _ => None,
    }
}

fn format_size(size_bytes: u64) -> String {
    if size_bytes >= 1024 * 1024 {
        format!("{:.1} MB", size_bytes as f32 / (1024.0 * 1024.0))
    } else if size_bytes >= 1024 {
        format!("{:.1} KB", size_bytes as f32 / 1024.0)
    } else {
        format!("{size_bytes} B")
    }
}

pub fn about_modal_spec(version: &str, build_time: &str, screen_width: i32) -> ModalFormSpec {
    let form_x = 18;
    let form_w = (screen_width - 36).max(1);
    let form_y = 92;
    let form_h = 188;
    let button_w = 96;
    let button_h = 34;
    let button_x = form_x + form_w - button_w - 16;
    let button_y = form_y + form_h - button_h - 14;

    ModalFormSpec {
        form_id: ABOUT_FORM_ID,
        bounds: Rect::new(form_x, form_y, form_w, form_h),
        chrome: ModalChrome::Alert,
        title: "About".into(),
        widgets: vec![
            ModalWidget::Label {
                id: ABOUT_VERSION_ID,
                bounds: Rect::new(form_x + 18, form_y + 56, form_w - 36, 22),
                text: format!("Version: {version}"),
                font_id: 0,
            },
            ModalWidget::Label {
                id: ABOUT_BUILD_ID,
                bounds: Rect::new(form_x + 18, form_y + 86, form_w - 36, 22),
                text: format!("Build time: {build_time}"),
                font_id: 0,
            },
            ModalWidget::Button {
                id: ABOUT_OK_ID,
                bounds: Rect::new(button_x, button_y, button_w, button_h),
                text: "OK".into(),
                font_id: 0,
                style: 1,
                no_frame: false,
            },
        ],
        default_focus: Some(ABOUT_OK_ID),
    }
}

pub fn about_ok_id() -> ObjectId {
    ABOUT_OK_ID
}

pub fn records_modal_spec(
    entries: &[InstalledDatabaseEntry],
    selected_row: usize,
    top_row: usize,
    screen_width: i32,
    screen_height: i32,
) -> ModalFormSpec {
    let form_x = 2;
    let form_w = (screen_width - 4).max(1);
    let form_y = StatusBarView::HEIGHT + 5;
    let form_h = (screen_height - form_y).max(220);
    let table_x = form_x + 16;
    let table_y = form_y + 38;
    let table_w = form_w - 38;
    let table_h = (form_h - 56).max(72);
    let type_w = 96;
    let size_w = 82;
    let title_w = (table_w - type_w - size_w).max(140);
    let model = UiTableModel {
        cols: 3,
        top_row: top_row as u16,
        selected_row: (!entries.is_empty()).then_some(selected_row.min(entries.len().saturating_sub(1)) as u16),
        selected_col: None,
        columns: vec![
            UiTableColumn {
                width: type_w as i16,
                spacing: 10,
                usable: true,
            },
            UiTableColumn {
                width: title_w as i16,
                spacing: 10,
                usable: true,
            },
            UiTableColumn {
                width: size_w as i16,
                spacing: 0,
                usable: true,
            },
        ],
        rows: entries
            .iter()
            .enumerate()
            .map(|(index, entry)| UiTableRow {
                id: index as u16,
                height: 28,
                usable: true,
                selectable: true,
                data: index as u32,
                cells: vec![
                    UiTableCell {
                        text: entry.type_code.clone(),
                    },
                    UiTableCell {
                        text: entry.title.clone(),
                    },
                    UiTableCell {
                        text: format_size(entry.size_bytes),
                    },
                ],
            })
            .collect(),
    };

    ModalFormSpec {
        form_id: RECORDS_FORM_ID,
        bounds: Rect::new(form_x, form_y, form_w, form_h),
        chrome: ModalChrome::Form,
        title: "Records".into(),
        widgets: vec![
            ModalWidget::Table {
                id: RECORDS_TABLE_ID,
                bounds: Rect::new(table_x, table_y, table_w, table_h),
                model,
                cell_style: ModalTableCellStyle::PalmText {
                    font_id: 0,
                    padding_x: 6,
                    padding_y: 4,
                },
            },
            ModalWidget::ScrollBar {
                id: RECORDS_SCROLL_ID,
                bounds: Rect::new(table_x + table_w + 4, table_y, 11, table_h),
                table_id: RECORDS_TABLE_ID,
            },
        ],
        default_focus: (!entries.is_empty()).then_some(RECORDS_TABLE_ID),
    }
}

pub fn record_detail_modal_spec(
    entry: &InstalledDatabaseEntry,
    screen_width: i32,
    screen_height: i32,
) -> ModalFormSpec {
    let form_w = (screen_width - 36).max(1);
    let form_h = 224;
    let form_x = 18;
    let form_y = ((screen_height - form_h) / 2).max(70);
    let button_w = 94;
    let button_h = 34;
    let ok_x = form_x + form_w - button_w - 16;
    let delete_x = ok_x - button_w - 12;
    let button_y = form_y + form_h - button_h - 14;
    let mut widgets = vec![
        ModalWidget::Label {
            id: RECORD_DETAIL_TYPE_ID,
            bounds: Rect::new(form_x + 18, form_y + 52, form_w - 36, 22),
            text: format!("Type: {}", entry.type_code),
            font_id: 0,
        },
        ModalWidget::Label {
            id: RECORD_DETAIL_TITLE_ID,
            bounds: Rect::new(form_x + 18, form_y + 78, form_w - 36, 36),
            text: format!("Title: {}", entry.title),
            font_id: 0,
        },
        ModalWidget::Label {
            id: RECORD_DETAIL_CREATOR_ID,
            bounds: Rect::new(form_x + 18, form_y + 118, form_w - 36, 22),
            text: format!("Creator: {}", entry.creator_code),
            font_id: 0,
        },
        ModalWidget::Label {
            id: RECORD_DETAIL_KIND_ID,
            bounds: Rect::new(form_x + 18, form_y + 144, form_w - 36, 22),
            text: format!(
                "Kind: {}",
                match entry.kind {
                    crate::ternos::services::db::DbKind::Resource => "Resource",
                    crate::ternos::services::db::DbKind::Record => "Record",
                }
            ),
            font_id: 0,
        },
        ModalWidget::Label {
            id: RECORD_DETAIL_SIZE_ID,
            bounds: Rect::new(form_x + 18, form_y + 170, form_w - 36, 22),
            text: format!("Size: {}", format_size(entry.size_bytes)),
            font_id: 0,
        },
        ModalWidget::Button {
            id: RECORD_DETAIL_OK_ID,
            bounds: Rect::new(ok_x, button_y, button_w, button_h),
            text: "OK".into(),
            font_id: 0,
            style: 1,
            no_frame: false,
        },
    ];
    if entry.can_delete {
        widgets.push(ModalWidget::Button {
            id: RECORD_DETAIL_DELETE_ID,
            bounds: Rect::new(delete_x, button_y, button_w, button_h),
            text: "Delete".into(),
            font_id: 0,
            style: 1,
            no_frame: false,
        });
    }
    ModalFormSpec {
        form_id: RECORD_DETAIL_FORM_ID,
        bounds: Rect::new(form_x, form_y, form_w, form_h),
        chrome: ModalChrome::Alert,
        title: "Record".into(),
        widgets,
        default_focus: Some(RECORD_DETAIL_OK_ID),
    }
}

pub fn records_table_id() -> ObjectId {
    RECORDS_TABLE_ID
}

pub fn record_detail_ok_id() -> ObjectId {
    RECORD_DETAIL_OK_ID
}

pub fn record_detail_delete_id() -> ObjectId {
    RECORD_DETAIL_DELETE_ID
}
