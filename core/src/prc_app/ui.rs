extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use embedded_graphics::{
    Drawable, Pixel,
    mono_font::{
        MonoTextStyle,
        ascii::{FONT_6X10, FONT_9X15},
    },
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, OriginDimensions, Point, Primitive, Size},
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};

use crate::prc_app::{
    bitmap::PrcBitmap,
    form_preview::{FormPreview, FormPreviewObject},
    menu_preview::MenuBarPreview,
    runner::{RuntimeBitmapDraw, RuntimeFieldDraw, RuntimeHelpDialog, RuntimeTableDraw},
    runtime::PalmFont,
};
use crate::ui::{prc_alert, prc_components};

fn find_font(fonts: &[PalmFont], font_id: u8) -> Option<&PalmFont> {
    fonts.iter().find(|f| f.font_id == font_id as u16)
}

fn fallback_text_style(font_id: u8, color: BinaryColor) -> MonoTextStyle<'static, BinaryColor> {
    if font_id == 2 {
        MonoTextStyle::new(&FONT_9X15, color)
    } else {
        MonoTextStyle::new(&FONT_6X10, color)
    }
}

fn draw_bitmap_text<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    text: &str,
    x: i32,
    y: i32,
    font: &PalmFont,
    scale: i32,
    color: BinaryColor,
) {
    let mut pen_x = x;
    let s = scale.max(1);
    for ch in text.bytes() {
        let w = if ch >= font.first_char && ch <= font.last_char {
            let idx = (ch - font.first_char) as usize;
            if let Some(glyph) = font.glyphs.get(idx) {
                draw_bitmap_glyph(target, pen_x, y, glyph.width, glyph.rows, s, color);
                glyph.width.max(1) as i32
            } else {
                font.widths.get(idx).unwrap_or(font.avg_width).max(1) as i32
            }
        } else {
            font.avg_width.max(1) as i32
        };
        pen_x += w * s;
    }
}

fn draw_bitmap_glyph<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    glyph_width: u8,
    glyph_rows: &[u16],
    scale: i32,
    color: BinaryColor,
) {
    for (ry, row_bits) in glyph_rows.iter().enumerate() {
        for rx in 0..(glyph_width as i32) {
            if rx >= 16 {
                break;
            }
            if (row_bits & (1u16 << rx)) != 0 {
                for sy in 0..scale {
                    for sx in 0..scale {
                        let _ = Pixel(
                            Point::new(x + rx * scale + sx, y + ry as i32 * scale + sy),
                            color,
                        )
                        .draw(target);
                    }
                }
            }
        }
    }
}

fn text_metrics(text: &str, font_id: u8, fonts: &[PalmFont], scale: i32) -> (i32, i32) {
    let s = scale.max(1);
    if let Some(font) = find_font(fonts, font_id) {
        let mut width = 0i32;
        for ch in text.bytes() {
            let w = if ch >= font.first_char && ch <= font.last_char {
                let idx = (ch - font.first_char) as usize;
                font.widths.get(idx).unwrap_or(font.avg_width).max(1) as i32
            } else {
                font.avg_width.max(1) as i32
            };
            width += w;
        }
        (width * s, font.rect_height.max(1) as i32 * s)
    } else if font_id == 2 {
        (text.len() as i32 * 9, 15)
    } else {
        (text.len() as i32 * 6, 10)
    }
}

fn draw_text<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    text: &str,
    x: i32,
    y: i32,
    font_id: u8,
    fonts: &[PalmFont],
    scale: i32,
    color: BinaryColor,
) {
    let can_use_bitmap_font = |font: &PalmFont, s: &str| -> bool {
        s.bytes().all(|ch| {
            if ch < font.first_char || ch > font.last_char {
                return false;
            }
            let idx = (ch - font.first_char) as usize;
            font.glyphs.get(idx).is_some()
        })
    };
    if let Some(font) = find_font(fonts, font_id).filter(|f| can_use_bitmap_font(f, text)) {
        draw_bitmap_text(target, text, x, y, font, scale, color);
    } else {
        let style = fallback_text_style(font_id, color);
        let _ = Text::new(text, Point::new(x, y + 10), style).draw(target);
    }
}

fn draw_button_outline<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    bx: i32,
    by: i32,
    bw: i32,
    bh: i32,
    style: u8,
    _outline: PrimitiveStyle<BinaryColor>,
) {
    if style == 1 || style == 5 {
        let _ = Rectangle::new(Point::new(bx, by), Size::new(bw.max(1) as u32, bh.max(1) as u32))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 1))
            .draw(target);
    } else {
        prc_components::draw_button_frame(target, bx, by, bw, bh, BinaryColor::Off);
    }
}

