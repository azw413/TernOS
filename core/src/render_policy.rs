use crate::display::{GrayscaleMode, RefreshMode};
use crate::platform::DisplayCaps;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryDither {
    Threshold,
    Bayer4x4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderPolicy {
    pub gray_levels: u8,
    pub bits_per_pixel: u8,
    pub binary_dither: BinaryDither,
    pub absolute_grayscale_mode: GrayscaleMode,
    pub prefer_differential_grayscale: bool,
}

impl RenderPolicy {
    pub fn from_display_caps(caps: DisplayCaps) -> Self {
        let binary_dither = if caps.gray_levels >= 16 {
            BinaryDither::Threshold
        } else {
            BinaryDither::Bayer4x4
        };
        let absolute_grayscale_mode = if caps.gray_levels >= 16 {
            GrayscaleMode::Standard
        } else {
            GrayscaleMode::Fast
        };
        Self {
            gray_levels: caps.gray_levels,
            bits_per_pixel: caps.bits_per_pixel,
            binary_dither,
            absolute_grayscale_mode,
            prefer_differential_grayscale: caps.gray_levels < 16,
        }
    }

    pub fn binary_color_for_luma(&self, x: i32, y: i32, lum: u8) -> bool {
        match self.binary_dither {
            BinaryDither::Threshold => lum >= 128,
            BinaryDither::Bayer4x4 => {
                const BAYER_4X4: [[u8; 4]; 4] = [
                    [0, 8, 2, 10],
                    [12, 4, 14, 6],
                    [3, 11, 1, 9],
                    [15, 7, 13, 5],
                ];
                let threshold = (BAYER_4X4[(y as usize) & 3][(x as usize) & 3] * 16 + 8) as u8;
                lum >= threshold
            }
        }
    }

    pub fn partial_refresh_mode(&self) -> RefreshMode {
        if self.gray_levels >= 16 {
            RefreshMode::Half
        } else {
            RefreshMode::Fast
        }
    }

    pub fn refresh_mode(&self, full_refresh: bool) -> RefreshMode {
        if full_refresh {
            RefreshMode::Full
        } else {
            self.partial_refresh_mode()
        }
    }
}
