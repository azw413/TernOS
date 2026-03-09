use embedded_graphics::{
    Drawable, Pixel,
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Point, Primitive, Size},
    primitives::{PrimitiveStyle, Rectangle},
};

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
