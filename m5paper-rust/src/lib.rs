#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

mod allocator;
mod display;
mod ffi;
mod image_source;
mod runtime_host;

use alloc::alloc::{alloc_zeroed, handle_alloc_error, Layout};
use alloc::boxed::Box;
use core::panic::PanicInfo;

use display::M5PaperDisplay;
use image_source::M5PaperImageSource;
use runtime_host::M5PaperRuntimeHost;
use tern_core::{
    application::Application,
    framebuffer::DisplayBuffers,
    platform::{DisplayCaps, DisplayRotation, LogicalStyle, PlatformInputEvent},
    runtime_host::{draw_application_frame, update_application_frame},
};

fn display_caps() -> DisplayCaps {
    DisplayCaps {
        partial_refresh: true,
        gray_levels: 16,
        bits_per_pixel: 4,
        rotation: DisplayRotation::Rotate0,
        logical_style: LogicalStyle::TernPortrait,
    }
}

fn boxed_zeroed<T>() -> Box<T> {
    let layout = Layout::new::<T>();
    let ptr = unsafe { alloc_zeroed(layout) };
    if ptr.is_null() {
        handle_alloc_error(layout);
    }
    unsafe { Box::from_raw(ptr.cast()) }
}

#[unsafe(no_mangle)]
pub extern "C" fn tern_runtime_main() {
    ffi::log_line("m5paper-rust: starting\n\0");

    let start = ffi::backend_start();
    if start != ffi::Status::Ok {
        ffi::log_status(b"m5paper-rust: backend_start=%d\n\0", start);
        loop {
            ffi::delay_ms(1000);
        }
    }
    ffi::log_line("m5paper-rust: backend started\n\0");

    let info = match ffi::epd_init() {
        Ok(info) => info,
        Err(status) => {
            ffi::log_status(b"m5paper-rust: epd_init=%d\n\0", status);
            loop {
                ffi::delay_ms(1000);
            }
        }
    };
    unsafe {
        ffi::ets_printf(
            b"m5paper-rust: epd %ux%u buf=0x%08X vcom=%umV\n\0".as_ptr(),
            info.panel_width as u32,
            info.panel_height as u32,
            info.image_buffer_addr,
            info.vcom_mv as u32,
        );
    }
    let clear = ffi::epd_clear(true);
    ffi::log_status(b"m5paper-rust: epd_clear=%d\n\0", clear);
    let storage = ffi::storage_init();
    ffi::log_status(b"m5paper-rust: storage_init=%d\n\0", storage);
    let rtc = ffi::rtc_init();
    ffi::log_status(b"m5paper-rust: rtc_init=%d\n\0", rtc);

    ffi::log_line("m5paper-rust: alloc display buffers\n\0");
    let mut display_buffers: Box<DisplayBuffers> = boxed_zeroed();
    display_buffers.clear_screen(0xFF);
    display_buffers.copy_active_to_inactive();
    ffi::log_line("m5paper-rust: display buffers ready\n\0");

    ffi::log_line("m5paper-rust: init source\n\0");
    let mut source = M5PaperImageSource::new();
    ffi::log_line("m5paper-rust: init application\n\0");
    let mut application = Application::new(display_buffers.as_mut(), &mut source, display_caps());
    ffi::log_line("m5paper-rust: init display\n\0");
    let mut display = M5PaperDisplay::new();
    ffi::log_line("m5paper-rust: init host\n\0");
    let mut host = M5PaperRuntimeHost::new();
    ffi::log_line("m5paper-rust: entering loop\n\0");

    let mut first_frame = true;
    loop {
        let frame = host.next_frame(20);
        let should_draw = first_frame
            || frame.events.iter().any(|event| {
                matches!(
                    event,
                    PlatformInputEvent::ButtonDown(_)
                        | PlatformInputEvent::ButtonUp(_)
                        | PlatformInputEvent::TouchDown { .. }
                        | PlatformInputEvent::TouchUp { .. }
                )
            });
        update_application_frame(&mut application, &frame);
        if should_draw {
            if first_frame {
                let _ = ffi::epd_fill_white();
            }
            draw_application_frame(&mut application, &mut display);
            first_frame = false;
        }
        ffi::delay_ms(20);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    ffi::log_line("m5paper-rust: panic\n\0");
    loop {
        ffi::delay_ms(1000);
    }
}
