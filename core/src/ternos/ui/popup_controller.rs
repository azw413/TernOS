use crate::platform::ButtonId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopupMenuAction {
    None,
    Redraw { selected_index: usize },
    Activate { selected_index: usize },
    Close,
}

pub fn handle_button(
    selected_index: usize,
    item_count: usize,
    button: ButtonId,
) -> PopupMenuAction {
    match button {
        ButtonId::Up => PopupMenuAction::Redraw {
            selected_index: selected_index.saturating_sub(1),
        },
        ButtonId::Down => PopupMenuAction::Redraw {
            selected_index: (selected_index + 1).min(item_count.saturating_sub(1)),
        },
        ButtonId::Confirm => PopupMenuAction::Activate {
            selected_index: selected_index.min(item_count.saturating_sub(1)),
        },
        ButtonId::Back | ButtonId::Left => PopupMenuAction::Close,
        _ => PopupMenuAction::None,
    }
}
