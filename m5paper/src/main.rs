#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

mod platform;
mod hardware;
mod ffi;

use embedded_hal::digital::OutputPin;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Address, Command, Config as SpiConfig, DataMode, Spi};
use esp_hal::time::{Duration as HalDuration, Rate};
use esp_hal::timer::timg::TimerGroup;
use log::{error, info, warn};

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

fn set_sd_spi_mode<SPI, CS>(spi: &mut SPI, sd_cs: &mut CS)
where
    SPI: embedded_hal::spi::SpiBus<u8>,
    CS: OutputPin,
{
    let dummy = [0xFFu8; 32];
    let _ = sd_cs.set_high();
    let _ = spi.write(&dummy);
    let _ = sd_cs.set_low();

    let mut cmd58 = [0x7A, 0x00, 0x00, 0x00, 0x00, 0xFD, 0xFF, 0xFF];
    let _ = spi.transfer_in_place(&mut cmd58);

    if cmd58[6] == cmd58[7] {
        let _ = sd_cs.set_high();
        let _ = spi.write(&dummy);
        let _ = sd_cs.set_low();
        let cmd0 = [0x40, 0x00, 0x00, 0x00, 0x00, 0x95, 0xFF, 0xFF];
        let _ = spi.write(&cmd0);
    }

    let _ = sd_cs.set_high();
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 65536);

    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let delay = Delay::new();
    let caps = platform::caps();
    let pins = platform::pins();

    let _main_power = Output::new(peripherals.GPIO2, Level::High, OutputConfig::default());
    let _ext_power = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
    #[cfg(not(feature = "cshim"))]
    let mut sd_cs = Output::new(peripherals.GPIO4, Level::High, OutputConfig::default());
    #[cfg(feature = "cshim")]
    let _sd_cs = Output::new(peripherals.GPIO4, Level::High, OutputConfig::default());
    let _epd_power = Output::new(peripherals.GPIO23, Level::High, OutputConfig::default());
    delay.delay(HalDuration::from_millis(1000));

    info!(
        "m5paper bootstrap touch={} buttons={} rtc_set={} display={}x{} sck={} miso={} mosi={} cs={} busy={} rst={} tf_cs={} spi_mode=0 spi_host=3",
        caps.has_touch,
        caps.has_buttons,
        caps.supports_rtc_set,
        platform::DISPLAY_WIDTH,
        platform::DISPLAY_HEIGHT,
        pins.spi_sck,
        pins.spi_miso,
        pins.spi_mosi,
        pins.epd_cs,
        pins.epd_busy,
        23,
        pins.sd_cs
    );

    let dial_right = Input::new(peripherals.GPIO37, InputConfig::default());
    let dial_press = Input::new(peripherals.GPIO38, InputConfig::default());
    let dial_left = Input::new(peripherals.GPIO39, InputConfig::default());
    let touch_int = Input::new(peripherals.GPIO36, InputConfig::default());
    info!(
        "m5paper inputs dial_right={} dial_press={} dial_left={} touch_int={}",
        dial_right.is_high(),
        dial_press.is_high(),
        dial_left.is_high(),
        touch_int.is_high()
    );

    #[cfg(feature = "cshim")]
    {
        info!("m5paper cshim feature enabled");
        let board = ffi::board_init();
        info!("m5paper cshim board_init={}", board);
        match ffi::epd_init() {
            Ok(info) => info!(
                "m5paper cshim epd_init panel={}x{} img_buf=0x{:08X} vcom={}mV",
                info.panel_width,
                info.panel_height,
                info.image_buffer_addr,
                info.vcom_mv
            ),
            Err(err) => warn!("m5paper cshim epd_init failed: {}", err),
        }
        match ffi::epd_clear(true) {
            Ok(()) => info!("m5paper cshim epd_clear ok"),
            Err(err) => warn!("m5paper cshim epd_clear failed: {}", err),
        }
        match ffi::touch_init() {
            Ok(()) => info!("m5paper cshim touch_init ok"),
            Err(err) => warn!("m5paper cshim touch_init failed: {}", err),
        }
        match ffi::touch_read() {
            Ok(state) => info!(
                "m5paper cshim touch touched={} count={} x={} y={}",
                state.touched,
                state.count,
                state.x,
                state.y
            ),
            Err(err) => warn!("m5paper cshim touch_read failed: {}", err),
        }
        loop {
            Timer::after(Duration::from_secs(1)).await;
        }
    }

    #[cfg(not(feature = "cshim"))]
    {
    let epd_busy_pullup = Input::new(
        peripherals.GPIO27,
        InputConfig::default().with_pull(Pull::Up),
    );
    let busy_initial = epd_busy_pullup.is_high();
    let epd_cs = Output::new(peripherals.GPIO15, Level::High, OutputConfig::default());
    let spi_cfg = SpiConfig::default()
        .with_frequency(Rate::from_mhz(10))
        .with_mode(Mode::_0);
    let epd_spi = Spi::new(peripherals.SPI3, spi_cfg)
        .expect("Failed to create SPI3")
        .with_sck(peripherals.GPIO14)
        .with_mosi(peripherals.GPIO12)
        .with_miso(peripherals.GPIO13);
    let mut epd_spi = epd_spi;
    set_sd_spi_mode(&mut epd_spi, &mut sd_cs);
    let epd_busy = epd_busy_pullup;
    info!("epd busy initial_high={}", busy_initial);
    if !busy_initial {
        let start = embassy_time::Instant::now();
        loop {
            if epd_busy.is_high() {
                info!(
                    "epd busy became high after {} ms",
                    embassy_time::Instant::now()
                        .duration_since(start)
                        .as_millis()
                );
                break;
            }
            if embassy_time::Instant::now()
                .duration_since(start)
                .as_millis()
                > 1024
            {
                warn!("epd busy did not go high within 1024 ms after power-up");
                break;
            }
            Timer::after(Duration::from_millis(1)).await;
        }
    }
    let variants = [
        ("be_be", hardware::WordOrder::Be, hardware::WordOrder::Be),
        ("be_le", hardware::WordOrder::Be, hardware::WordOrder::Le),
        ("le_be", hardware::WordOrder::Le, hardware::WordOrder::Be),
        ("le_le", hardware::WordOrder::Le, hardware::WordOrder::Le),
    ];
    let mut epd_spi = epd_spi;
    let mut epd_cs = epd_cs;
    let mut epd_busy = epd_busy;
    for (label, write_order, read_order) in variants {
        let mut ready = false;
        let start = embassy_time::Instant::now();
        loop {
            if epd_busy.is_high() {
                info!(
                    "epd variant={} busy became high after {} ms",
                    label,
                    embassy_time::Instant::now().duration_since(start).as_millis()
                );
                ready = true;
                break;
            }
            if embassy_time::Instant::now().duration_since(start).as_millis() > 1024 {
                warn!("epd variant={} busy did not go high within 1024 ms", label);
                break;
            }
            Timer::after(Duration::from_millis(1)).await;
        }
        if !ready {
            continue;
        }

        let mut direct_probe = [0u8; 40];
    let half_write = epd_spi.half_duplex_write(
        DataMode::Single,
        Command::_16Bit(0x6000, DataMode::Single),
        Address::_16Bit(0x0302, DataMode::Single),
        0,
        &[0x00],
    );
    info!("epd direct half_write ok={}", half_write.is_ok());
    let half_read = epd_spi.half_duplex_read(
        DataMode::Single,
        Command::_16Bit(0x1000, DataMode::Single),
        Address::_16Bit(0x0000, DataMode::Single),
        0,
        &mut direct_probe,
    );
    info!(
        "epd direct half_read ok={} bytes={:02X?}",
        half_read.is_ok(),
        &direct_probe[..8]
    );

    let mut epd = hardware::It8951::new(epd_spi, epd_cs, epd_busy);
        epd.set_word_order(write_order, read_order);
        match epd.init_m5epd_style() {
            Ok(info) => {
                info!(
                    "epd variant={} legacy init ok panel={}x{} img_buf=0x{:08X} vcom={}mV fallback={}",
                    label,
                    info.panel_width,
                    info.panel_height,
                    info.image_buffer_addr,
                    info.vcom_mv,
                    info.used_fallback
                );
                match epd.read_vcom_m5epd_style() {
                    Ok(vcom) => info!("epd variant={} readback vcom={}mV", label, vcom),
                    Err(err) => warn!("epd variant={} readback vcom failed: {}", label, err),
                }
                match epd.read_lutafsr_m5epd_style() {
                    Ok(status) => info!("epd variant={} readback lutafsr=0x{:04X}", label, status),
                    Err(err) => warn!("epd variant={} readback lutafsr failed: {}", label, err),
                }
                match epd.get_sysinfo_m5epd_style() {
                    Ok(words) => info!(
                        "epd variant={} sysinfo words={:04X?} panel={}x{} mem=0x{:04X}{:04X}",
                        label,
                        &words[..8],
                        words[0],
                        words[1],
                        words[3],
                        words[2]
                    ),
                    Err(err) => warn!("epd variant={} sysinfo failed: {}", label, err),
                }
                if label == "be_be" {
                    match epd.clear_m5epd_style(true) {
                        Ok(()) => info!("epd variant={} legacy clear dispatched", label),
                        Err(err) => warn!("epd variant={} legacy clear failed: {}", label, err),
                    }
                    match epd.read_lutafsr_m5epd_style() {
                        Ok(status) => info!("epd variant={} post-clear lutafsr=0x{:04X}", label, status),
                        Err(err) => warn!("epd variant={} post-clear lutafsr failed: {}", label, err),
                    }
                }
            }
            Err(err) => warn!("epd variant={} legacy init failed: {}", label, err),
        }
        (epd_spi, epd_cs, epd_busy) = epd.release();
    }
    Timer::after(Duration::from_millis(250)).await;
    }

    let mut i2c = I2c::new(peripherals.I2C0, I2cConfig::default().with_frequency(Rate::from_khz(100)))
        .expect("Failed to create I2C0")
        .with_sda(peripherals.GPIO21)
        .with_scl(peripherals.GPIO22);
    match hardware::scan_i2c_bus(&mut i2c) {
        Ok(found) => {
            info!(
                "i2c scan touch14={} touch5d={} rtc(0x51)={} sht30(0x44)={} eeprom(0x50)={}",
                found[0x14],
                found[0x5D],
                found[0x51],
                found[0x44],
                found[0x50]
            );
            if found[0x51] {
                match hardware::read_i2c_reg8(&mut i2c, 0x51, 0x02) {
                    Ok(seconds_bcd) => info!("rtc bm8563 seconds_bcd=0x{:02X}", seconds_bcd),
                    Err(err) => warn!("rtc bm8563 read failed: {}", err),
                }
            }
            if found[0x50] {
                match hardware::read_i2c_reg8(&mut i2c, 0x50, 0x00) {
                    Ok(byte0) => info!("eeprom fm24c02 byte0=0x{:02X}", byte0),
                    Err(err) => warn!("eeprom fm24c02 read failed: {}", err),
                }
            }
            if found[0x44] {
                match hardware::read_sht30_status(&mut i2c) {
                    Ok(status) => info!(
                        "sensor sht30 status_raw={:02X}{:02X}{:02X}",
                        status[0], status[1], status[2]
                    ),
                    Err(err) => warn!("sensor sht30 status read failed: {}", err),
                }
            }
            let touch_addr = if found[0x5D] {
                Some(0x5D)
            } else if found[0x14] {
                Some(0x14)
            } else {
                None
            };
            if let Some(addr) = touch_addr {
                let mut product_id = [0u8; 4];
                match hardware::read_i2c_reg16(&mut i2c, addr, 0x8140, &mut product_id) {
                    Ok(()) => info!(
                        "touch gt911 addr=0x{:02X} product_id='{}'",
                        addr,
                        core::str::from_utf8(&product_id).unwrap_or("????")
                    ),
                    Err(err) => warn!("touch gt911 read failed at 0x{:02X}: {}", addr, err),
                }
            }
        }
        Err(err) => error!("i2c scan failed: {}", err),
    }

    loop {
        Timer::after(Duration::from_millis(250)).await;
    }
}
