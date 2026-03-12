use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::ffi::CString;

use tern_core::display::RefreshMode;
use tern_core::platform::{
    BatteryStatus, ButtonId, ClockDevice, DisplayCaps, DisplayDensity, DisplayDevice,
    DisplayRotation, Platform, PlatformCaps, PlatformError, PlatformInputEvent, PowerDevice, SleepMode,
    StorageDevice, StorageEntry,
};

use crate::ffi;

pub const DISPLAY_WIDTH: u16 = 540;
pub const DISPLAY_HEIGHT: u16 = 960;

pub struct M5PaperIdfDisplay;

impl M5PaperIdfDisplay {
    pub fn clear(&mut self, init: bool) -> Result<(), ffi::Status> {
        ffi::epd_clear(init)
    }

    pub fn update_region(
        &mut self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        data: &[u8],
    ) -> Result<(), ffi::Status> {
        ffi::epd_update_region(x, y, width, height, data)
    }
}

impl DisplayDevice for M5PaperIdfDisplay {
    fn size_px(&self) -> (u32, u32) {
        (DISPLAY_WIDTH as u32, DISPLAY_HEIGHT as u32)
    }

    fn logical_density(&self) -> DisplayDensity {
        DisplayDensity::DeviceNative
    }

    fn caps(&self) -> DisplayCaps {
        DisplayCaps {
            partial_refresh: true,
            grayscale: true,
            rotation: DisplayRotation::Rotate0,
        }
    }

    fn present(&mut self, _mode: RefreshMode) {}
}

pub struct M5PaperIdfClock;

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = (year - era * 400) as i64;
    let month = month as i64;
    let day = day as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era as i64 * 146097 + doe - 719468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

fn rtc_datetime_to_unix(datetime: ffi::RtcDateTime) -> u32 {
    let days = days_from_civil(datetime.year as i32, datetime.month as u32, datetime.day as u32);
    let secs = days * 86_400
        + i64::from(datetime.hour) * 3_600
        + i64::from(datetime.minute) * 60
        + i64::from(datetime.second);
    secs.max(0) as u32
}

fn unix_to_rtc_datetime(seconds: u32) -> ffi::RtcDateTime {
    let seconds = i64::from(seconds);
    let days = seconds.div_euclid(86_400);
    let rem = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = (rem / 3_600) as u8;
    let minute = ((rem % 3_600) / 60) as u8;
    let second = (rem % 60) as u8;
    let week = ((days + 4).rem_euclid(7)) as u8;
    ffi::RtcDateTime {
        year: year as u16,
        month: month as u8,
        day: day as u8,
        week,
        hour,
        minute,
        second,
    }
}

impl ClockDevice for M5PaperIdfClock {
    fn monotonic_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64
    }

    fn rtc_seconds(&self) -> u32 {
        match ffi::rtc_read() {
            Ok(datetime) => rtc_datetime_to_unix(datetime),
            Err(_) => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs() as u32,
        }
    }

    fn set_rtc_seconds(&mut self, value: u32) -> Result<(), PlatformError> {
        let datetime = unix_to_rtc_datetime(value);
        ffi::rtc_set(&datetime).map_err(|_| PlatformError::Io)
    }
}

pub struct M5PaperIdfStorage;

impl StorageDevice for M5PaperIdfStorage {
    fn read(&self, path: &str) -> Result<Vec<u8>, PlatformError> {
        let c_path = CString::new(path).map_err(|_| PlatformError::Invalid)?;
        let size = ffi::storage_file_size(&c_path).map_err(|err| match err {
            ffi::Status::NotFound => PlatformError::NotFound,
            ffi::Status::Unsupported => PlatformError::Unsupported,
            _ => PlatformError::Io,
        })?;

        let mut data = vec![0u8; size as usize];
        let mut offset = 0u32;
        while offset < size {
            let read = ffi::storage_read_chunk(&c_path, offset, &mut data[offset as usize..])
                .map_err(|err| match err {
                    ffi::Status::NotFound => PlatformError::NotFound,
                    ffi::Status::Unsupported => PlatformError::Unsupported,
                    _ => PlatformError::Io,
                })?;
            if read == 0 {
                return Err(PlatformError::Io);
            }
            offset += read;
        }
        Ok(data)
    }

    fn write(&mut self, _path: &str, _data: &[u8]) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported)
    }

    fn list(&self, path: &str) -> Result<Vec<StorageEntry>, PlatformError> {
        let c_path = CString::new(path).map_err(|_| PlatformError::Invalid)?;
        ffi::storage_list_begin(&c_path).map_err(|err| match err {
            ffi::Status::NotFound => PlatformError::NotFound,
            ffi::Status::Unsupported => PlatformError::Unsupported,
            _ => PlatformError::Io,
        })?;

        let mut entries = Vec::new();
        loop {
            match ffi::storage_list_next() {
                Ok(Some(entry)) => {
                    let nul_pos = entry.name.iter().position(|&b| b == 0).unwrap_or(entry.name.len());
                    let name = String::from_utf8_lossy(&entry.name[..nul_pos]).into_owned();
                    entries.push(StorageEntry {
                        name,
                        is_dir: entry.is_dir,
                        size: u64::from(entry.size),
                    });
                }
                Ok(None) => break,
                Err(err) => {
                    ffi::storage_list_end();
                    return Err(match err {
                        ffi::Status::NotFound => PlatformError::NotFound,
                        ffi::Status::Unsupported => PlatformError::Unsupported,
                        _ => PlatformError::Io,
                    });
                }
            }
        }
        ffi::storage_list_end();
        Ok(entries)
    }

    fn exists(&self, path: &str) -> bool {
        match CString::new(path) {
            Ok(c_path) => ffi::storage_exists(&c_path),
            Err(_) => false,
        }
    }
}

