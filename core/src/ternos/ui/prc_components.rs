use embedded_graphics::{
    Drawable, Pixel,
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Point, Primitive, Size},
    primitives::{PrimitiveStyle, Rectangle},
};
use crate::palm::runtime::PalmFont;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiResourceKind {
    Alert, // Talt
    AppIcon, // tAIB / taif
    Bitmap, // tFBM / Tbmp / tbmf
    CommandButton, // tBTN / tgbn
    Checkbox, // tCBX
    Form, // tFRM
    Gadget, // tGDT
    ShiftIndicator, // tGSI
    Label, // tLBL
    List, // tLST
    MenuBar, // MBAR
    Menu, // MENU
    PopupTrigger, // tPUT
    PopupList, // tPUL
    PushButton, // tPBN / tgpb
    RepeatingButton, // tREP / tgrb
    ScrollBar, // tSCL
    SelectorTrigger, // tSLT
    Slider, // tsld / tslf
    Table, // tTBL
    Field, // tFLD
}

pub fn draw_button_frame<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: BinaryColor,
) {
    let o1 = 1;
    let o2 = 2;
    if w < 6 || h < 6 {
        let _ = Rectangle::new(Point::new(x, y), Size::new(w.max(1) as u32, h.max(1) as u32))
            .into_styled(PrimitiveStyle::with_stroke(color, 1))
            .draw(target);
        return;
    }
    let x0 = x;
    let y0 = y;
    let x1 = x + w - 1;
    let y1 = y + h - 1;

    let _ = Rectangle::new(Point::new(x0 + o2, y0), Size::new((w - o2 * 2) as u32, 1))
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(target);
    let _ = Rectangle::new(Point::new(x0 + o2, y1), Size::new((w - o2 * 2) as u32, 1))
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(target);
    let _ = Rectangle::new(Point::new(x0, y0 + o2), Size::new(1, (h - o2 * 2) as u32))
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(target);
    let _ = Rectangle::new(Point::new(x1, y0 + o2), Size::new(1, (h - o2 * 2) as u32))
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(target);

    for (px, py) in [
        (x0 + o1, y0 + o1),
        (x0 + o2, y0),
        (x0 + o1, y0),
        (x0, y0 + o1),
        (x0, y0 + o2),
        (x1 - o1, y0 + o1),
        (x1 - o2, y0),
        (x1 - o1, y0),
        (x1, y0 + o1),
        (x1, y0 + o2),
        (x0 + o1, y1 - o1),
        (x0 + o2, y1),
        (x0 + o1, y1),
        (x0, y1 - o1),
        (x0, y1 - o2),
        (x1 - o1, y1 - o1),
        (x1 - o2, y1),
        (x1 - o1, y1),
        (x1, y1 - o1),
        (x1, y1 - o2),
    ] {
        let _ = Pixel(Point::new(px, py), color).draw(target);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ButtonLayout {
    pub w: i32,
    pub h: i32,
    pub text_x: i32,
    pub text_y: i32,
}

pub fn auto_button_layout_for_label(
    x: i32,
    y: i32,
    text_w: i32,
    text_h: i32,
    min_w: i32,
    min_h: i32,
    pad_x: i32,
    pad_y: i32,
) -> ButtonLayout {
    let w = (text_w + pad_x * 2).max(min_w);
    let h = (text_h + pad_y * 2).max(min_h);
    let text_x = x + ((w - text_w) / 2).max(1);
    let text_y = y + ((h - text_h) / 2).max(1);
    ButtonLayout { w, h, text_x, text_y }
}

pub fn draw_alert_frame<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    header_h: i32,
) {
    if w <= 4 || h <= 4 {
        return;
    }
    let _ = Rectangle::new(Point::new(x, y), Size::new(w as u32, h as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target);

    for inset in 0..2 {
        let ix = x + inset;
        let iy = y + inset;
        let iw = w - inset * 2;
        let ih = h - inset * 2;
        let cut_h = if inset == 0 { 2 } else { 0 };
        let cut_v = if inset == 0 { 2 } else { 1 };
        for px in (ix + cut_h)..=(ix + iw - 1 - cut_h) {
            let _ = Pixel(Point::new(px, iy), BinaryColor::Off).draw(target);
            let _ = Pixel(Point::new(px, iy + ih - 1), BinaryColor::Off).draw(target);
        }
        for py in (iy + cut_v)..=(iy + ih - 1 - cut_v) {
            let _ = Pixel(Point::new(ix, py), BinaryColor::Off).draw(target);
            let _ = Pixel(Point::new(ix + iw - 1, py), BinaryColor::Off).draw(target);
        }
    }

    let _ = Rectangle::new(
        Point::new(x + 2, y + 2),
        Size::new((w - 4).max(1) as u32, header_h.max(1) as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
    .draw(target);
}

pub fn draw_scroll_indicator<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    up: bool,
    down: bool,
) {
    let draw_triangle = |target: &mut T, ox: i32, oy: i32, up_dir: bool| {
        if up_dir {
            for row in 0..5 {
                for col in -row..=row {
                    let _ = Pixel(Point::new(ox + col, oy + row), BinaryColor::Off).draw(target);
                }
            }
        } else {
            for row in 0..5 {
                for col in -row..=row {
                    let _ = Pixel(Point::new(ox + col, oy - row), BinaryColor::Off).draw(target);
                }
            }
        }
    };
    if up {
        draw_triangle(target, x, y, true);
    }
    if down {
        draw_triangle(target, x, y + 10, false);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FormTitleLayout {
    pub tab_x: i32,
    pub tab_y: i32,
    pub tab_w: i32,
    pub tab_h: i32,
    pub line_y: i32,
}

/// Draw a Palm-style form title tab with rounded top corners and a thick
/// horizontal rule beneath it. Returns geometry for text placement.
pub fn draw_form_title_bar<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    tab_w: i32,
    tab_h: i32,
    line_thickness: i32,
) -> FormTitleLayout {
    let tab_w = tab_w.max(1).min(w.max(1));
    let tab_h = tab_h.max(1);
    let line_thickness = line_thickness.max(1);
    let tab_x = x;
    let tab_y = y;
    let _ = Rectangle::new(Point::new(tab_x, tab_y), Size::new(tab_w as u32, tab_h as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(target);
    // Rounded top corners: trim a bit more than a single corner pixel.
    let _ = Pixel(Point::new(tab_x, tab_y), BinaryColor::On).draw(target);
    let _ = Pixel(Point::new(tab_x + tab_w - 1, tab_y), BinaryColor::On).draw(target);
    if tab_w >= 4 {
        let _ = Pixel(Point::new(tab_x + 1, tab_y), BinaryColor::On).draw(target);
        let _ = Pixel(Point::new(tab_x + tab_w - 2, tab_y), BinaryColor::On).draw(target);
    }
    if tab_h >= 2 {
        let _ = Pixel(Point::new(tab_x, tab_y + 1), BinaryColor::On).draw(target);
        let _ = Pixel(Point::new(tab_x + tab_w - 1, tab_y + 1), BinaryColor::On).draw(target);
    }

    let line_y = tab_y + tab_h;
    for i in 0..line_thickness {
        let _ = Rectangle::new(
            Point::new(x, line_y + i),
            Size::new(w.max(1) as u32, 1),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(target);
    }

    FormTitleLayout {
        tab_x,
        tab_y,
        tab_w,
        tab_h,
        line_y,
    }
}

/// Palm-style pull-down menu box used by MBAR menus:
/// square top edge, rounded-ish bottom, and 1px right/bottom shadow.
pub fn draw_palm_pull_down_box<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    if w <= 1 || h <= 1 {
        return;
    }
    let _ = Rectangle::new(
        Point::new(x, y),
        Size::new(w.max(1) as u32, h.max(1) as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
    .draw(target);

    for px in x..=(x + w - 1) {
        let _ = Pixel(Point::new(px, y), BinaryColor::Off).draw(target);
    }
    for px in (x + 1)..=(x + w - 2) {
        let _ = Pixel(Point::new(px, y + h - 1), BinaryColor::Off).draw(target);
    }
    for py in y..=(y + h - 2) {
        let _ = Pixel(Point::new(x, py), BinaryColor::Off).draw(target);
        let _ = Pixel(Point::new(x + w - 1, py), BinaryColor::Off).draw(target);
    }

    let shadow_y = y + h;
    let shadow_x = x + w;
    for px in (x + 3)..=(x + w - 3) {
        let _ = Pixel(Point::new(px, shadow_y), BinaryColor::Off).draw(target);
    }
    for py in y..=(y + h - 3) {
        let _ = Pixel(Point::new(shadow_x, py), BinaryColor::Off).draw(target);
    }
}

fn find_palm_font(fonts: &[PalmFont], font_id: u8) -> Option<&PalmFont> {
    fonts.iter().find(|f| f.font_id == font_id as u16)
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
                // Palm glyph rows here are packed into u16; guard wider glyph
                // metadata to avoid invalid shifts.
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

#[allow(dead_code)]
pub fn draw_component_placeholder<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: BinaryColor,
) {
    let _ = Rectangle::new(Point::new(x, y), Size::new(w.max(1) as u32, h.max(1) as u32))
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(target);
}
