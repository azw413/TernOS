use crate::framebuffer::{BUFFER_SIZE, DisplayBuffers};
use crate::ternos::ui::Rect;

pub const WIDTH: usize = 800;
pub const HEIGHT: usize = 480;

/// Refresh modes for the display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RefreshMode {
    /// Full refresh with complete waveform
    Full,
    /// Half refresh (1720ms) - balanced quality and speed
    Half,
    /// Fast refresh using custom LUT
    Fast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrayscaleMode {
    Standard,
    Fast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageOverlayKind {
    Old,
    New,
    Exposed,
    Presented,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageOverlayRect {
    pub kind: DamageOverlayKind,
    pub rect: Rect,
}

pub trait Display {
    fn display(&mut self, buffers: &mut DisplayBuffers, mode: RefreshMode);
    fn display_region(&mut self, buffers: &mut DisplayBuffers, rect: Rect, mode: RefreshMode) {
        let _ = rect;
        self.display(buffers, mode);
    }
    fn set_damage_overlay(&mut self, _overlay: &[DamageOverlayRect]) {}
    fn copy_to_lsb(&mut self, buffers: &[u8; BUFFER_SIZE]);
    fn copy_to_msb(&mut self, buffers: &[u8; BUFFER_SIZE]);
    fn copy_grayscale_buffers(&mut self, lsb: &[u8; BUFFER_SIZE], msb: &[u8; BUFFER_SIZE]);
    fn display_differential_grayscale(&mut self, turn_off_screen: bool);
    fn display_absolute_grayscale(&mut self, mode: GrayscaleMode);
}