fn draw_dotted_hline<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x0: i32,
    x1: i32,
    y: i32,
) {
    let start = x0.min(x1);
    let end = x0.max(x1);
    let mut x = start;
    while x <= end {
        let _ = Pixel(Point::new(x, y), BinaryColor::Off).draw(target);
        x += 2;
    }
}

fn draw_dotted_vline<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y0: i32,
    y1: i32,
) {
    let start = y0.min(y1);
    let end = y0.max(y1);
    let mut y = start;
    while y <= end {
        let _ = Pixel(Point::new(x, y), BinaryColor::Off).draw(target);
        y += 2;
    }
}

struct MonoCanvas160 {
    px: [BinaryColor; 160 * 160],
}

impl MonoCanvas160 {
    fn new() -> Self {
        Self {
            px: [BinaryColor::On; 160 * 160],
        }
    }

    fn get(&self, x: i32, y: i32) -> Option<BinaryColor> {
        if x < 0 || y < 0 || x >= 160 || y >= 160 {
            return None;
        }
        Some(self.px[(y as usize * 160) + x as usize])
    }
}

impl OriginDimensions for MonoCanvas160 {
    fn size(&self) -> Size {
        Size::new(160, 160)
    }
}

impl DrawTarget for MonoCanvas160 {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            if point.x < 0 || point.y < 0 || point.x >= 160 || point.y >= 160 {
                continue;
            }
            self.px[(point.y as usize * 160) + point.x as usize] = color;
        }
        Ok(())
    }
}

fn blit_scaled<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    canvas: &MonoCanvas160,
    pane_x: i32,
    pane_y: i32,
    src_w: i32,
    src_h: i32,
    scale: i32,
    opaque_rect: Option<(i32, i32, i32, i32)>,
) {
    let s = scale.max(1);
    for y in 0..src_h.min(160) {
        for x in 0..src_w.min(160) {
            if let Some(color) = canvas.get(x, y) {
                if color == BinaryColor::On {
                    let opaque = opaque_rect
                        .map(|(rx, ry, rw, rh)| x >= rx && y >= ry && x < (rx + rw) && y < (ry + rh))
                        .unwrap_or(false);
                    if !opaque {
                        continue;
                    }
                    let _ = Rectangle::new(
                        Point::new(pane_x + x * s, pane_y + y * s),
                        Size::new(s as u32, s as u32),
                    )
                    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                    .draw(target);
                    continue;
                }
                let _ = Rectangle::new(
                    Point::new(pane_x + x * s, pane_y + y * s),
                    Size::new(s as u32, s as u32),
                )
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                .draw(target);
            }
        }
    }
}

fn find_bitmap(bitmaps: &[PrcBitmap], resource_id: u16) -> Option<&PrcBitmap> {
    bitmaps.iter().find(|b| b.resource_id == resource_id)
}

fn find_field_text<'a>(
    field_draws: &'a [RuntimeFieldDraw],
    form_id: u16,
    field_id: u16,
) -> Option<&'a str> {
    field_draws
        .iter()
        .find(|f| f.form_id == form_id && f.field_id == field_id)
        .map(|f| f.text.as_str())
}

fn find_table_draw<'a>(
    table_draws: &'a [RuntimeTableDraw],
    form_id: u16,
    table_id: u16,
) -> Option<&'a RuntimeTableDraw> {
    table_draws
        .iter()
        .find(|t| t.form_id == form_id && t.table_id == table_id)
}

fn draw_wrapped_text_in_rect<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font_id: u8,
    fonts: &[PalmFont],
    color: BinaryColor,
) {
    if w <= 2 || h <= 2 || text.is_empty() {
        return;
    }
    let line_h = text_metrics("Mg", font_id, fonts, 1).1.max(8);
    let max_lines = (h / line_h).max(1);
    let mut line = alloc::string::String::new();
    let mut line_idx = 0i32;
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            alloc::string::String::from(word)
        } else {
            let mut c = line.clone();
            c.push(' ');
            c.push_str(word);
            c
        };
        let (cw, _) = text_metrics(&candidate, font_id, fonts, 1);
        if cw <= (w - 2) {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            draw_text(target, &line, x + 1, y + 1 + line_idx * line_h, font_id, fonts, 1, color);
            line_idx += 1;
            if line_idx >= max_lines {
                return;
            }
            line.clear();
        }
        // Word itself doesn't fit: emit truncated line.
        let mut cur = alloc::string::String::new();
        for ch in word.chars() {
            let mut c = cur.clone();
            c.push(ch);
            if text_metrics(&c, font_id, fonts, 1).0 > (w - 2) {
                break;
            }
            cur = c;
        }
        if !cur.is_empty() {
            draw_text(target, &cur, x + 1, y + 1 + line_idx * line_h, font_id, fonts, 1, color);
            line_idx += 1;
            if line_idx >= max_lines {
                return;
            }
        }
    }
    if !line.is_empty() && line_idx < max_lines {
        draw_text(target, &line, x + 1, y + 1 + line_idx * line_h, font_id, fonts, 1, color);
    }
}

