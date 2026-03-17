extern crate alloc;

use alloc::{format, vec};

use crate::{
    palm::menu_preview::{MenuBarPreview, MenuItemPreview, MenuPullDownPreview},
    ternos::ui::{ModalFormSpec, ModalWidget, ObjectId, Rect},
};
const HOME_MENU_HELP: u16 = 1;
const HOME_CMD_ABOUT: u16 = 1001;
const ABOUT_FORM_ID: u16 = 0x4142;
const ABOUT_VERSION_ID: ObjectId = 1;
const ABOUT_BUILD_ID: ObjectId = 2;
const ABOUT_OK_ID: ObjectId = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeMenuCommand {
    About,
}

pub fn home_menu_bar() -> MenuBarPreview {
    MenuBarPreview {
        resource_id: 1,
        menus: vec![MenuPullDownPreview {
            resource_id: HOME_MENU_HELP,
            title: "Help".into(),
            items: vec![MenuItemPreview {
                id: HOME_CMD_ABOUT,
                text: "About".into(),
                shortcut: None,
            }],
        }],
    }
}

pub fn home_menu_command(item_id: u16) -> Option<HomeMenuCommand> {
    match item_id {
        HOME_CMD_ABOUT => Some(HomeMenuCommand::About),
        _ => None,
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
