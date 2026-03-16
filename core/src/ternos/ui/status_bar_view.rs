extern crate alloc;

use alloc::format;

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::{Point, Primitive, Size},
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
    Drawable,
};

use crate::framebuffer::{Rotation, BUFFER_SIZE, HEIGHT as FB_HEIGHT, WIDTH as FB_WIDTH};

use super::{
    prc_components::{draw_palm_text, palm_text_height, palm_text_width},
    view::{RenderQueue, UiContext, View},
    Rect,
};

mod generated_icons {
    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StatusBarActionState {
    pub enabled: bool,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusBarHit {
    Home,
    Menu,
}

pub struct StatusBarView<'a> {
    pub battery_percent: Option<u8>,
    pub right_text: Option<&'a str>,
    pub home: StatusBarActionState,
    pub menu: StatusBarActionState,
    pub palm_fonts: &'a [crate::palm::runtime::PalmFont],
}

impl<'a> StatusBarView<'a> {
    pub const HEIGHT: i32 = 44;
    const PADDING_X: i32 = 8;
    const ACTION_SIZE: i32 = 30;
    const ACTION_GAP: i32 = 6;

    pub fn new(palm_fonts: &'a [crate::palm::runtime::PalmFont]) -> Self {
        Self {
            battery_percent: None,
            right_text: None,
            home: StatusBarActionState::default(),
            menu: StatusBarActionState::default(),
            palm_fonts,
        }
    }

    pub fn home_rect(rect: Rect) -> Rect {
        Rect::new(
            rect.x + Self::PADDING_X,
            rect.y + ((rect.h - Self::ACTION_SIZE) / 2),
            Self::ACTION_SIZE,
            Self::ACTION_SIZE,
        )
    }

    pub fn menu_rect(rect: Rect) -> Rect {
        let home = Self::home_rect(rect);
        Rect::new(
            home.x + home.w + Self::ACTION_GAP,
            home.y,
            Self::ACTION_SIZE,
            Self::ACTION_SIZE,
        )
    }

    pub fn hit_test(rect: Rect, point: super::Point) -> Option<StatusBarHit> {
        let home = Self::home_rect(rect);
        if point.x >= home.x
            && point.x < home.x + home.w
            && point.y >= home.y
            && point.y < home.y + home.h
        {
            return Some(StatusBarHit::Home);
        }
        let menu = Self::menu_rect(rect);
        if point.x >= menu.x
            && point.x < menu.x + menu.w
            && point.y >= menu.y
            && point.y < menu.y + menu.h
        {
            return Some(StatusBarHit::Menu);
        }
        None
    }

    fn right_text_rect(rect: Rect) -> Rect {
        Rect::new(rect.x + rect.w - 144, rect.y + 6, 136, rect.h - 12)
    }

    fn battery_rect(rect: Rect) -> Rect {
        Rect::new(rect.x + (rect.w - 110) / 2, rect.y + 9, 110, 28)
    }

    fn use_true_gray(ctx: &UiContext<'_>) -> bool {
        ctx.gray2.is_some()
            && ctx
                .gray2
                .as_ref()
                .map(|gray2| gray2.lsb.len() >= BUFFER_SIZE && gray2.msb.len() >= BUFFER_SIZE)
                .unwrap_or(false)
            && !(ctx.render_policy.gray_levels == 4 && ctx.render_policy.bits_per_pixel == 2)
    }

    fn fill_bar(ctx: &mut UiContext<'_>, rect: Rect) {
        if Self::use_true_gray(ctx) {
            if let Some(gray2) = ctx.gray2.as_mut() {
                *gray2.used = true;
                for yy in rect.y..(rect.y + rect.h) {
                    for xx in rect.x..(rect.x + rect.w) {
                        ctx.buffers.set_pixel(xx, yy, BinaryColor::On);
                        let Some((fx, fy)) = map_display_point(ctx.buffers.rotation(), xx, yy) else {
                            continue;
                        };
                        let idx = fy * FB_WIDTH + fx;
                        let byte = idx / 8;
                        let bit = 7 - (idx % 8);
                        gray2.lsb[byte] |= 1 << bit;
                        gray2.msb[byte] &= !(1 << bit);
                    }
                }
            }
        } else {
            Self::fill_dither_bar(ctx, rect);
        }
    }

