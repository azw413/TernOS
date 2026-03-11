extern crate alloc;

use alloc::vec::Vec;

use crate::display::RefreshMode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonId {
    Left,
    Right,
    Up,
    Down,
    Confirm,
    Back,
    Power,
    Menu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformInputEvent {
    ButtonDown(ButtonId),
    ButtonUp(ButtonId),
    TouchDown { x: i32, y: i32 },
    TouchMove { x: i32, y: i32 },
    TouchUp { x: i32, y: i32 },
    KeyDown { chr: u16, key_code: u16, modifiers: u16 },
    KeyUp { key_code: u16 },
    Tick,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayRotation {
    Rotate0,
    Rotate90,
    Rotate180,
    Rotate270,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayDensity {
    Palm160,
    Palm320,
    DeviceNative,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayCaps {
    pub partial_refresh: bool,
    pub grayscale: bool,
    pub rotation: DisplayRotation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformCaps {
    pub has_touch: bool,
    pub has_buttons: bool,
    pub has_keyboard: bool,
    pub has_wifi: bool,
    pub has_bluetooth: bool,
    pub supports_partial_refresh: bool,
    pub supports_grayscale: bool,
    pub supports_sleep: bool,
    pub supports_rtc_set: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SleepMode {
    Light,
    Deep,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryStatus {
    pub percent: Option<u8>,
    pub charging: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageEntry {
    pub name: alloc::string::String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformError {
    Unsupported,
    NotFound,
    Io,
    Invalid,
}

pub trait DisplayDevice {
    fn size_px(&self) -> (u32, u32);
    fn logical_density(&self) -> DisplayDensity;
    fn caps(&self) -> DisplayCaps;
    fn present(&mut self, mode: RefreshMode);
}

pub trait ClockDevice {
    fn monotonic_ms(&self) -> u64;
    fn rtc_seconds(&self) -> u32;
    fn set_rtc_seconds(&mut self, value: u32) -> Result<(), PlatformError>;
}

pub trait StorageDevice {
    fn read(&self, path: &str) -> Result<Vec<u8>, PlatformError>;
    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), PlatformError>;
    fn list(&self, path: &str) -> Result<Vec<StorageEntry>, PlatformError>;
    fn exists(&self, path: &str) -> bool;
}

pub trait PowerDevice {
    fn battery_status(&self) -> BatteryStatus;
    fn sleep(&mut self, mode: SleepMode) -> Result<(), PlatformError>;
}

pub trait Platform {
    type Display: DisplayDevice;
    type Clock: ClockDevice;
    type Storage: StorageDevice;
    type Power: PowerDevice;

    fn caps(&self) -> PlatformCaps;
    fn display(&mut self) -> &mut Self::Display;
    fn clock(&mut self) -> &mut Self::Clock;
    fn storage(&mut self) -> &mut Self::Storage;
    fn power(&mut self) -> &mut Self::Power;
    fn poll_input(&mut self, sink: &mut dyn FnMut(PlatformInputEvent));
}
