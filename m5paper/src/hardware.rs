use alloc::string::String;
use core::fmt;

use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal::i2c::I2c;
use embedded_hal::spi::SpiBus;
use log::warn;

const IT8951_TCON_SYS_RUN: u16 = 0x0001;
const IT8951_TCON_STANDBY: u16 = 0x0002;
const IT8951_TCON_SLEEP: u16 = 0x0003;
const IT8951_TCON_REG_RD: u16 = 0x0010;
const IT8951_TCON_REG_WR: u16 = 0x0011;
const IT8951_TCON_LD_IMG_AREA: u16 = 0x0021;
const IT8951_TCON_LD_IMG_END: u16 = 0x0022;

const USDEF_I80_CMD_DPY_BUF_AREA: u16 = 0x0037;
const USDEF_I80_CMD_GET_DEV_INFO: u16 = 0x0302;
const USDEF_I80_CMD_VCOM: u16 = 0x0039;

const IT8951_I80CPCR: u16 = 0x0004;
const IT8951_LISAR: u16 = 0x0208;
const DISPLAY_REG_BASE: u16 = 0x1000;
const LUTAFSR: u16 = DISPLAY_REG_BASE + 0x224;

const IT8951_LDIMG_B_ENDIAN: u16 = 1;
const IT8951_LDIMG_L_ENDIAN: u16 = 0;
const IT8951_4BPP: u16 = 2;
const IT8951_ROTATE_0: u16 = 0;
const UPDATE_MODE_INIT: u16 = 0;
const UPDATE_MODE_GC16: u16 = 2;
const M5PAPER_PANEL_WIDTH: u16 = 960;
const M5PAPER_PANEL_HEIGHT: u16 = 540;
const M5PAPER_IMAGE_BUFFER_ADDR: u32 = 0x0012_36E0;
const READY_TIMEOUT_LOOPS: u32 = 5_000_000;
const DISPLAY_TIMEOUT_LOOPS: u32 = 20_000_000;

#[derive(Debug)]
pub enum BringupError {
    BusyTimeout,
    Spi,
    Gpio,
    I2c,
}

