extern crate alloc;

use alloc::alloc::{alloc_zeroed, handle_alloc_error, Layout};
use alloc::boxed::Box;

use tern_core::display::{Display, GrayscaleMode, RefreshMode};
use tern_core::framebuffer::{BUFFER_SIZE, DisplayBuffers, HEIGHT as FB_HEIGHT, Rotation, WIDTH as FB_WIDTH};

use crate::ffi;

const DEVICE_W: usize = 540;
const DEVICE_H: usize = 960;
const VIEW_W: usize = FB_HEIGHT / 2;
const VIEW_H: usize = FB_WIDTH / 2;
const OFFSET_X: u16 = ((DEVICE_W - VIEW_W) / 2) as u16;
const OFFSET_Y: u16 = ((DEVICE_H - VIEW_H) / 2) as u16;
const FULL_FRAME_BYTES: usize = (VIEW_W * VIEW_H) / 2;

pub struct M5PaperDisplay {
    gray_lsb: Option<Box<[u8; BUFFER_SIZE]>>,
    gray_msb: Option<Box<[u8; BUFFER_SIZE]>>,
    frame4: Box<[u8; FULL_FRAME_BYTES]>,
}

impl M5PaperDisplay {
    pub fn new() -> Self {
        Self {
            gray_lsb: None,
            gray_msb: None,
            frame4: boxed_zeroed(),
        }
    }

    fn ensure_gray_buffers(&mut self) {
        if self.gray_lsb.is_none() {
            self.gray_lsb = Some(boxed_zeroed());
        }
        if self.gray_msb.is_none() {
            self.gray_msb = Some(boxed_zeroed());
        }
    }

    fn bit_at(buf: &[u8; BUFFER_SIZE], vx: usize, vy: usize, rotation: Rotation) -> bool {
        let (fx, fy) = match rotation {
            Rotation::Rotate0 => (vx, vy),
            Rotation::Rotate90 => (vy, FB_HEIGHT - 1 - vx),
            Rotation::Rotate180 => (FB_WIDTH - 1 - vx, FB_HEIGHT - 1 - vy),
            Rotation::Rotate270 => (FB_WIDTH - 1 - vy, vx),
        };
        let idx = fy * FB_WIDTH + fx;
        let byte = idx / 8;
        let bit = 7 - (idx % 8);
        (buf[byte] & (1 << bit)) != 0
    }

    fn nibble_for(base: bool, lsb: bool, msb: bool) -> u8 {
        if base || (lsb && msb) {
            0x0
        } else if msb {
            0x5
        } else if lsb {
            0xA
        } else {
            0xF
        }
    }

    fn present_binary_region(&mut self, buffers: &DisplayBuffers, rect: tern_core::ui::Rect) {
        self.present_region(Some(buffers.get_active_buffer()), buffers.rotation(), None, rect);
    }

    fn present_grayscale_region(&mut self, rect: tern_core::ui::Rect) {
        let gray_lsb = self.gray_lsb.as_deref();
        let gray_msb = self.gray_msb.as_deref();
        let gray = gray_lsb.zip(gray_msb);
        Self::present_region_inner(&mut self.frame4, None, Rotation::Rotate90, gray, rect);
    }

    fn present_region(
        &mut self,
        active: Option<&[u8; BUFFER_SIZE]>,
        rotation: Rotation,
        gray: Option<(&[u8; BUFFER_SIZE], &[u8; BUFFER_SIZE])>,
        rect: tern_core::ui::Rect,
    ) {
        Self::present_region_inner(&mut self.frame4, active, rotation, gray, rect);
    }

