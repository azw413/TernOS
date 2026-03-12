use std::vec::Vec;

use tern_core::{
    input::{ButtonState, Buttons},
    platform::{ButtonId, Platform, PlatformInputEvent},
    runtime_host::RuntimeFrame,
};

use crate::platform::M5PaperIdfPlatform;

pub struct M5PaperRuntimeHost {
    platform: M5PaperIdfPlatform,
    current_buttons: u8,
}

impl M5PaperRuntimeHost {
    pub fn new(platform: M5PaperIdfPlatform) -> Self {
        Self {
            platform,
            current_buttons: 0,
        }
    }

    pub fn platform(&mut self) -> &mut M5PaperIdfPlatform {
        &mut self.platform
    }

    pub fn next_frame(&mut self, elapsed_ms: u32) -> RuntimeFrame {
        let mut events = Vec::new();
        self.platform.poll_input(&mut |event| {
            match event {
                PlatformInputEvent::ButtonDown(button) => {
                    self.current_buttons |= button_mask(button);
                }
                PlatformInputEvent::ButtonUp(button) => {
                    self.current_buttons &= !button_mask(button);
                }
                _ => {}
            }
            events.push(event);
        });

        let mut buttons = ButtonState::default();
        buttons.update(self.current_buttons);
        RuntimeFrame::new(buttons, events, elapsed_ms)
    }
}

fn button_mask(button: ButtonId) -> u8 {
    match button {
        ButtonId::Up => 1 << (Buttons::Up as u8),
        ButtonId::Down => 1 << (Buttons::Down as u8),
        ButtonId::Power => 1 << (Buttons::Power as u8),
        ButtonId::Confirm => 1 << (Buttons::Confirm as u8),
        ButtonId::Back => 1 << (Buttons::Back as u8),
        ButtonId::Left => 1 << (Buttons::Left as u8),
        ButtonId::Right => 1 << (Buttons::Right as u8),
        ButtonId::Menu => 0,
    }
}