impl fmt::Display for BringupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BringupError::BusyTimeout => f.write_str("busy timeout"),
            BringupError::Spi => f.write_str("spi error"),
            BringupError::Gpio => f.write_str("gpio error"),
            BringupError::I2c => f.write_str("i2c error"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EpdSystemInfo {
    pub panel_width: u16,
    pub panel_height: u16,
    pub image_buffer_addr: u32,
    pub vcom_mv: u16,
    pub used_fallback: bool,
    pub fw_version: String,
    pub lut_version: String,
}

pub struct It8951<SPI, CS, BUSY> {
    spi: SPI,
    cs: CS,
    busy: BUSY,
    write_order: WordOrder,
    read_order: WordOrder,
}

#[derive(Copy, Clone, Debug)]
pub enum WordOrder {
    Be,
    Le,
}

impl<SPI, CS, BUSY> It8951<SPI, CS, BUSY>
where
    SPI: SpiBus<u8>,
    CS: OutputPin,
    BUSY: InputPin,
{
    pub fn new(spi: SPI, cs: CS, busy: BUSY) -> Self {
        Self {
            spi,
            cs,
            busy,
            write_order: WordOrder::Be,
            read_order: WordOrder::Be,
        }
    }

    pub fn set_word_order(&mut self, write_order: WordOrder, read_order: WordOrder) {
        self.write_order = write_order;
        self.read_order = read_order;
    }

    pub fn init(&mut self) -> Result<EpdSystemInfo, BringupError> {
        self.write_command(IT8951_TCON_SYS_RUN)?;
        self.write_reg(IT8951_I80CPCR, 0x0001)?;
        self.write_command(USDEF_I80_CMD_VCOM)?;
        self.write_word(0x0001)?;
        self.write_word(2300)?;
        let vcom_mv = self.read_vcom()?;
        let info = self.get_system_info().unwrap_or_else(|_| fallback_system_info(vcom_mv));
        if info.panel_width == 0 || info.panel_height == 0 || info.image_buffer_addr == 0 {
            Ok(fallback_system_info(vcom_mv))
        } else {
            Ok(EpdSystemInfo { vcom_mv, ..info })
        }
    }

    pub fn init_m5epd_style(&mut self) -> Result<EpdSystemInfo, BringupError> {
        let info = fallback_system_info(2300);
        self.write_command(IT8951_TCON_SYS_RUN)?;
        self.write_reg(IT8951_I80CPCR, 0x0001)?;
        self.write_command(USDEF_I80_CMD_VCOM)?;
        self.write_word(0x0001)?;
        self.write_word(2300)?;
        Ok(info)
    }

    pub fn read_vcom_m5epd_style(&mut self) -> Result<u16, BringupError> {
        self.write_command(USDEF_I80_CMD_VCOM)?;
        self.write_word(0x0000)?;
        self.read_word()
    }

    pub fn read_lutafsr_m5epd_style(&mut self) -> Result<u16, BringupError> {
        self.write_command(IT8951_TCON_REG_RD)?;
        self.write_word(LUTAFSR)?;
        self.read_word()
    }

    pub fn get_sysinfo_m5epd_style(&mut self) -> Result<[u16; 20], BringupError> {
        self.write_command(USDEF_I80_CMD_GET_DEV_INFO)?;
        let mut words = [0u16; 20];
        self.read_words(&mut words)?;
        Ok(words)
    }

    pub fn clear_m5epd_style(&mut self, init: bool) -> Result<(), BringupError> {
        self.set_target_memory_addr(M5PAPER_IMAGE_BUFFER_ADDR)?;
        self.set_area_with_mode(
            0,
            0,
            M5PAPER_PANEL_WIDTH,
            M5PAPER_PANEL_HEIGHT,
            IT8951_LDIMG_L_ENDIAN,
        )?;
        for _ in 0..((u32::from(M5PAPER_PANEL_WIDTH) * u32::from(M5PAPER_PANEL_HEIGHT)) >> 2) {
            self.write_gram_data(0xFFFF)?;
        }
        self.write_command(IT8951_TCON_LD_IMG_END)?;
        if init {
            self.update_area_m5epd_style(
                0,
                0,
                M5PAPER_PANEL_WIDTH,
                M5PAPER_PANEL_HEIGHT,
                UPDATE_MODE_INIT,
            )?;
        }
        Ok(())
    }

    pub fn probe_devinfo_raw(&mut self) -> Result<[u8; 40], BringupError> {
        self.probe_devinfo_raw_with(ProbeReadMode::TransferInPlace)
    }

    pub fn probe_devinfo_raw_with(
        &mut self,
        mode: ProbeReadMode,
    ) -> Result<[u8; 40], BringupError> {
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        self.write_u16_raw(0x6000)?;
        self.write_u16_raw(USDEF_I80_CMD_GET_DEV_INFO)?;
        self.cs.set_high().map_err(|_| BringupError::Gpio)?;

        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        self.cs.set_low().map_err(|_| BringupError::Gpio)?;
        self.write_u16_raw(0x1000)?;
        self.write_u16_raw(0x0000)?;
        let mut buf = [0u8; 40];
        match mode {
            ProbeReadMode::Read => {
                self.spi.read(&mut buf).map_err(|_| BringupError::Spi)?;
            }
            ProbeReadMode::TransferInPlace => {
                self.spi
                    .transfer_in_place(&mut buf)
                    .map_err(|_| BringupError::Spi)?;
            }
            ProbeReadMode::TransferZeros => {
                let tx = [0u8; 40];
                self.spi
                    .transfer(&mut buf, &tx)
                    .map_err(|_| BringupError::Spi)?;
            }
            ProbeReadMode::TransferOnes => {
                let tx = [0xFFu8; 40];
                self.spi
                    .transfer(&mut buf, &tx)
                    .map_err(|_| BringupError::Spi)?;
            }
        }
        self.cs.set_high().map_err(|_| BringupError::Gpio)?;
        Ok(buf)
    }

    pub fn fill_screen(
        &mut self,
        info: &EpdSystemInfo,
        pixel_word: u16,
        update_mode: u16,
    ) -> Result<u16, BringupError> {
        if let Err(err) = self.set_target_memory_addr(info.image_buffer_addr) {
            warn!("epd clear step=set_target_memory_addr failed: {}", err);
            return Err(err);
        }
        if let Err(err) = self.set_area(0, 0, info.panel_width, info.panel_height) {
            warn!("epd clear step=set_area failed: {}", err);
            return Err(err);
        }

        // Match M5GFX: hold CS active for the whole image stream after the area command.
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        self.write_u16_raw(0x0000)?;

        let words = (u32::from(info.panel_width) * u32::from(info.panel_height)) >> 2;
        for _ in 0..words {
            self.write_u16_raw(pixel_word)?;
        }

        if let Err(err) = self.write_command(IT8951_TCON_LD_IMG_END) {
            warn!("epd clear step=ld_img_end failed: {}", err);
            return Err(err);
        }
        if let Err(err) = self.update_full(info, update_mode) {
            warn!("epd clear step=update_full failed: {}", err);
            return Err(err);
        }
        self.lutafsr()
    }

    pub fn split_test_pattern(
        &mut self,
        info: &EpdSystemInfo,
        update_mode: u16,
    ) -> Result<u16, BringupError> {
        if let Err(err) = self.set_target_memory_addr(info.image_buffer_addr) {
            warn!("epd pattern step=set_target_memory_addr failed: {}", err);
            return Err(err);
        }
        if let Err(err) = self.set_area(0, 0, info.panel_width, info.panel_height) {
            warn!("epd pattern step=set_area failed: {}", err);
            return Err(err);
        }

        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        self.write_u16_raw(0x0000)?;

        let half_rows = u32::from(info.panel_height) / 2;
        let top_words = (u32::from(info.panel_width) * half_rows) >> 2;
        let total_words = (u32::from(info.panel_width) * u32::from(info.panel_height)) >> 2;

        for i in 0..total_words {
            let pixel_word = if i < top_words { 0x0000 } else { 0xFFFF };
            self.write_u16_raw(pixel_word)?;
        }

        if let Err(err) = self.write_command(IT8951_TCON_LD_IMG_END) {
            warn!("epd pattern step=ld_img_end failed: {}", err);
            return Err(err);
        }
        if let Err(err) = self.update_full(info, update_mode) {
            warn!("epd pattern step=update_full failed: {}", err);
            return Err(err);
        }
        self.lutafsr()
    }

    pub fn clear_white(&mut self, info: &EpdSystemInfo) -> Result<u16, BringupError> {
        self.fill_screen(info, 0xFFFF, UPDATE_MODE_GC16)
    }

    pub fn clear_black(&mut self, info: &EpdSystemInfo) -> Result<u16, BringupError> {
        self.fill_screen(info, 0x0000, UPDATE_MODE_INIT)
    }

    pub fn draw_split_pattern(&mut self, info: &EpdSystemInfo) -> Result<u16, BringupError> {
        self.split_test_pattern(info, UPDATE_MODE_GC16)
    }

    pub fn release(self) -> (SPI, CS, BUSY) {
        (self.spi, self.cs, self.busy)
    }

    pub fn busy_high(&mut self) -> Result<bool, BringupError> {
        self.busy.is_high().map_err(|_| BringupError::Gpio)
    }

    pub fn lutafsr(&mut self) -> Result<u16, BringupError> {
        self.read_reg(LUTAFSR)
    }

    pub fn wait_for_display_idle_public(&mut self, timeout_loops: u32) -> Result<(), BringupError> {
        self.wait_for_display_idle(timeout_loops)
    }

    fn get_system_info(&mut self) -> Result<EpdSystemInfo, BringupError> {
        self.write_command(USDEF_I80_CMD_GET_DEV_INFO)?;
        let mut words = [0u16; 20];
        self.read_words(&mut words)?;
        Ok(EpdSystemInfo {
            panel_width: words[0],
            panel_height: words[1],
            image_buffer_addr: u32::from(words[2]) | (u32::from(words[3]) << 16),
            vcom_mv: 0,
            used_fallback: false,
            fw_version: decode_u16_c_string(&words[4..12]),
            lut_version: decode_u16_c_string(&words[12..20]),
        })
    }

    fn read_vcom(&mut self) -> Result<u16, BringupError> {
        self.write_command(USDEF_I80_CMD_VCOM)?;
        self.write_word(0x0000)?;
        self.read_word()
    }

    fn set_target_memory_addr(&mut self, addr: u32) -> Result<(), BringupError> {
        self.write_reg(IT8951_LISAR + 2, ((addr >> 16) & 0xFFFF) as u16)?;
        self.write_reg(IT8951_LISAR, (addr & 0xFFFF) as u16)?;
        Ok(())
    }

    fn set_area(&mut self, x: u16, y: u16, w: u16, h: u16) -> Result<(), BringupError> {
        self.set_area_with_mode(x, y, w, h, IT8951_LDIMG_B_ENDIAN)
    }

    fn set_area_with_mode(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        endian: u16,
    ) -> Result<(), BringupError> {
        let args = [
            (endian << 8) | (IT8951_4BPP << 4) | IT8951_ROTATE_0,
            x,
            y,
            w,
            h,
        ];
        self.write_args(IT8951_TCON_LD_IMG_AREA, &args)
    }

    fn write_gram_data(&mut self, value: u16) -> Result<(), BringupError> {
        self.cs.set_low().map_err(|_| BringupError::Gpio)?;
        self.write_u32_raw(0x0000, value)?;
        self.cs.set_high().map_err(|_| BringupError::Gpio)?;
        Ok(())
    }

    fn update_area_m5epd_style(
        &mut self,
        x: u16,
        y: u16,
        mut w: u16,
        mut h: u16,
        update_mode: u16,
    ) -> Result<(), BringupError> {
        self.check_afsr()?;
        if x + w > M5PAPER_PANEL_WIDTH {
            w = M5PAPER_PANEL_WIDTH - x;
        }
        if y + h > M5PAPER_PANEL_HEIGHT {
            h = M5PAPER_PANEL_HEIGHT - y;
        }
        let args = [
            x,
            y,
            w,
            h,
            update_mode,
            (M5PAPER_IMAGE_BUFFER_ADDR & 0xFFFF) as u16,
            ((M5PAPER_IMAGE_BUFFER_ADDR >> 16) & 0xFFFF) as u16,
        ];
        self.write_args(USDEF_I80_CMD_DPY_BUF_AREA, &args)
    }

    fn update_full(&mut self, info: &EpdSystemInfo, update_mode: u16) -> Result<(), BringupError> {
        self.check_afsr()?;

        let mut left = 0u16;
        let mut right = info.panel_width.saturating_sub(1);
        if (left & !3) == (right & !3) {
            if (left & 3) < (3 - (right & 3)) {
                left = (left & !3).saturating_sub(1);
            } else {
                right = (right + 4) & !3;
            }
        }
        let width = right - left + 1;
        let args = [
            left,
            0,
            width,
            info.panel_height,
            update_mode,
            (info.image_buffer_addr & 0xFFFF) as u16,
            ((info.image_buffer_addr >> 16) & 0xFFFF) as u16,
        ];
        self.write_args(USDEF_I80_CMD_DPY_BUF_AREA, &args)
    }

    fn write_reg(&mut self, reg: u16, value: u16) -> Result<(), BringupError> {
        self.write_command(IT8951_TCON_REG_WR)?;
        self.write_word(reg)?;
        self.write_word(value)
    }

    fn read_reg(&mut self, reg: u16) -> Result<u16, BringupError> {
        self.write_command(IT8951_TCON_REG_RD)?;
        self.write_word(reg)?;
        self.read_word()
    }

    fn write_args(&mut self, cmd: u16, args: &[u16]) -> Result<(), BringupError> {
        self.write_command(cmd)?;
        for arg in args {
            self.write_word(*arg)?;
        }
        Ok(())
    }

    fn write_command(&mut self, cmd: u16) -> Result<(), BringupError> {
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        self.cs.set_low().map_err(|_| BringupError::Gpio)?;
        self.write_u16_raw(0x6000)?;
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        self.write_u16_raw(cmd)?;
        self.cs.set_high().map_err(|_| BringupError::Gpio)?;
        Ok(())
    }

    fn write_word(&mut self, value: u16) -> Result<(), BringupError> {
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        self.cs.set_low().map_err(|_| BringupError::Gpio)?;
        self.write_u16_raw(0x0000)?;
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        self.write_u16_raw(value)?;
        self.cs.set_high().map_err(|_| BringupError::Gpio)?;
        Ok(())
    }

    fn read_word(&mut self) -> Result<u16, BringupError> {
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        self.cs.set_low().map_err(|_| BringupError::Gpio)?;
        self.write_u16_raw(0x1000)?;
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        let mut dummy = [0u8; 2];
        self.spi
            .transfer_in_place(&mut dummy)
            .map_err(|_| BringupError::Spi)?;
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        let mut word = [0u8; 2];
        self.spi
            .transfer_in_place(&mut word)
            .map_err(|_| BringupError::Spi)?;
        self.cs.set_high().map_err(|_| BringupError::Gpio)?;
        Ok(self.decode_u16(word))
    }

    fn read_words(&mut self, words: &mut [u16]) -> Result<(), BringupError> {
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        self.cs.set_low().map_err(|_| BringupError::Gpio)?;
        self.write_u16_raw(0x1000)?;
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        let mut dummy = [0u8; 2];
        self.spi
            .transfer_in_place(&mut dummy)
            .map_err(|_| BringupError::Spi)?;
        self.wait_ready(READY_TIMEOUT_LOOPS)?;
        for word in words.iter_mut() {
            let mut raw = [0u8; 2];
            self.spi
                .transfer_in_place(&mut raw)
                .map_err(|_| BringupError::Spi)?;
            *word = self.decode_u16(raw);
        }
        self.cs.set_high().map_err(|_| BringupError::Gpio)?;
        Ok(())
    }

    fn wait_for_display_idle(&mut self, timeout_loops: u32) -> Result<(), BringupError> {
        for _ in 0..timeout_loops {
            if self.read_reg(LUTAFSR)? == 0 {
                return Ok(());
            }
        }
        Err(BringupError::BusyTimeout)
    }

    fn check_afsr(&mut self) -> Result<(), BringupError> {
        for _ in 0..DISPLAY_TIMEOUT_LOOPS {
            let status = self.read_reg(LUTAFSR)?;
            if status == 0 {
                return Ok(());
            }
            if status == 0xFFFF {
                break;
            }
        }
        Err(BringupError::BusyTimeout)
    }

    fn wait_ready(&mut self, timeout_loops: u32) -> Result<(), BringupError> {
        for _ in 0..timeout_loops {
            if self.busy.is_high().map_err(|_| BringupError::Gpio)? {
                return Ok(());
            }
        }
        Err(BringupError::BusyTimeout)
    }

    fn write_u16_raw(&mut self, value: u16) -> Result<(), BringupError> {
        let bytes = match self.write_order {
            WordOrder::Be => value.to_be_bytes(),
            WordOrder::Le => value.to_le_bytes(),
        };
        self.spi.write(&bytes).map_err(|_| BringupError::Spi)
    }

    fn write_u32_raw(&mut self, hi: u16, lo: u16) -> Result<(), BringupError> {
        let hi = match self.write_order {
            WordOrder::Be => hi.to_be_bytes(),
            WordOrder::Le => hi.to_le_bytes(),
        };
        let lo = match self.write_order {
            WordOrder::Be => lo.to_be_bytes(),
            WordOrder::Le => lo.to_le_bytes(),
        };
        let bytes = [hi[0], hi[1], lo[0], lo[1]];
        self.spi.write(&bytes).map_err(|_| BringupError::Spi)
    }

    fn decode_u16(&self, bytes: [u8; 2]) -> u16 {
        match self.read_order {
            WordOrder::Be => u16::from_be_bytes(bytes),
            WordOrder::Le => u16::from_le_bytes(bytes),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum ProbeReadMode {
    Read,
    TransferInPlace,
    TransferZeros,
    TransferOnes,
}

pub fn scan_i2c_bus<I: I2c>(i2c: &mut I) -> Result<[bool; 128], BringupError> {
    let mut found = [false; 128];
    for addr in 0x08u8..0x78 {
        if i2c.write(addr, &[]).is_ok() {
            found[addr as usize] = true;
        }
    }
    Ok(found)
}

pub fn read_i2c_reg8<I: I2c>(i2c: &mut I, addr: u8, reg: u8) -> Result<u8, BringupError> {
    let mut buf = [0u8; 1];
    i2c.write_read(addr, &[reg], &mut buf)
        .map_err(|_| BringupError::I2c)?;
    Ok(buf[0])
}

pub fn read_i2c_reg16<I: I2c>(
    i2c: &mut I,
    addr: u8,
    reg: u16,
    out: &mut [u8],
) -> Result<(), BringupError> {
    let reg_buf = reg.to_be_bytes();
    i2c.write_read(addr, &reg_buf, out)
        .map_err(|_| BringupError::I2c)
}

pub fn read_sht30_status<I: I2c>(i2c: &mut I) -> Result<[u8; 3], BringupError> {
    let mut buf = [0u8; 3];
    i2c.write_read(0x44, &[0xF3, 0x2D], &mut buf)
        .map_err(|_| BringupError::I2c)?;
    Ok(buf)
}

fn decode_u16_c_string(words: &[u16]) -> String {
    let mut bytes = alloc::vec::Vec::with_capacity(words.len() * 2);
    for word in words {
        let pair = word.to_le_bytes();
        for b in pair {
            if b == 0 {
                return String::from_utf8_lossy(&bytes).into_owned();
            }
            bytes.push(b);
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn fallback_system_info(vcom_mv: u16) -> EpdSystemInfo {
    EpdSystemInfo {
        panel_width: M5PAPER_PANEL_WIDTH,
        panel_height: M5PAPER_PANEL_HEIGHT,
        image_buffer_addr: M5PAPER_IMAGE_BUFFER_ADDR,
        vcom_mv,
        used_fallback: true,
        fw_version: String::new(),
        lut_version: String::new(),
    }
}