fn draw_prc_bitmap<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    bmp: &PrcBitmap,
    x0: i32,
    y0: i32,
) {
    let w = bmp.width as usize;
    let h = bmp.height as usize;
    let rb = bmp.row_bytes as usize;
    for y in 0..h {
        let row = y * rb;
        for x in 0..w {
            let byte = bmp.bits.get(row + (x / 8)).copied().unwrap_or(0);
            let mask = 0x80u8 >> (x & 7);
            let color = if (byte & mask) != 0 {
                BinaryColor::Off
            } else {
                BinaryColor::On
            };
            let _ = Pixel(Point::new(x0 + x as i32, y0 + y as i32), color).draw(target);
        }
    }
}

fn draw_palm_box(canvas: &mut MonoCanvas160, x: i32, y: i32, w: i32, h: i32, with_shadow: bool) {
    if w <= 4 || h <= 4 {
        return;
    }
    let _ = Rectangle::new(
        Point::new(x, y),
        Size::new(w.max(1) as u32, h.max(1) as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
    .draw(canvas);
    let _ = Rectangle::new(
        Point::new(x, y),
        Size::new(w.max(1) as u32, h.max(1) as u32),
    ).into_styled(PrimitiveStyle::with_fill(BinaryColor::On)).draw(canvas);

    // Rounded-corner look: draw frame with corner pixels omitted.
    for px in (x + 1)..=(x + w - 2) {
        let _ = Pixel(Point::new(px, y), BinaryColor::Off).draw(canvas);
        let _ = Pixel(Point::new(px, y + h - 1), BinaryColor::Off).draw(canvas);
    }
    for py in (y + 1)..=(y + h - 2) {
        let _ = Pixel(Point::new(x, py), BinaryColor::Off).draw(canvas);
        let _ = Pixel(Point::new(x + w - 1, py), BinaryColor::Off).draw(canvas);
    }

    if with_shadow {
        // Palm-like 1px drop shadow lines on right/bottom.
        let shadow_y = y + h;
        let shadow_x = x + w;
        for px in (x + 3)..=(x + w - 3) {
            let _ = Pixel(Point::new(px, shadow_y), BinaryColor::Off).draw(canvas);
        }
        for py in (y + 2)..=(y + h - 3) {
            let _ = Pixel(Point::new(shadow_x, py), BinaryColor::Off).draw(canvas);
        }
    }
}

fn draw_palm_pull_down_box(canvas: &mut MonoCanvas160, x: i32, y: i32, w: i32, h: i32) {
    if w <= 1 || h <= 1 {
        return;
    }
    let _ = Rectangle::new(
        Point::new(x, y),
        Size::new(w.max(1) as u32, h.max(1) as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
    .draw(canvas);

    // Top edge is square on Palm pull-downs.
    for px in x..=(x + w - 1) {
        let _ = Pixel(Point::new(px, y), BinaryColor::Off).draw(canvas);
    }
    // Bottom edge keeps rounded-corner look.
    for px in (x + 1)..=(x + w - 2) {
        let _ = Pixel(Point::new(px, y + h - 1), BinaryColor::Off).draw(canvas);
    }
    // Side edges connect at the menu-bar baseline.
    for py in y..=(y + h - 2) {
        let _ = Pixel(Point::new(x, py), BinaryColor::Off).draw(canvas);
        let _ = Pixel(Point::new(x + w - 1, py), BinaryColor::Off).draw(canvas);
    }

    // Palm-like drop shadow.
    let shadow_y = y + h;
    let shadow_x = x + w;
    for px in (x + 3)..=(x + w - 3) {
        let _ = Pixel(Point::new(px, shadow_y), BinaryColor::Off).draw(canvas);
    }
    for py in y..=(y + h - 3) {
        let _ = Pixel(Point::new(shadow_x, py), BinaryColor::Off).draw(canvas);
    }
}

fn draw_menu_overlay_on_canvas(
    canvas: &mut MonoCanvas160,
    menu: &MenuBarPreview,
    active_menu_index: usize,
    active_item_index: Option<usize>,
    fonts: &[PalmFont],
) {
    if menu.menus.is_empty() {
        return;
    }
    // Match Palm menu bar proportions more closely:
    // deeper bar with ~3px text-to-bottom gap.
    let top_h = 15i32;
    let menu_font = 1u8; // bold
    let _ = Rectangle::new(Point::new(0, 0), Size::new(160, top_h as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(canvas);
    // Menu bar box has a 1px gap from top/left/right edges.
    draw_palm_box(canvas, 1, 1, 157, top_h - 1, true);

    // First title highlight starts around x=6 on Palm.
    let mut x = 6i32;
    let mut active_title_bounds: Option<(i32, i32)> = None;
    for (idx, m) in menu.menus.iter().enumerate() {
        let (tw, _) = text_metrics(&m.title, menu_font, fonts, 1);
        // Title sits 3px inside highlight; highlight extends 3px after text.
        let pad = 3i32;
        let w = (tw + pad * 2).clamp(10, 70);
        if idx == active_menu_index {
            active_title_bounds = Some((x, w));
        }
        if idx == active_menu_index && active_item_index.is_none() {
            let _ = Rectangle::new(Point::new(x, 2), Size::new(w as u32, (top_h - 3) as u32))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                .draw(canvas);
            draw_text(canvas, &m.title, x + pad, 2, menu_font, fonts, 1, BinaryColor::On);
        } else {
            draw_text(canvas, &m.title, x + pad, 2, menu_font, fonts, 1, BinaryColor::Off);
        }
        x += w + 1;
        if x >= 156 {
            break;
        }
    }

    let menu_idx = active_menu_index.min(menu.menus.len().saturating_sub(1));
    let pull = &menu.menus[menu_idx];
    if pull.items.is_empty() {
        return;
    }
    let mut max_w = 56i32;
    for item in &pull.items {
        let (tw, _) = text_metrics(&item.text, menu_font, fonts, 1);
        max_w = max_w.max(tw + 28);
    }
    max_w = max_w.min(150);
    let row_h = 12i32;
    let h = (pull.items.len() as i32 * row_h + 2).min(150 - top_h);
    let preferred_x = active_title_bounds.map(|(x, _)| x).unwrap_or(0);
    let x0 = preferred_x.clamp(0, 159 - max_w);
    // Align one pixel lower.
    let y0 = top_h - 1;
    draw_palm_pull_down_box(canvas, x0, y0, max_w, h);
    for (idx, item) in pull.items.iter().enumerate() {
        let iy = y0 + 1 + (idx as i32 * row_h);
        if iy + row_h > y0 + h {
            break;
        }
        if Some(idx) == active_item_index {
            let _ = Rectangle::new(
                Point::new(x0 + 1, iy),
                Size::new((max_w - 2) as u32, row_h as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
            .draw(canvas);
            draw_text(
                canvas,
                &item.text,
                x0 + 3,
                iy + 1,
                menu_font,
                fonts,
                1,
                BinaryColor::On,
            );
            if let Some(ch) = item.shortcut {
                let sc = alloc::format!("/{}", ch);
                let (sw, _) = text_metrics(&sc, menu_font, fonts, 1);
                draw_text(
                    canvas,
                    &sc,
                    x0 + max_w - sw - 4,
                    iy + 1,
                    menu_font,
                    fonts,
                    1,
                    BinaryColor::On,
                );
            }
        } else {
            draw_text(
                canvas,
                &item.text,
                x0 + 3,
                iy + 1,
                menu_font,
                fonts,
                1,
                BinaryColor::Off,
            );
            if let Some(ch) = item.shortcut {
                let sc = alloc::format!("/{}", ch);
                let (sw, _) = text_metrics(&sc, menu_font, fonts, 1);
                draw_text(
                    canvas,
                    &sc,
                    x0 + max_w - sw - 4,
                    iy + 1,
                    menu_font,
                    fonts,
                    1,
                    BinaryColor::Off,
                );
            }
        }
    }
}

fn wrap_text_lines(text: &str, font_id: u8, max_w: i32, fonts: &[PalmFont]) -> alloc::vec::Vec<alloc::string::String> {
    let mut lines = alloc::vec::Vec::new();
    for para in text.split('\n') {
        let words: alloc::vec::Vec<&str> = para.split_whitespace().collect();
        if words.is_empty() {
            lines.push(alloc::string::String::new());
            continue;
        }
        let mut cur = alloc::string::String::new();
        for w in words {
            let candidate = if cur.is_empty() {
                alloc::format!("{}", w)
            } else {
                alloc::format!("{} {}", cur, w)
            };
            let (cw, _) = text_metrics(&candidate, font_id, fonts, 1);
            if cw <= max_w || cur.is_empty() {
                cur = candidate;
            } else {
                lines.push(cur);
                cur = alloc::format!("{}", w);
            }
        }
        lines.push(cur);
    }
    lines
}

fn draw_help_dialog_on_canvas(canvas: &mut MonoCanvas160, dialog: &RuntimeHelpDialog, fonts: &[PalmFont]) {
    let x = 1i32;
    let y = 1i32;
    let w = 158i32;
    let h = 158i32;
    let header_h = 14i32;
    prc_alert::draw_alert_frame(canvas, x, y, w, h, header_h);

    let title = "Tips";
    let (tw, _) = text_metrics(title, 1, fonts, 1);
    let tx = x + ((w - tw) / 2).max(0);
    draw_text(canvas, title, tx, y + 3, 1, fonts, 1, BinaryColor::On);

    let body_x = x + 6;
    let body_y = y + header_h + 5;
    let body_w = w - 14;
    let body_h = h - header_h - 26;
    let line_h = 12i32;
    let visible = (body_h / line_h).max(1) as usize;
    let lines = wrap_text_lines(&dialog.text, 1, body_w, fonts);
    let max_scroll = lines.len().saturating_sub(visible);
    let scroll = dialog.scroll_line.min(max_scroll);
    for row in 0..visible {
        let idx = scroll + row;
        let Some(line) = lines.get(idx) else { break };
        draw_text(
            canvas,
            line,
            body_x,
            body_y + row as i32 * line_h,
            1,
            fonts,
            1,
            BinaryColor::Off,
        );
    }

    let (done_tw, done_th) = text_metrics("Done", 1, fonts, 1);
    let btn_x = x + 8;
    let layout =
        prc_components::auto_button_layout_for_label(btn_x, 0, done_tw, done_th, 36, 10, 7, 2);
    let btn_y = y + h - layout.h - 4;
    prc_alert::draw_done_button(canvas, btn_x, btn_y, layout.w, layout.h);
    let done_tx = btn_x + ((layout.w - done_tw) / 2).max(1);
    let done_ty = btn_y + ((layout.h - done_th) / 2).max(1);
    draw_text(
        canvas,
        "Done",
        done_tx,
        done_ty,
        1,
        fonts,
        1,
        BinaryColor::Off,
    );

    prc_alert::draw_scroll_indicator(
        canvas,
        x + w - 11,
        y + h - 17,
        scroll > 0,
        scroll < max_scroll,
    );
}

pub fn draw_form_preview<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    form: &FormPreview,
    fonts: &[PalmFont],
    bitmaps: &[PrcBitmap],
    runtime_bitmap_draws: &[RuntimeBitmapDraw],
    runtime_field_draws: &[RuntimeFieldDraw],
    runtime_table_draws: &[RuntimeTableDraw],
    focused_control_id: Option<u16>,
    focused_field_id: Option<u16>,
    menu_overlay: Option<(&MenuBarPreview, usize, Option<usize>)>,
    help_overlay: Option<&RuntimeHelpDialog>,
    pane_x: i32,
    pane_y: i32,
    pane_w: i32,
    pane_h: i32,
    scale: i32,
    clear_pane: bool,
    outline: PrimitiveStyle<BinaryColor>,
) {
    if clear_pane {
        // Clear the whole preview pane so form changes don't leave stale pixels.
        let _ = Rectangle::new(
            Point::new(pane_x, pane_y),
            Size::new(pane_w.max(1) as u32, pane_h.max(1) as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target);
    }

    let mut canvas = MonoCanvas160::new();
    // Always render in Palm's native 160x160 space.
    // Some modal forms are smaller than full-screen; scaling them by their
    // own bounds makes them look like alert/help overlays instead of forms.
    let src_w = 160i32;
    let src_h = 160i32;

    // Most form object coordinates are form-relative in packed resources.
    // Detect that case and shift by form origin so popup forms land correctly
    // within the 160x160 screen.
    let mut max_right = 0i16;
    let mut max_bottom = 0i16;
    for obj in form.objects.iter() {
        match obj {
            FormPreviewObject::Title { x, y, .. } | FormPreviewObject::Label { x, y, .. } => {
                if *x > max_right {
                    max_right = *x;
                }
                if *y > max_bottom {
                    max_bottom = *y;
                }
            }
            FormPreviewObject::Button { x, y, w, h, .. }
            | FormPreviewObject::Field { x, y, w, h, .. }
            | FormPreviewObject::Table { x, y, w, h, .. } => {
                max_right = max_right.max(x.saturating_add(*w));
                max_bottom = max_bottom.max(y.saturating_add(*h));
            }
            FormPreviewObject::Bitmap { x, y, .. } => {
                if *x > max_right {
                    max_right = *x;
                }
                if *y > max_bottom {
                    max_bottom = *y;
                }
            }
        }
    }
    let coords_are_form_relative =
        (form.x != 0 || form.y != 0)
            && max_right <= form.w.saturating_add(4)
            && max_bottom <= form.h.saturating_add(4);
    let map_x = |x: i16| {
        if coords_are_form_relative {
            x.saturating_add(form.x).max(0) as i32
        } else {
            x.max(0) as i32
        }
    };
    let map_y = |y: i16| {
        if coords_are_form_relative {
            y.saturating_add(form.y).max(0) as i32
        } else {
            y.max(0) as i32
        }
    };

    // Modal/frame forms (FrmPopupForm dialogs) use Palm's alert/dialog chrome.
    let dialog_framed = form.frame_type != 0 || (form.window_flags & 0x2000) != 0;
    if dialog_framed {
        let fx = form.x.max(0) as i32;
        let fy = form.y.max(0) as i32;
        let fw = form.w.max(20).min(160) as i32;
        let fh = form.h.max(20).min(160) as i32;
        prc_components::draw_alert_frame(&mut canvas, fx, fy, fw, fh, 12);
    }

    // Snap adjacent push buttons (tPBN style=1) so neighboring buttons share
    // one divider line, matching Palm's adjoined segmented-button look.
    struct PushRow {
        y: i32,
        h: i32,
        items: Vec<(usize, i32, i32)>, // (object index, x, w)
    }
    let mut push_rows: Vec<PushRow> = Vec::new();
    for (obj_idx, obj) in form.objects.iter().take(48).enumerate() {
        let FormPreviewObject::Button {
            x, y, w, h, style, ..
        } = obj
        else {
            continue;
        };
        if *style != 1 {
            continue;
        }
        let bx = map_x(*x);
        let mut by = map_y(*y);
        let mut bw = (*w).max(8) as i32;
        let mut bh = (*h).max(8) as i32;
        if *style == 1 {
            // Push buttons: slightly wider/deeper than raw resource bounds to
            // match Palm's segmented day strip look.
            by -= 1;
            bw += 2;
            bh += 2;
        }
        if bw <= 0 || bh <= 0 {
            continue;
        }
        if let Some(row) = push_rows
            .iter_mut()
            .find(|r| (r.y - by).abs() <= 1 && (r.h - bh).abs() <= 2)
        {
            row.items.push((obj_idx, bx, bw));
        } else {
            push_rows.push(PushRow {
                y: by,
                h: bh,
                items: vec![(obj_idx, bx, bw)],
            });
        }
    }
    let mut push_button_x: Vec<(usize, i32)> = Vec::new();
    for row in push_rows.iter_mut() {
        row.items.sort_by_key(|(_, x, _)| *x);
        let mut prev_right: Option<i32> = None;
        for (obj_idx, x, w) in row.items.iter() {
            let mut adj_x = *x;
            if let Some(pr) = prev_right {
                // Share one divider line with previous push-button.
                adj_x = pr;
            }
            prev_right = Some(adj_x + *w - 1);
            push_button_x.push((*obj_idx, adj_x));
        }
    }

    if let Some(FormPreviewObject::Title { x, y, font, text }) = form
        .objects
        .iter()
        .find(|o| matches!(o, FormPreviewObject::Title { .. }))
    {
        if dialog_framed {
            let fx = form.x.max(0) as i32;
            let fy = form.y.max(0) as i32;
            let fw = form.w.max(20).min(160) as i32;
            let (tw, _) = text_metrics(text, *font, fonts, 1);
            let tx = fx + ((fw - tw) / 2).max(2);
            let ty = fy + 2;
            draw_text(&mut canvas, text, tx, ty, *font, fonts, 1, BinaryColor::On);
        } else {
            let tx = map_x(*x);
            let ty = map_y(*y);
            let (tw, th) = text_metrics(text, *font, fonts, 1);
            let pad_x = 4;
            let pad_top = 2;
            let pad_bottom = 0;
            let tab_w = (tw + pad_x * 2).clamp(24, src_w.max(24));
            let tab_h = (th + pad_top + pad_bottom).max(10);
            let layout = prc_components::draw_form_title_bar(
                &mut canvas,
                tx.clamp(0, src_w.saturating_sub(1)),
                ty.clamp(0, src_h.saturating_sub(1)),
                (src_w - tx).max(1),
                tab_w,
                tab_h,
                2,
            );
            draw_text(
                &mut canvas,
                text,
                layout.tab_x + pad_x,
                layout.tab_y + pad_top,
                *font,
                fonts,
                1,
                BinaryColor::On,
            );
        }
    }

    for (obj_idx, obj) in form.objects.iter().take(48).enumerate() {
        match obj {
            FormPreviewObject::Title { .. } => {}
            FormPreviewObject::Label { x, y, font, text } => {
                draw_text(
                    &mut canvas,
                    text,
                    map_x(*x),
                    map_y(*y),
                    *font,
                    fonts,
                    1,
                    BinaryColor::Off,
                );
            }
            FormPreviewObject::Button {
                id,
                x,
                y,
                w,
                h,
                font,
                style,
                no_frame,
                text,
            } => {
                let mut bx = map_x(*x);
                let mut by = map_y(*y);
                let mut bw = (*w).max(8) as i32;
                let mut bh = (*h).max(8) as i32;
                if *style == 1 {
                    by -= 1;
                    bw += 2;
                    bh += 2;
                }
                if bw <= 0 || bh <= 0 {
                    continue;
                }
                if *style == 1 {
                    if let Some((_, adj_x)) = push_button_x.iter().find(|(idx, _)| *idx == obj_idx) {
                        bx = *adj_x;
                    }
                }
                let focused = focused_control_id == Some(*id);
                if !*no_frame {
                    draw_button_outline(&mut canvas, bx, by, bw, bh, *style, outline);
                    if focused && bw > 4 && bh > 4 {
                        let _ = Rectangle::new(
                            Point::new(bx + 1, by + 1),
                            Size::new((bw - 2) as u32, (bh - 2) as u32),
                        )
                        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                        .draw(&mut canvas);
                    }
                } else if focused {
                    let _ = Rectangle::new(
                        Point::new(bx.max(0), by.max(0)),
                        Size::new(bw.max(1) as u32, bh.max(1) as u32),
                    )
                    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 1))
                    .draw(&mut canvas);
                }
                let (tw, th) = text_metrics(text, *font, fonts, 1);
                let mut tx = bx + ((bw - tw) / 2).max(1);
                if *style == 1 {
                    tx += 1;
                }
                let ty = by + ((bh - th) / 2).max(1);
                if *no_frame && focused {
                    let _ = Rectangle::new(
                        Point::new((tx - 1).max(0), (ty - 1).max(0)),
                        Size::new((tw + 2).max(1) as u32, (th + 2).max(1) as u32),
                    )
                    .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                    .draw(&mut canvas);
                }
                draw_text(
                    &mut canvas,
                    text,
                    tx,
                    ty,
                    *font,
                    fonts,
                    1,
                    if focused {
                        BinaryColor::On
                    } else {
                        BinaryColor::Off
                    },
                );
            }
            FormPreviewObject::Bitmap { x, y, resource_id } => {
                if let Some(bmp) = find_bitmap(bitmaps, *resource_id) {
                    draw_prc_bitmap(&mut canvas, bmp, map_x(*x), map_y(*y));
                }
            }
            FormPreviewObject::Table { id, x, y, w, h } => {
                let tx = map_x(*x);
                let ty = map_y(*y);
                let tw = (*w).max(8) as i32;
                let th = (*h).max(8) as i32;
                if tw <= 2 || th <= 2 {
                    continue;
                }
                let _ = Rectangle::new(
                    Point::new(tx, ty),
                    Size::new(tw as u32, th as u32),
                )
                .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 1))
                .draw(&mut canvas);

                let state = find_table_draw(runtime_table_draws, form.form_id, *id);
                let rows_decl = state.map(|s| s.rows as usize).unwrap_or(0);
                let cols_decl = state.map(|s| s.cols as usize).unwrap_or(0);
                let row_usable = state.map(|s| s.row_usable.as_slice()).unwrap_or(&[]);
                let col_usable = state.map(|s| s.col_usable.as_slice()).unwrap_or(&[]);
                let row_hints = state.map(|s| s.row_height.as_slice()).unwrap_or(&[]);
                let col_hints = state.map(|s| s.col_width.as_slice()).unwrap_or(&[]);
                let col_spacing = state.map(|s| s.col_spacing.as_slice()).unwrap_or(&[]);

                let mut visible_rows: alloc::vec::Vec<usize> = (0..rows_decl.max(1))
                    .filter(|r| row_usable.get(*r).copied().unwrap_or(true))
                    .collect();
                let mut visible_cols: alloc::vec::Vec<usize> = (0..cols_decl.max(1))
                    .filter(|c| col_usable.get(*c).copied().unwrap_or(true))
                    .collect();
                if visible_rows.is_empty() {
                    visible_rows.push(0);
                }
                if visible_cols.is_empty() {
                    visible_cols.push(0);
                }

                let inner_w = (tw - 2).max(1);
                let inner_h = (th - 2).max(1);

                let col_widths: alloc::vec::Vec<i32> = visible_cols
                    .iter()
                    .map(|idx| col_hints.get(*idx).copied().unwrap_or(28).max(1) as i32)
                    .collect();
                let row_heights: alloc::vec::Vec<i32> = visible_rows
                    .iter()
                    .map(|idx| row_hints.get(*idx).copied().unwrap_or(11).max(1) as i32)
                    .collect();
                let spacing_total: i32 = visible_cols
                    .iter()
                    .take(visible_cols.len().saturating_sub(1))
                    .map(|idx| col_spacing.get(*idx).copied().unwrap_or(0).max(0) as i32)
                    .sum();

                let natural_w: i32 = col_widths.iter().sum::<i32>().max(1) + spacing_total;
                let natural_h: i32 = row_heights.iter().sum::<i32>().max(1);
                let mut x_boundaries: alloc::vec::Vec<i32> = alloc::vec::Vec::new();
                let mut y_boundaries: alloc::vec::Vec<i32> = alloc::vec::Vec::new();

                let mut x_cursor = tx + 1;
                for (i, idx) in visible_cols.iter().enumerate() {
                    let w_px = (col_widths[i] * inner_w) / natural_w.max(1);
                    x_cursor += w_px.max(1);
                    if i + 1 < visible_cols.len() {
                        x_boundaries.push(x_cursor);
                        x_cursor += col_spacing.get(*idx).copied().unwrap_or(0).max(0) as i32;
                    }
                }
                let mut y_cursor = ty + 1;
                for (i, _) in visible_rows.iter().enumerate() {
                    let h_px = (row_heights[i] * inner_h) / natural_h.max(1);
                    y_cursor += h_px.max(1);
                    if i + 1 < visible_rows.len() {
                        y_boundaries.push(y_cursor);
                    }
                }

                for y in y_boundaries.iter().copied() {
                    draw_dotted_hline(&mut canvas, tx + 1, tx + tw - 2, y);
                }
                for x in x_boundaries.iter().copied() {
                    draw_dotted_vline(&mut canvas, x, ty + 1, ty + th - 2);
                }

                if let Some(s) = state {
                    if s.selected_row >= 0 && s.selected_col >= 0 {
                        let sel_r = s.selected_row as usize;
                        let sel_c = s.selected_col as usize;
                        if let (Some(vr), Some(vc)) = (
                            visible_rows.iter().position(|r| *r == sel_r),
                            visible_cols.iter().position(|c| *c == sel_c),
                        ) {
                            let mut left = tx + 1;
                            for b in x_boundaries.iter().take(vc) {
                                left = *b + 1;
                            }
                            let right = if vc < x_boundaries.len() {
                                x_boundaries[vc] - 1
                            } else {
                                tx + tw - 2
                            };
                            let mut top = ty + 1;
                            for b in y_boundaries.iter().take(vr) {
                                top = *b + 1;
                            }
                            let bottom = if vr < y_boundaries.len() {
                                y_boundaries[vr] - 1
                            } else {
                                ty + th - 2
                            };
                            if right > left && bottom > top {
                                let _ = Rectangle::new(
                                    Point::new(left, top),
                                    Size::new((right - left + 1) as u32, (bottom - top + 1) as u32),
                                )
                                .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 2))
                                .draw(&mut canvas);
                            }
                        }
                    }
                }
            }
            FormPreviewObject::Field { id, x, y, w, h, font } => {
                let fx = map_x(*x);
                let fy = map_y(*y);
                let fw = (*w).max(8) as i32;
                let fh = (*h).max(8) as i32;
                if focused_field_id == Some(*id) {
                    let border_w = fw.max(2) as u32;
                    let border_h = fh.max(2) as u32;
                    let _ = Rectangle::new(Point::new(fx, fy), Size::new(border_w, border_h))
                        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 1))
                        .draw(&mut canvas);
                }
                if let Some(text) = find_field_text(runtime_field_draws, form.form_id, *id) {
                    draw_wrapped_text_in_rect(
                        &mut canvas,
                        text,
                        fx + 2,
                        fy + 1,
                        (fw - 4).max(1),
                        (fh - 2).max(1),
                        *font,
                        fonts,
                        BinaryColor::Off,
                    );
                }
            }
        }
    }

    for draw in runtime_bitmap_draws.iter().take(64) {
        if let Some(bmp) = find_bitmap(bitmaps, draw.resource_id) {
            draw_prc_bitmap(&mut canvas, bmp, draw.x as i32, draw.y as i32);
        }
    }

    if let Some((menu, active_menu_index, active_item_index)) = menu_overlay {
        draw_menu_overlay_on_canvas(&mut canvas, menu, active_menu_index, active_item_index, fonts);
    }
    if let Some(dialog) = help_overlay {
        draw_help_dialog_on_canvas(&mut canvas, dialog, fonts);
    }

    let s = scale.max(1);
    let out_w = (src_w * s).min(pane_w.max(1));
    let out_h = (src_h * s).min(pane_h.max(1));
    let dst_x = pane_x + ((pane_w - out_w) / 2).max(0);
    let dst_y = pane_y + ((pane_h - out_h) / 2).max(0);
    let opaque_rect = if dialog_framed {
        Some((
            form.x.max(0) as i32,
            form.y.max(0) as i32,
            form.w.max(1).min(160) as i32,
            form.h.max(1).min(160) as i32,
        ))
    } else {
        None
    };
    blit_scaled(target, &canvas, dst_x, dst_y, src_w, src_h, s, opaque_rect);
}
