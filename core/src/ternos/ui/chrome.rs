use embedded_graphics::{
    Drawable, Pixel,
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Point, Primitive, Size},
    primitives::{PrimitiveStyle, Rectangle},
};

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
    let header_h = header_h.clamp(12, h.saturating_sub(4));
    let divider_h = 2;
    let line_y = y + header_h;
    let border = 2;

    let _ = Rectangle::new(Point::new(x, y), Size::new(w as u32, h as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target);

    let _ = Rectangle::new(
        Point::new(x + border, y + border),
        Size::new((w - border * 2).max(1) as u32, (header_h - border).max(1) as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
    .draw(target);

    for inset in 0..border {
        for px in (x + border - inset)..=(x + w - 1 - (border - inset)) {
            let _ = Pixel(Point::new(px, y + inset), BinaryColor::Off).draw(target);
            let _ = Pixel(Point::new(px, y + h - 1 - inset), BinaryColor::Off).draw(target);
        }
    }
    for inset in 0..border {
        for py in (y + border - inset)..=(y + h - 1 - (border - inset)) {
            let _ = Pixel(Point::new(x + inset, py), BinaryColor::Off).draw(target);
            let _ = Pixel(Point::new(x + w - 1 - inset, py), BinaryColor::Off).draw(target);
        }
    }

    for (cx, cy) in [
        (x, y),
        (x + 1, y),
        (x, y + 1),
        (x + w - 1, y),
        (x + w - 2, y),
        (x + w - 1, y + 1),
        (x, y + h - 1),
        (x + 1, y + h - 1),
        (x, y + h - 2),
        (x + w - 1, y + h - 1),
        (x + w - 2, y + h - 1),
        (x + w - 1, y + h - 2),
    ] {
        let _ = Pixel(Point::new(cx, cy), BinaryColor::On).draw(target);
    }

    let _ = Rectangle::new(
        Point::new(x + border, line_y),
        Size::new((w - border * 2).max(1) as u32, divider_h as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
    .draw(target);
}

pub fn draw_alert_frame_hi<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    header_h: i32,
) {
    if w <= 8 || h <= 8 {
        return;
    }
    let header_h = header_h.clamp(20, h.saturating_sub(8));
    let divider_h = 5;
    let line_y = y + header_h;
    let border = 5;

    let _ = Rectangle::new(Point::new(x, y), Size::new(w as u32, h as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target);

    let _ = Rectangle::new(
        Point::new(x + border, y + border),
        Size::new((w - border * 2).max(1) as u32, (header_h - border).max(1) as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
    .draw(target);

    for inset in 0..border {
        for px in (x + border - inset)..=(x + w - 1 - (border - inset)) {
            let _ = Pixel(Point::new(px, y + inset), BinaryColor::Off).draw(target);
            let _ = Pixel(Point::new(px, y + h - 1 - inset), BinaryColor::Off).draw(target);
        }
    }
    for inset in 0..border {
        for py in (y + border - inset)..=(y + h - 1 - (border - inset)) {
            let _ = Pixel(Point::new(x + inset, py), BinaryColor::Off).draw(target);
            let _ = Pixel(Point::new(x + w - 1 - inset, py), BinaryColor::Off).draw(target);
        }
    }

    for (cx, cy) in [
        (x, y),
        (x + 1, y),
        (x, y + 1),
        (x + 1, y + 1),
        (x + 2, y),
        (x, y + 2),
        (x + w - 1, y),
        (x + w - 2, y),
        (x + w - 1, y + 1),
        (x + w - 2, y + 1),
        (x + w - 3, y),
        (x + w - 1, y + 2),
        (x, y + h - 1),
        (x + 1, y + h - 1),
        (x, y + h - 2),
        (x + 1, y + h - 2),
        (x + 2, y + h - 1),
        (x, y + h - 3),
        (x + w - 1, y + h - 1),
        (x + w - 2, y + h - 1),
        (x + w - 1, y + h - 2),
        (x + w - 2, y + h - 2),
        (x + w - 3, y + h - 1),
        (x + w - 1, y + h - 3),
    ] {
        let _ = Pixel(Point::new(cx, cy), BinaryColor::On).draw(target);
    }

    let _ = Rectangle::new(
        Point::new(x + border, line_y),
        Size::new((w - border * 2).max(1) as u32, divider_h as u32),
    )
    .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
    .draw(target);
}

pub fn draw_palm_box<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    with_shadow: bool,
) {
    if w <= 4 || h <= 4 {
        return;
    }
    let _ = Rectangle::new(Point::new(x, y), Size::new(w.max(1) as u32, h.max(1) as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target);

    for px in (x + 1)..=(x + w - 2) {
        let _ = Pixel(Point::new(px, y), BinaryColor::Off).draw(target);
        let _ = Pixel(Point::new(px, y + h - 1), BinaryColor::Off).draw(target);
    }
    for py in (y + 1)..=(y + h - 2) {
        let _ = Pixel(Point::new(x, py), BinaryColor::Off).draw(target);
        let _ = Pixel(Point::new(x + w - 1, py), BinaryColor::Off).draw(target);
    }

    if with_shadow {
        let shadow_y = y + h;
        let shadow_x = x + w;
        for px in (x + 3)..=(x + w - 3) {
            let _ = Pixel(Point::new(px, shadow_y), BinaryColor::Off).draw(target);
        }
        for py in (y + 2)..=(y + h - 3) {
            let _ = Pixel(Point::new(shadow_x, py), BinaryColor::Off).draw(target);
        }
    }
}

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
    let _ = Rectangle::new(Point::new(x, y), Size::new(w.max(1) as u32, h.max(1) as u32))
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

pub fn draw_palm_box_hi<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    with_shadow: bool,
) {
    if w <= 6 || h <= 6 {
        draw_palm_box(target, x, y, w, h, with_shadow);
        return;
    }
    let _ = Rectangle::new(Point::new(x, y), Size::new(w.max(1) as u32, h.max(1) as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target);

    for inset in 0..2 {
        let ix = x + inset;
        let iy = y + inset;
        let iw = w - inset * 2;
        let ih = h - inset * 2;
        for px in (ix + 1)..=(ix + iw - 2) {
            let _ = Pixel(Point::new(px, iy), BinaryColor::Off).draw(target);
            let _ = Pixel(Point::new(px, iy + ih - 1), BinaryColor::Off).draw(target);
        }
        for py in (iy + 1)..=(iy + ih - 2) {
            let _ = Pixel(Point::new(ix, py), BinaryColor::Off).draw(target);
            let _ = Pixel(Point::new(ix + iw - 1, py), BinaryColor::Off).draw(target);
        }
    }

    if with_shadow {
        let shadow_y = y + h;
        let shadow_x = x + w;
        for off in 0..2 {
            for px in (x + 4)..=(x + w - 4) {
                let _ = Pixel(Point::new(px, shadow_y + off), BinaryColor::Off).draw(target);
            }
            for py in (y + 3)..=(y + h - 4) {
                let _ = Pixel(Point::new(shadow_x + off, py), BinaryColor::Off).draw(target);
            }
        }
    }
}

pub fn draw_palm_pull_down_box_hi<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    if w <= 6 || h <= 6 {
        draw_palm_pull_down_box(target, x, y, w, h);
        return;
    }
    let _ = Rectangle::new(Point::new(x, y), Size::new(w.max(1) as u32, h.max(1) as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target);

    for inset in 0..2 {
        let ix = x + inset;
        let iy = y + inset;
        let iw = w - inset * 2;
        let ih = h - inset * 2;
        for px in ix..=(ix + iw - 1) {
            let _ = Pixel(Point::new(px, iy), BinaryColor::Off).draw(target);
        }
        for px in (ix + 1)..=(ix + iw - 2) {
            let _ = Pixel(Point::new(px, iy + ih - 1), BinaryColor::Off).draw(target);
        }
        for py in iy..=(iy + ih - 2) {
            let _ = Pixel(Point::new(ix, py), BinaryColor::Off).draw(target);
            let _ = Pixel(Point::new(ix + iw - 1, py), BinaryColor::Off).draw(target);
        }
    }

    let shadow_y = y + h;
    let shadow_x = x + w;
    for off in 0..2 {
        for px in (x + 4)..=(x + w - 4) {
            let _ = Pixel(Point::new(px, shadow_y + off), BinaryColor::Off).draw(target);
        }
        for py in (y + 2)..=(y + h - 4) {
            let _ = Pixel(Point::new(shadow_x + off, py), BinaryColor::Off).draw(target);
        }
    }
}
