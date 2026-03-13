extern crate alloc;

use alloc::alloc::{alloc_zeroed, handle_alloc_error, Layout};
use alloc::boxed::Box;

use tern_core::display::{Display, GrayscaleMode, RefreshMode};
use tern_core::framebuffer::{BUFFER_SIZE, DisplayBuffers, HEIGHT as FB_HEIGHT, Rotation, WIDTH as FB_WIDTH};
use tern_core::ternos::ui::Rect;

use crate::ffi::{self, UpdateMode};

const DEVICE_W: usize = 540;
const DEVICE_H: usize = 960;
const BAND_H: usize = 96;
const BAND_FRAME_BYTES: usize = (DEVICE_W * BAND_H) / 2;

pub struct M5PaperDisplay {
    gray_lsb: Option<Box<[u8; BUFFER_SIZE]>>,
    gray_msb: Option<Box<[u8; BUFFER_SIZE]>>,
    band4: Box<[u8; BAND_FRAME_BYTES]>,
}

impl M5PaperDisplay {
    pub fn new() -> Self {
        Self {
            gray_lsb: None,
            gray_msb: None,
            band4: boxed_zeroed(),
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

    fn map_rect_to_device(rect: Rect) -> Option<(usize, usize, usize, usize)> {
        let mut x0 = ((rect.x.max(0) as usize) * DEVICE_W) / FB_HEIGHT;
        let mut y0 = ((rect.y.max(0) as usize) * DEVICE_H) / FB_WIDTH;
        let mut x1 = ((((rect.x + rect.w).max(0) as usize) * DEVICE_W) + FB_HEIGHT - 1) / FB_HEIGHT;
        let mut y1 = ((((rect.y + rect.h).max(0) as usize) * DEVICE_H) + FB_WIDTH - 1) / FB_WIDTH;
        x1 = x1.min(DEVICE_W);
        y1 = y1.min(DEVICE_H);
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        x0 &= !3usize;
        x1 = (x1 + 3) & !3usize;
        if x1 > DEVICE_W {
            x1 = DEVICE_W;
        }
        if x0 >= x1 {
            return None;
        }
        Some((x0, y0, x1 - x0, y1 - y0))
    }

    fn present_binary_region(&mut self, buffers: &DisplayBuffers, rect: Rect, mode: RefreshMode) {
        self.present_region(Some(buffers.get_active_buffer()), buffers.rotation(), None, rect, mode);
    }

    fn present_grayscale_region(&mut self, rect: Rect, mode: RefreshMode) {
        let gray_lsb_ptr = self.gray_lsb.as_deref().map(|b| b as *const [u8; BUFFER_SIZE]);
        let gray_msb_ptr = self.gray_msb.as_deref().map(|b| b as *const [u8; BUFFER_SIZE]);
        let gray = match (gray_lsb_ptr, gray_msb_ptr) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        };
        unsafe {
            let gray_refs = gray.map(|(a, b)| (&*a, &*b));
            Self::present_region_inner(&mut self.band4, None, Rotation::Rotate90, gray_refs, rect, mode);
        }
    }

    fn present_region(
        &mut self,
        active: Option<&[u8; BUFFER_SIZE]>,
        rotation: Rotation,
        gray: Option<(&[u8; BUFFER_SIZE], &[u8; BUFFER_SIZE])>,
        rect: Rect,
        mode: RefreshMode,
    ) {
        Self::present_region_inner(&mut self.band4, active, rotation, gray, rect, mode);
    }

    fn present_region_inner(
        band4: &mut [u8; BAND_FRAME_BYTES],
        active: Option<&[u8; BUFFER_SIZE]>,
        rotation: Rotation,
        gray: Option<(&[u8; BUFFER_SIZE], &[u8; BUFFER_SIZE])>,
        rect: Rect,
        mode: RefreshMode,
    ) {
        let Some((x0, y0, w, h)) = Self::map_rect_to_device(rect) else {
            return;
        };
        unsafe {
            ffi::ets_printf(
                b"m5paper: display region x=%u y=%u w=%u h=%u\n\0".as_ptr(),
                x0 as u32,
                y0 as u32,
                w as u32,
                h as u32,
            );
        }
        let row_bytes = w / 2;
        let mut band_y = y0;
        while band_y < y0 + h {
            let band_h = core::cmp::min(BAND_H, y0 + h - band_y);
            for local_y in 0..band_h {
                let out_y = band_y + local_y;
                for col in 0..row_bytes {
                    let out_x0 = x0 + col * 2;
                    let out_x1 = out_x0 + 1;
                    let src_x0 = (out_x0 * FB_HEIGHT) / DEVICE_W;
                    let src_x1 = (out_x1 * FB_HEIGHT) / DEVICE_W;
                    let src_y = (out_y * FB_WIDTH) / DEVICE_H;
                    let base0 = active
                        .map(|buf| Self::bit_at(buf, src_x0, src_y, rotation))
                        .unwrap_or(false);
                    let base1 = active
                        .map(|buf| Self::bit_at(buf, src_x1, src_y, rotation))
                        .unwrap_or(false);
                    let (lsb0, msb0, lsb1, msb1) = if let Some((gray_lsb, gray_msb)) = gray {
                        (
                            Self::bit_at(gray_lsb, src_x0, src_y, rotation),
                            Self::bit_at(gray_msb, src_x0, src_y, rotation),
                            Self::bit_at(gray_lsb, src_x1, src_y, rotation),
                            Self::bit_at(gray_msb, src_x1, src_y, rotation),
                        )
                    } else {
                        (false, false, false, false)
                    };
                    let hi = Self::nibble_for(base0, lsb0, msb0);
                    let lo = Self::nibble_for(base1, lsb1, msb1);
                    band4[local_y * row_bytes + col] = (hi << 4) | lo;
                }
            }
            let _ = ffi::epd_update_region(
                x0 as u16,
                band_y as u16,
                w as u16,
                band_h as u16,
                &band4[..row_bytes * band_h],
                if matches!(mode, RefreshMode::Fast | RefreshMode::Half) { UpdateMode::Fast } else { UpdateMode::Quality },
            );
            ffi::delay_ms(1);
            band_y += band_h;
        }
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
    fn display(&mut self, buffers: &mut DisplayBuffers, mode: RefreshMode) {
        self.present_binary_region(buffers, Rect::new(0, 0, FB_HEIGHT as i32, FB_WIDTH as i32), mode);
        buffers.swap_buffers();
    }

    fn display_region(&mut self, buffers: &mut DisplayBuffers, rect: Rect, mode: RefreshMode) {
        self.present_binary_region(buffers, rect, mode);
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
        self.present_grayscale_region(Rect::new(0, 0, FB_HEIGHT as i32, FB_WIDTH as i32), RefreshMode::Fast);
    }

    fn display_absolute_grayscale(&mut self, _mode: GrayscaleMode) {
        self.present_grayscale_region(Rect::new(0, 0, FB_HEIGHT as i32, FB_WIDTH as i32), RefreshMode::Fast);
    }
}
