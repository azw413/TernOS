use crate::platform::ButtonId;

use super::status_bar_view::StatusBarHit;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusBarButtons {
    pub home_enabled: bool,
    pub menu_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusBarNavResult {
    pub focus: Option<StatusBarHit>,
    pub activated: Option<StatusBarHit>,
    pub consumed: bool,
}

pub fn preferred_focus(buttons: StatusBarButtons) -> Option<StatusBarHit> {
    if buttons.menu_enabled {
        Some(StatusBarHit::Menu)
    } else if buttons.home_enabled {
        Some(StatusBarHit::Home)
    } else {
        None
    }
}

pub fn handle_button(
    focus: Option<StatusBarHit>,
    buttons: StatusBarButtons,
    button: ButtonId,
) -> StatusBarNavResult {
    let mut result = StatusBarNavResult {
        focus,
        activated: None,
        consumed: false,
    };
    match button {
        ButtonId::Left => {
            if focus == Some(StatusBarHit::Menu) && buttons.home_enabled {
                result.focus = Some(StatusBarHit::Home);
                result.consumed = true;
            }
        }
        ButtonId::Right => {
            if focus == Some(StatusBarHit::Home) && buttons.menu_enabled {
                result.focus = Some(StatusBarHit::Menu);
                result.consumed = true;
            }
        }
        ButtonId::Down | ButtonId::Back => {
            if focus.is_some() {
                result.focus = None;
                result.consumed = true;
            }
        }
        ButtonId::Confirm => {
            if let Some(current) = focus {
                let enabled = match current {
                    StatusBarHit::Home => buttons.home_enabled,
                    StatusBarHit::Menu => buttons.menu_enabled,
                };
                if enabled {
                    result.activated = Some(current);
                    result.consumed = true;
                }
            }
        }
        _ => {}
    }
    result
}