    fn present_region_inner(
        frame4: &mut [u8; FULL_FRAME_BYTES],
        active: Option<&[u8; BUFFER_SIZE]>,
        rotation: Rotation,
        gray: Option<(&[u8; BUFFER_SIZE], &[u8; BUFFER_SIZE])>,
        rect: tern_core::ui::Rect,
    ) {
        let mut x0 = (rect.x.max(0) / 2) as usize;
        let mut y0 = (rect.y.max(0) / 2) as usize;
        let mut x1 = ((rect.x + rect.w).max(0) + 1) / 2;
        let mut y1 = ((rect.y + rect.h).max(0) + 1) / 2;
        x1 = x1.min(VIEW_W as i32);
        y1 = y1.min(VIEW_H as i32);
        if x0 >= x1 as usize || y0 >= y1 as usize {
            return;
        }
        x0 &= !3usize;
        let mut w = x1 as usize - x0;
        w = (w + 3) & !3usize;
        if x0 + w > VIEW_W { w = VIEW_W - x0; }
        y0 = y0.min(VIEW_H);
        let h = (y1 as usize).saturating_sub(y0);
        if w == 0 || h == 0 { return; }
        let row_bytes = w / 2;
        for local_y in 0..h {
            let vy = y0 + local_y;
            for col in 0..row_bytes {
                let vx0 = x0 + col * 2;
                let vx1 = vx0 + 1;
                let sx0 = vx0 * 2;
                let sx1 = vx1 * 2;
                let sy = vy * 2;
                let base0 = active.map(|buf| Self::bit_at(buf, sx0, sy, rotation)).unwrap_or(false);
                let base1 = active.map(|buf| Self::bit_at(buf, sx1, sy, rotation)).unwrap_or(false);
                let (lsb0, msb0, lsb1, msb1) = if let Some((gray_lsb, gray_msb)) = gray {
                    (
                        Self::bit_at(gray_lsb, sx0, sy, rotation),
                        Self::bit_at(gray_msb, sx0, sy, rotation),
                        Self::bit_at(gray_lsb, sx1, sy, rotation),
                        Self::bit_at(gray_msb, sx1, sy, rotation),
                    )
                } else {
                    (false, false, false, false)
                };
                let hi = Self::nibble_for(base0, lsb0, msb0);
                let lo = Self::nibble_for(base1, lsb1, msb1);
                frame4[local_y * row_bytes + col] = (hi << 4) | lo;
            }
        }
        unsafe {
            ffi::ets_printf(b"m5paper-rust: display region x=%u y=%u w=%u h=%u\n\0".as_ptr(), x0 as u32, y0 as u32, w as u32, h as u32);
        }
        let _ = ffi::epd_update_region(OFFSET_X + x0 as u16, OFFSET_Y + y0 as u16, w as u16, h as u16, &frame4[..row_bytes * h]);
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

impl Default for M5PaperDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for M5PaperDisplay {
    fn display(&mut self, buffers: &mut DisplayBuffers, _mode: RefreshMode) {
        self.present_binary_region(buffers, tern_core::ui::Rect::new(0, 0, FB_HEIGHT as i32, FB_WIDTH as i32));
        buffers.swap_buffers();
    }

    fn display_region(&mut self, buffers: &mut DisplayBuffers, rect: tern_core::ui::Rect, _mode: RefreshMode) {
        self.present_binary_region(buffers, rect);
        buffers.swap_buffers();
    }

    fn copy_to_lsb(&mut self, buffers: &[u8; BUFFER_SIZE]) {
        self.ensure_gray_buffers();
        self.gray_lsb.as_mut().unwrap().copy_from_slice(buffers);
    }

    fn copy_to_msb(&mut self, buffers: &[u8; BUFFER_SIZE]) {
        self.ensure_gray_buffers();
        self.gray_msb.as_mut().unwrap().copy_from_slice(buffers);
    }

    fn copy_grayscale_buffers(&mut self, lsb: &[u8; BUFFER_SIZE], msb: &[u8; BUFFER_SIZE]) {
        self.ensure_gray_buffers();
        self.gray_lsb.as_mut().unwrap().copy_from_slice(lsb);
        self.gray_msb.as_mut().unwrap().copy_from_slice(msb);
    }

    fn display_differential_grayscale(&mut self, _turn_off_screen: bool) {
        self.present_grayscale_region(tern_core::ui::Rect::new(0, 0, FB_HEIGHT as i32, FB_WIDTH as i32));
    }

    fn display_absolute_grayscale(&mut self, _mode: GrayscaleMode) {
        self.present_grayscale_region(tern_core::ui::Rect::new(0, 0, FB_HEIGHT as i32, FB_WIDTH as i32));
    }
}
