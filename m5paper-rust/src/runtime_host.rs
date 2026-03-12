extern crate alloc;

use alloc::vec::Vec;

use tern_core::{
    input::{ButtonState, Buttons},
    platform::{ButtonId, PlatformInputEvent},
    runtime_host::RuntimeFrame,
};

use crate::ffi;

pub struct M5PaperRuntimeHost {
    current_buttons: u8,
}

impl M5PaperRuntimeHost {
    pub fn new() -> Self {
        Self { current_buttons: 0 }
    }

    pub fn next_frame(&mut self, elapsed_ms: u32) -> RuntimeFrame {
        let mut events = Vec::new();
        while let Ok(Some(event)) = ffi::input_next() {
            match event.event_type {
                ffi::INPUT_BUTTON_DOWN => {
                    let mapped = map_button(event.button_id);
                    self.current_buttons |= 1 << (mapped as u8);
                    events.push(PlatformInputEvent::ButtonDown(map_platform_button(mapped)));
                }
                ffi::INPUT_BUTTON_UP => {
                    let mapped = map_button(event.button_id);
                    self.current_buttons &= !(1 << (mapped as u8));
                    events.push(PlatformInputEvent::ButtonUp(map_platform_button(mapped)));
                }
                ffi::INPUT_TOUCH_DOWN => events.push(PlatformInputEvent::TouchDown {
                    x: i32::from(event.x),
                    y: i32::from(event.y),
                }),
                ffi::INPUT_TOUCH_MOVE => events.push(PlatformInputEvent::TouchMove {
                    x: i32::from(event.x),
                    y: i32::from(event.y),
                }),
                ffi::INPUT_TOUCH_UP => events.push(PlatformInputEvent::TouchUp {
                    x: i32::from(event.x),
                    y: i32::from(event.y),
                }),
                _ => {}
            }
        }

        let mut buttons = ButtonState::default();
        buttons.update(self.current_buttons);
        RuntimeFrame::new(buttons, events, elapsed_ms)
    }
}

fn map_button(button_id: u8) -> Buttons {
    match button_id {
        1 => Buttons::Up,
        2 => Buttons::Down,
        3 => Buttons::Confirm,
        _ => Buttons::Back,
    }
}

fn map_platform_button(button: Buttons) -> ButtonId {
    match button {
        Buttons::Up => ButtonId::Up,
        Buttons::Down => ButtonId::Down,
        Buttons::Confirm => ButtonId::Confirm,
        Buttons::Power => ButtonId::Power,
        Buttons::Back => ButtonId::Back,
        Buttons::Left => ButtonId::Left,
        Buttons::Right => ButtonId::Right,
    }
}
