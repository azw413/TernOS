use tern_core::{
    application::Application,
    display::{HEIGHT, WIDTH},
    framebuffer::DisplayBuffers,
    platform::DisplayDevice,
    runtime_host::{draw_application_frame, update_application_frame, RuntimeFrame},
};

use crate::display::MinifbDisplay;
use crate::image_source::DesktopImageSource;

mod display;
mod image_source;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("TernReader desktop application started");

    let options = minifb::WindowOptions {
        borderless: false,
        title: true,
        resize: true,
        scale: minifb::Scale::X2,
        ..minifb::WindowOptions::default()
    };
    let mut window = minifb::Window::new(
        "TernReader Desktop",
        HEIGHT,
        WIDTH,
        options,
    )
    .unwrap_or_else(|e| {
        panic!("Unable to open window: {}", e);
    });

    window.set_target_fps(60);

    let mut display_buffers = Box::new(DisplayBuffers::default());
    let mut display = Box::new(MinifbDisplay::new(window));
    let mut image_source = DesktopImageSource::new("sdcard");
    let display_caps = display.caps();
    let mut application = Application::new(&mut display_buffers, &mut image_source, display_caps);
    let mut last_tick = std::time::Instant::now();

    while display.is_open() {
        display.update();
        let platform_events = display.take_input_events();
        let elapsed_ms = last_tick.elapsed().as_millis() as u32;
        last_tick = std::time::Instant::now();
        let frame = RuntimeFrame::new(display.get_buttons(), platform_events, elapsed_ms);
        update_application_frame(&mut application, &frame);
        draw_application_frame(&mut application, &mut *display);
    }
}
