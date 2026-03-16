use embedded_graphics::{
    Drawable, Pixel,
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Point},
};

use crate::palm::runtime::PalmFont;

fn find_palm_font(fonts: &[PalmFont], font_id: u8) -> Option<&PalmFont> {
    fonts
        .iter()
        .find(|f| f.font_id == font_id as u16)
        .or_else(|| fonts.iter().find(|f| f.font_id == 0))
        .or_else(|| fonts.first())
}

pub fn palm_text_width(text: &str, font_id: u8, fonts: &[PalmFont], scale: i32) -> i32 {
    let Some(font) = find_palm_font(fonts, font_id) else {
        return (text.chars().count() as i32) * 6 * scale;
    };
    let mut w = 0i32;
    for ch in text.chars() {
        let code = ch as u32;
        if code < font.first_char as u32 || code > font.last_char as u32 {
            w += (font.avg_width.max(1) as i32) * scale;
            continue;
        }
        let idx = (code as u8 - font.first_char) as usize;
        if let Some(width) = font.widths.get(idx) {
            w += (width.max(1) as i32) * scale;
        } else {
            w += (font.avg_width.max(1) as i32) * scale;
        }
    }
    w
}

pub fn palm_text_width_scaled(
    text: &str,
    font_id: u8,
    fonts: &[PalmFont],
    scale_num: i32,
    scale_den: i32,
) -> i32 {
    let den = scale_den.max(1);
    let base = palm_text_width(text, font_id, fonts, 1);
    ((base * scale_num.max(1)) + den - 1) / den
}

pub fn palm_text_height(font_id: u8, fonts: &[PalmFont], scale: i32) -> i32 {
    if let Some(font) = find_palm_font(fonts, font_id) {
        (font.rect_height.max(1) as i32) * scale
    } else {
        10 * scale
    }
}

pub fn palm_text_height_scaled(
    font_id: u8,
    fonts: &[PalmFont],
    scale_num: i32,
    scale_den: i32,
) -> i32 {
    let den = scale_den.max(1);
    let base = palm_text_height(font_id, fonts, 1);
    ((base * scale_num.max(1)) + den - 1) / den
}

pub fn draw_palm_text<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    text: &str,
    x: i32,
    y: i32,
    font_id: u8,
    fonts: &[PalmFont],
    scale: i32,
    color: BinaryColor,
) {
    let Some(font) = find_palm_font(fonts, font_id) else {
        return;
    };
    let mut pen_x = x;
    for ch in text.chars() {
        let code = ch as u32;
        if code < font.first_char as u32 || code > font.last_char as u32 {
            pen_x += (font.avg_width.max(1) as i32) * scale;
            continue;
        }
        let idx = (code as u8 - font.first_char) as usize;
        let advance = font
            .widths
            .get(idx)
            .unwrap_or(font.avg_width)
            .max(1) as i32
            * scale;
        if let Some(glyph) = font.glyphs.get(idx) {
            for (ry, row_bits) in glyph.rows.iter().enumerate() {
                let draw_w = core::cmp::min(glyph.width as i32, 16);
                for rx in 0..draw_w {
                    let Some(mask) = (1u16).checked_shl(rx as u32) else {
                        continue;
                    };
                    if (row_bits & mask) == 0 {
                        continue;
                    }
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let _ = Pixel(
                                Point::new(pen_x + rx * scale + sx, y + (ry as i32) * scale + sy),
                                color,
                            )
                            .draw(target);
                        }
                    }
                }
            }
        }
        pen_x += advance;
    }
}

pub fn draw_palm_text_scaled<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    text: &str,
    x: i32,
    y: i32,
    font_id: u8,
    fonts: &[PalmFont],
    scale_num: i32,
    scale_den: i32,
    color: BinaryColor,
) {
    let den = scale_den.max(1);
    let num = scale_num.max(1);
    let Some(font) = find_palm_font(fonts, font_id) else {
        return;
    };
    let mut pen_x = x;
    for ch in text.chars() {
        let code = ch as u32;
        if code < font.first_char as u32 || code > font.last_char as u32 {
            let adv = font.avg_width.max(1) as i32;
            pen_x += ((adv * num) + den - 1) / den;
            continue;
        }
        let idx = (code as u8 - font.first_char) as usize;
        let glyph_adv = font.widths.get(idx).unwrap_or(font.avg_width).max(1) as i32;
        let advance = ((glyph_adv * num) + den - 1) / den;
        if let Some(glyph) = font.glyphs.get(idx) {
            for (ry, row_bits) in glyph.rows.iter().enumerate() {
                let draw_w = core::cmp::min(glyph.width as i32, 16);
                for rx in 0..draw_w {
                    let Some(mask) = (1u16).checked_shl(rx as u32) else {
                        continue;
                    };
                    if (row_bits & mask) == 0 {
                        continue;
                    }
                    let x0 = pen_x + ((rx * num) / den);
                    let x1 = pen_x + ((((rx + 1) * num) + den - 1) / den) - 1;
                    let y0 = y + (((ry as i32) * num) / den);
                    let y1 = y + ((((ry as i32 + 1) * num) + den - 1) / den) - 1;
                    for py in y0..=y1.max(y0) {
                        for px in x0..=x1.max(x0) {
                            let _ = Pixel(Point::new(px, py), color).draw(target);
                        }
                    }
                }
            }
        }
        pen_x += advance.max(1);
    }
}
