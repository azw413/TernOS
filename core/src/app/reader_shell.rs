use crate::{
    input,
    palm::{runtime::PalmFont, ui, ui_component::UiNavEvent},
    ternos::ui::{preferred_status_bar_focus, Point, Rect, StatusBarButtons, StatusBarHit, StatusBarView},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReaderStatusBarFocus {
    Home,
    Menu,
}

pub fn nav_event_from_buttons(buttons: &input::ButtonState) -> Option<UiNavEvent> {
    if buttons.is_pressed(input::Buttons::Back) {
        Some(UiNavEvent::Back)
    } else if buttons.is_pressed(input::Buttons::Left) {
        Some(UiNavEvent::Left)
    } else if buttons.is_pressed(input::Buttons::Right) {
        Some(UiNavEvent::Right)
    } else if buttons.is_pressed(input::Buttons::Up) {
        Some(UiNavEvent::Up)
    } else if buttons.is_pressed(input::Buttons::Down) {
        Some(UiNavEvent::Down)
    } else if buttons.is_pressed(input::Buttons::Confirm) {
        Some(UiNavEvent::Confirm)
    } else {
        None
    }
}

pub fn to_status_hit(focus: Option<ReaderStatusBarFocus>) -> Option<StatusBarHit> {
    match focus {
        Some(ReaderStatusBarFocus::Home) => Some(StatusBarHit::Home),
        Some(ReaderStatusBarFocus::Menu) => Some(StatusBarHit::Menu),
        None => None,
    }
}

pub fn from_status_hit(hit: Option<StatusBarHit>) -> Option<ReaderStatusBarFocus> {
    match hit {
        Some(StatusBarHit::Home) => Some(ReaderStatusBarFocus::Home),
        Some(StatusBarHit::Menu) => Some(ReaderStatusBarFocus::Menu),
        None => None,
    }
}

pub fn preferred_focus() -> Option<ReaderStatusBarFocus> {
    from_status_hit(preferred_status_bar_focus(StatusBarButtons {
        home_enabled: true,
        menu_enabled: true,
    }))
}

pub fn status_bar_hit(screen_width: i32, point: Point) -> Option<StatusBarHit> {
    StatusBarView::hit_test(
        Rect::new(0, 0, screen_width, StatusBarView::HEIGHT),
        point,
    )
}

pub fn help_dialog_rect() -> Rect {
    Rect::new(18, StatusBarView::HEIGHT + 8, 448, 236)
}

pub fn help_overlay_hit(
    dialog: &crate::palm::runner::RuntimeHelpDialog,
    fonts: &[PalmFont],
    point: Point,
) -> Option<ui::HelpOverlayHit> {
    ui::hit_test_help_overlay_native(dialog, fonts, help_dialog_rect(), point)
}

pub fn menu_overlay_hit(
    menu: &crate::palm::menu_preview::MenuBarPreview,
    active_menu_index: usize,
    fonts: &[PalmFont],
    screen_width: i32,
    point: Point,
) -> Option<ui::MenuOverlayHit> {
    ui::hit_test_menu_overlay_native(
        menu,
        active_menu_index,
        fonts,
        0,
        StatusBarView::HEIGHT + 5,
        screen_width,
        point,
    )
}