    fn fill_dither_bar(ctx: &mut UiContext<'_>, rect: Rect) {
        for yy in rect.y..(rect.y + rect.h) {
            for xx in rect.x..(rect.x + rect.w) {
                let color = if ((xx + yy) & 1) == 0 {
                    BinaryColor::On
                } else {
                    BinaryColor::Off
                };
                ctx.buffers.set_pixel(xx, yy, color);
            }
        }
        Rectangle::new(
            Point::new(rect.x, rect.y + rect.h - 1),
            Size::new(rect.w.max(1) as u32, 1),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(ctx.buffers)
        .ok();
    }

    fn draw_action_background(ctx: &mut UiContext<'_>, rect: Rect, state: StatusBarActionState) {
        let mut style = PrimitiveStyle::with_stroke(BinaryColor::Off, 1);
        if state.focused && state.enabled {
            style = PrimitiveStyle::with_fill(BinaryColor::Off);
        }
        Rectangle::new(
            Point::new(rect.x, rect.y),
            Size::new(rect.w.max(1) as u32, rect.h.max(1) as u32),
        )
        .into_styled(style)
        .draw(ctx.buffers)
        .ok();
    }

    fn draw_mask_icon(
        ctx: &mut UiContext<'_>,
        rect: Rect,
        dark_mask: &[u8],
        light_mask: &[u8],
        enabled: bool,
        inverted: bool,
    ) {
        let width = generated_icons::STATUS_ICON_SIZE as i32;
        let height = generated_icons::STATUS_ICON_SIZE as i32;
        let expected = (width as usize * height as usize).div_ceil(8);
        if dark_mask.len() != expected || light_mask.len() != expected {
            return;
        }
        let origin_x = rect.x + ((rect.w - width) / 2);
        let origin_y = rect.y + ((rect.h - height) / 2);
        for yy in 0..height {
            for xx in 0..width {
                let idx = (yy * width + xx) as usize;
                let byte = idx / 8;
                let bit = 7 - (idx % 8);
                let dark = (dark_mask[byte] >> bit) & 1 == 1;
                let light = (light_mask[byte] >> bit) & 1 == 1;
                if !dark && !light {
                    continue;
                }
                let mut color = if light { BinaryColor::On } else { BinaryColor::Off };
                if !enabled {
                    color = if ((origin_x + xx + origin_y + yy) & 1) == 0 {
                        BinaryColor::On
                    } else {
                        BinaryColor::Off
                    };
                }
                if inverted {
                    color = match color {
                        BinaryColor::On => BinaryColor::Off,
                        BinaryColor::Off => BinaryColor::On,
                    };
                }
                ctx.buffers.set_pixel(origin_x + xx, origin_y + yy, color);
            }
        }
    }

    fn draw_battery(&self, ctx: &mut UiContext<'_>, rect: Rect) {
        let battery = self.battery_percent.unwrap_or(100);
        let text = format!("{}%", battery);
        let batt_rect = Self::battery_rect(rect);
        let batt_w = 102;
        let batt_h = 24;
        let cap_w = 6;
        let cap_h = 12;
        let batt_total_w = batt_w + cap_w;
        let batt_x = batt_rect.x + (batt_rect.w - batt_total_w) / 2;
        let batt_y = batt_rect.y + (batt_rect.h - batt_h) / 2;
        Rectangle::new(
            Point::new(batt_x, batt_y),
            Size::new(batt_w as u32, batt_h as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(ctx.buffers)
        .ok();
        Rectangle::new(
            Point::new(batt_x + batt_w, batt_y + (batt_h - cap_h) / 2),
            Size::new(cap_w as u32, cap_h as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(ctx.buffers)
        .ok();
        if !self.palm_fonts.is_empty() {
            let tw = palm_text_width(&text, 0, self.palm_fonts, 1);
            let th = palm_text_height(0, self.palm_fonts, 1);
            draw_palm_text(
                ctx.buffers,
                &text,
                batt_x + (batt_w - tw) / 2,
                batt_y + (batt_h - th) / 2,
                0,
                self.palm_fonts,
                1,
                BinaryColor::On,
            );
        } else {
            let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
            let tx = batt_x + (batt_w - (text.len() as i32 * 6)) / 2;
            let ty = batt_y + (batt_h + 6) / 2;
            Text::new(&text, Point::new(tx, ty), style)
                .draw(ctx.buffers)
                .ok();
        }
    }

    fn draw_right_text(&self, ctx: &mut UiContext<'_>, rect: Rect) {
        let Some(text) = self.right_text else {
            return;
        };
        let right_rect = Self::right_text_rect(rect);
        if !self.palm_fonts.is_empty() {
            let tw = palm_text_width(text, 0, self.palm_fonts, 1);
            let th = palm_text_height(0, self.palm_fonts, 1);
            draw_palm_text(
                ctx.buffers,
                text,
                right_rect.x + right_rect.w - tw,
                right_rect.y + (right_rect.h - th) / 2,
                0,
                self.palm_fonts,
                1,
                BinaryColor::Off,
            );
        } else {
            let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
            let tx = right_rect.x + right_rect.w - (text.len() as i32 * 6);
            let ty = right_rect.y + 13;
            Text::new(text, Point::new(tx, ty), style)
                .draw(ctx.buffers)
                .ok();
        }
    }
}

impl View for StatusBarView<'_> {
    fn render(&mut self, ctx: &mut UiContext<'_>, rect: Rect, _rq: &mut RenderQueue) {
        Self::fill_bar(ctx, rect);

        let home_rect = Self::home_rect(rect);
        let menu_rect = Self::menu_rect(rect);
        Self::draw_action_background(ctx, home_rect, self.home);
        Self::draw_action_background(ctx, menu_rect, self.menu);
        Self::draw_mask_icon(
            ctx,
            home_rect,
            generated_icons::ICON_HOME_DARK_MASK,
            generated_icons::ICON_HOME_LIGHT_MASK,
            self.home.enabled,
            self.home.focused && self.home.enabled,
        );
        Self::draw_mask_icon(
            ctx,
            menu_rect,
            generated_icons::ICON_MENU_DARK_MASK,
            generated_icons::ICON_MENU_LIGHT_MASK,
            self.menu.enabled,
            self.menu.focused && self.menu.enabled,
        );
        self.draw_battery(ctx, rect);
        self.draw_right_text(ctx, rect);
    }
}

fn map_display_point(rotation: Rotation, x: i32, y: i32) -> Option<(usize, usize)> {
    if x < 0 || y < 0 {
        return None;
    }
    let (x, y) = match rotation {
        Rotation::Rotate0 => (x as usize, y as usize),
        Rotation::Rotate90 => (y as usize, FB_HEIGHT - 1 - x as usize),
        Rotation::Rotate180 => (FB_WIDTH - 1 - x as usize, FB_HEIGHT - 1 - y as usize),
        Rotation::Rotate270 => (FB_WIDTH - 1 - y as usize, x as usize),
    };
    if x >= FB_WIDTH || y >= FB_HEIGHT {
        None
    } else {
        Some((x, y))
    }
}
