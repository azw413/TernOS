use tern_core::platform::PlatformCaps;

// M5Paper uses an IT8951 controller. The `rust-it8951` repository the user
// provided is a useful protocol reference, but it is a desktop USB driver
// built on `rusb`; it cannot be linked directly into this ESP32 target.
pub const DISPLAY_WIDTH: u32 = 540;
pub const DISPLAY_HEIGHT: u32 = 960;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinMap {
    pub spi_miso: u8,
    pub spi_mosi: u8,
    pub spi_sck: u8,
    pub epd_cs: u8,
    pub epd_busy: u8,
    pub sd_cs: u8,
    pub touch_sda: u8,
    pub touch_scl: u8,
    pub touch_int: u8,
    pub dial_right: u8,
    pub dial_press: u8,
    pub dial_left: u8,
    pub power_hold: u8,
    pub ext_power_en: u8,
    pub epd_power_en: u8,
    pub battery_adc: u8,
}

pub fn caps() -> PlatformCaps {
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

pub fn pins() -> PinMap {
    PinMap {
        spi_miso: 13,
        spi_mosi: 12,
        spi_sck: 14,
        epd_cs: 15,
        epd_busy: 27,
        sd_cs: 4,
        touch_sda: 21,
        touch_scl: 22,
        touch_int: 36,
        dial_right: 37,
        dial_press: 38,
        dial_left: 39,
        power_hold: 2,
        ext_power_en: 5,
        epd_power_en: 23,
        battery_adc: 35,
    }
}