pub struct M5PaperIdfPower;

impl PowerDevice for M5PaperIdfPower {
    fn battery_status(&self) -> BatteryStatus {
        BatteryStatus {
            percent: None,
            charging: false,
        }
    }

    fn sleep(&mut self, _mode: SleepMode) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported)
    }
}

pub struct M5PaperIdfPlatform {
    display: M5PaperIdfDisplay,
    clock: M5PaperIdfClock,
    storage: M5PaperIdfStorage,
    power: M5PaperIdfPower,
}

impl M5PaperIdfPlatform {
    pub fn new() -> Self {
        Self {
            display: M5PaperIdfDisplay,
            clock: M5PaperIdfClock,
            storage: M5PaperIdfStorage,
            power: M5PaperIdfPower,
        }
    }

    pub fn init(&mut self) -> Result<ffi::EpdInfo, ffi::Status> {
        let board = ffi::board_init();
        if board != ffi::Status::Ok {
            return Err(board);
        }
        let info = ffi::epd_init()?;
        ffi::touch_init()?;
        ffi::rtc_init()?;
        ffi::storage_init()?;
        Ok(info)
    }

    pub fn draw_test_rect(&mut self) -> Result<(), ffi::Status> {
        let buf = vec![0xFF; (64 * 64) / 2];
        self.display.update_region(0, 0, 64, 64, &buf)
    }

    pub fn run_demo_loop(&mut self) -> ! {
        let mut last_touch_down = false;
        let mut last_logged = None;
        loop {
            if let Ok(state) = ffi::touch_read() {
                if state.touched != last_touch_down
                    || state.touched && last_logged != Some((state.x, state.y, state.count))
                {
                    println!(
                        "rust m5paper: touch touched={} x={} y={} count={}",
                        state.touched, state.x, state.y, state.count
                    );
                    last_touch_down = state.touched;
                    last_logged = Some((state.x, state.y, state.count));
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
    }
}

impl Default for M5PaperIdfPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for M5PaperIdfPlatform {
    type Display = M5PaperIdfDisplay;
    type Clock = M5PaperIdfClock;
    type Storage = M5PaperIdfStorage;
    type Power = M5PaperIdfPower;

    fn caps(&self) -> PlatformCaps {
        PlatformCaps {
            has_touch: true,
            has_buttons: true,
            has_keyboard: false,
            has_wifi: true,
            has_bluetooth: true,
            supports_partial_refresh: true,
            supports_grayscale: true,
            supports_sleep: true,
            supports_rtc_set: true,
        }
    }

    fn display(&mut self) -> &mut Self::Display {
        &mut self.display
    }

    fn clock(&mut self) -> &mut Self::Clock {
        &mut self.clock
    }

    fn storage(&mut self) -> &mut Self::Storage {
        &mut self.storage
    }

    fn power(&mut self) -> &mut Self::Power {
        &mut self.power
    }

    fn poll_input(&mut self, sink: &mut dyn FnMut(PlatformInputEvent)) {
        while let Ok(Some(event)) = ffi::input_next() {
            match event.event_type {
                x if x == ffi::InputEventType::ButtonDown as u8 => {
                    let Some(button) = map_button_id(event.button_id) else {
                        continue;
                    };
                    sink(PlatformInputEvent::ButtonDown(button));
                }
                x if x == ffi::InputEventType::ButtonUp as u8 => {
                    let Some(button) = map_button_id(event.button_id) else {
                        continue;
                    };
                    sink(PlatformInputEvent::ButtonUp(button));
                }
                x if x == ffi::InputEventType::TouchDown as u8 => {
                    sink(PlatformInputEvent::TouchDown {
                        x: event.x as i32,
                        y: event.y as i32,
                    });
                }
                x if x == ffi::InputEventType::TouchMove as u8 => {
                    sink(PlatformInputEvent::TouchMove {
                        x: event.x as i32,
                        y: event.y as i32,
                    });
                }
                x if x == ffi::InputEventType::TouchUp as u8 => {
                    sink(PlatformInputEvent::TouchUp {
                        x: event.x as i32,
                        y: event.y as i32,
                    });
                }
                _ => {}
            }
        }
    }
}

fn map_button_id(button_id: u8) -> Option<ButtonId> {
    match button_id {
        x if x == ffi::ButtonId::Up as u8 => Some(ButtonId::Up),
        x if x == ffi::ButtonId::Down as u8 => Some(ButtonId::Down),
        x if x == ffi::ButtonId::Power as u8 => Some(ButtonId::Power),
        _ => None,
    }
}
