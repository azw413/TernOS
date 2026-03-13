use crate::platform::{ButtonId, PlatformInputEvent};

pub type FormId = u16;
pub type ObjectId = u16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiEvent {
    Nil,
    FormLoad { form_id: FormId },
    FormOpen { form_id: FormId },
    FormClose { form_id: FormId },
    ControlSelect { control_id: ObjectId },
    FieldEnter { field_id: ObjectId },
    FieldChanged { field_id: ObjectId },
    KeyDown { chr: u16, key_code: u16, modifiers: u16 },
    KeyUp { key_code: u16 },
    PenDown { x: i16, y: i16 },
    PenUp { x: i16, y: i16 },
    PenMove { x: i16, y: i16 },
    ButtonDown { button: ButtonId },
    ButtonUp { button: ButtonId },
    TableSelect { table_id: ObjectId, row: u16, col: u16 },
    MenuCommand { item_id: u16 },
    Tick,
    AppStop,
}

impl UiEvent {
    pub fn from_platform_event(event: PlatformInputEvent) -> Self {
        match event {
            PlatformInputEvent::ButtonDown(button) => Self::ButtonDown { button },
            PlatformInputEvent::ButtonUp(button) => Self::ButtonUp { button },
            PlatformInputEvent::TouchDown { x, y } => Self::PenDown {
                x: clamp_i16(x),
                y: clamp_i16(y),
            },
            PlatformInputEvent::TouchMove { x, y } => Self::PenMove {
                x: clamp_i16(x),
                y: clamp_i16(y),
            },
            PlatformInputEvent::TouchUp { x, y } => Self::PenUp {
                x: clamp_i16(x),
                y: clamp_i16(y),
            },
            PlatformInputEvent::KeyDown {
                chr,
                key_code,
                modifiers,
            } => Self::KeyDown {
                chr,
                key_code,
                modifiers,
            },
            PlatformInputEvent::KeyUp { key_code } => Self::KeyUp { key_code },
            PlatformInputEvent::Tick => Self::Tick,
        }
    }
}

const fn clamp_i16(value: i32) -> i16 {
    if value < i16::MIN as i32 {
        i16::MIN
    } else if value > i16::MAX as i32 {
        i16::MAX
    } else {
        value as i16
    }
}
