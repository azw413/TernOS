extern crate alloc;

use alloc::vec::Vec;

use crate::{
    application::Application,
    display::Display,
    image_viewer::AppSource,
    input::ButtonState,
    platform::PlatformInputEvent,
};

#[derive(Clone, Default)]
pub struct RuntimeFrame {
    pub buttons: ButtonState,
    pub events: Vec<PlatformInputEvent>,
    pub elapsed_ms: u32,
}

impl RuntimeFrame {
    pub fn new(buttons: ButtonState, events: Vec<PlatformInputEvent>, elapsed_ms: u32) -> Self {
        Self {
            buttons,
            events,
            elapsed_ms,
        }
    }
}

pub fn update_application_frame<S: AppSource>(
    application: &mut Application<'_, S>,
    frame: &RuntimeFrame,
) {
    application.update_with_events(&frame.buttons, &frame.events, frame.elapsed_ms);
}

pub fn draw_application_frame<S: AppSource, D: Display>(
    application: &mut Application<'_, S>,
    display: &mut D,
) {
    application.draw(display);
}
