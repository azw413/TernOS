use embedded_graphics::{
    Drawable, Pixel,
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Point, Primitive, Size},
    primitives::{PrimitiveStyle, Rectangle},
};

use crate::palm::runtime::PalmFont;

use super::text::{draw_palm_text, palm_text_height, palm_text_width};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PalmDensity {
    Low,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PalmMetrics {
    pub corner_inner: i32,
    pub corner_outer: i32,
    pub title_corner_trim: i32,
    pub title_vertical_trim: i32,
    pub button_stroke: i32,
    pub focus_stroke: u32,
    pub field_stroke: u32,
}

impl PalmMetrics {
    pub const fn for_density(density: PalmDensity) -> Self {
        match density {
            PalmDensity::Low => Self {
                corner_inner: 1,
                corner_outer: 2,
                title_corner_trim: 1,
                title_vertical_trim: 1,
                button_stroke: 1,
                focus_stroke: 1,
                field_stroke: 1,
            },
            PalmDensity::High => Self {
                corner_inner: 2,
                corner_outer: 4,
                title_corner_trim: 2,
                title_vertical_trim: 2,
                button_stroke: 2,
                focus_stroke: 1,
                field_stroke: 1,
            },
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiResourceKind {
    Alert,
    AppIcon,
    Bitmap,
    CommandButton,
    Checkbox,
    Form,
    Gadget,
    ShiftIndicator,
    Label,
    List,
    MenuBar,
    Menu,
    PopupTrigger,
    PopupList,
    PushButton,
    RepeatingButton,
    ScrollBar,
    SelectorTrigger,
    Slider,
    Table,
    Field,
}

pub fn draw_button_frame<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: BinaryColor,
) {
    draw_button_frame_with_metrics(
        target,
        x,
        y,
        w,
        h,
        color,
        PalmMetrics::for_density(PalmDensity::Low),
    );
}

pub fn draw_button_frame_hi<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: BinaryColor,
) {
    draw_button_frame_with_metrics(
        target,
        x,
        y,
        w,
        h,
        color,
        PalmMetrics::for_density(PalmDensity::High),
    );
}

pub fn draw_button_frame_with_metrics<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: BinaryColor,
    metrics: PalmMetrics,
) {
    let stroke_count = metrics.button_stroke.max(1);
    for inset in 0..stroke_count {
        let inset_metrics = PalmMetrics {
            corner_inner: (metrics.corner_inner - inset).max(1),
            corner_outer: (metrics.corner_outer - inset).max(1),
            ..metrics
        };
        draw_single_button_frame(
            target,
            x + inset,
            y + inset,
            w - inset * 2,
            h - inset * 2,
            color,
            inset_metrics,
        );
    }
}

fn draw_single_button_frame<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: BinaryColor,
    metrics: PalmMetrics,
) {
    let o1 = metrics.corner_inner;
    let o2 = metrics.corner_outer;
    let line_inset = if metrics.button_stroke > 1 {
        (o2 - 1).max(1)
    } else {
        o2
    };
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

    let _ = Rectangle::new(Point::new(x0 + line_inset, y0), Size::new((w - line_inset * 2) as u32, 1))
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(target);
    let _ = Rectangle::new(Point::new(x0 + line_inset, y1), Size::new((w - line_inset * 2) as u32, 1))
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(target);
    let _ = Rectangle::new(Point::new(x0, y0 + line_inset), Size::new(1, (h - line_inset * 2) as u32))
        .into_styled(PrimitiveStyle::with_stroke(color, 1))
        .draw(target);
    let _ = Rectangle::new(Point::new(x1, y0 + line_inset), Size::new(1, (h - line_inset * 2) as u32))
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

pub fn draw_form_title_bar<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    tab_w: i32,
    tab_h: i32,
    line_thickness: i32,
) -> FormTitleLayout {
    draw_form_title_bar_with_metrics(
        target,
        x,
        y,
        w,
        tab_w,
        tab_h,
        line_thickness,
        PalmMetrics::for_density(PalmDensity::Low),
    )
}

pub fn draw_form_title_bar_hi<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    tab_w: i32,
    tab_h: i32,
    line_thickness: i32,
) -> FormTitleLayout {
    draw_form_title_bar_with_metrics(
        target,
        x,
        y,
        w,
        tab_w,
        tab_h,
        line_thickness,
        PalmMetrics::for_density(PalmDensity::High),
    )
}

pub fn draw_form_title_bar_with_metrics<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    tab_w: i32,
    tab_h: i32,
    line_thickness: i32,
    metrics: PalmMetrics,
) -> FormTitleLayout {
    let tab_w = tab_w.max(1).min(w.max(1));
    let tab_h = tab_h.max(1);
    let line_thickness = line_thickness.max(1);
    let tab_x = x;
    let tab_y = y;
    let _ = Rectangle::new(Point::new(tab_x, tab_y), Size::new(tab_w as u32, tab_h as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(target);
    let _ = Pixel(Point::new(tab_x, tab_y), BinaryColor::On).draw(target);
    let _ = Pixel(Point::new(tab_x + tab_w - 1, tab_y), BinaryColor::On).draw(target);
    if tab_w >= 4 {
        for trim in 1..=metrics.title_corner_trim {
            let _ = Pixel(Point::new(tab_x + trim, tab_y), BinaryColor::On).draw(target);
            let _ = Pixel(
                Point::new(tab_x + tab_w - 1 - trim, tab_y),
                BinaryColor::On,
            )
            .draw(target);
        }
    }
    if tab_h >= 2 {
        for trim in 1..=metrics.title_vertical_trim {
            let _ = Pixel(Point::new(tab_x, tab_y + trim), BinaryColor::On).draw(target);
            let _ = Pixel(
                Point::new(tab_x + tab_w - 1, tab_y + trim),
                BinaryColor::On,
            )
            .draw(target);
        }
    }

    let line_y = tab_y + tab_h;
    for i in 0..line_thickness {
        let _ = Rectangle::new(Point::new(x, line_y + i), Size::new(w.max(1) as u32, 1))
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

pub fn draw_form_field<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    focused: bool,
) {
    draw_form_field_with_metrics(
        target,
        x,
        y,
        w,
        h,
        focused,
        PalmMetrics::for_density(PalmDensity::Low),
    );
}

pub fn draw_form_field_hi<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    focused: bool,
) {
    draw_form_field_with_metrics(
        target,
        x,
        y,
        w,
        h,
        focused,
        PalmMetrics::for_density(PalmDensity::High),
    );
}

pub fn draw_form_field_with_metrics<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    _focused: bool,
    metrics: PalmMetrics,
) {
    let _ = Rectangle::new(
        Point::new(x.max(0), y.max(0)),
        Size::new(w.max(1) as u32, h.max(1) as u32),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, metrics.field_stroke))
    .draw(target);
}

pub fn draw_form_button<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    fonts: &[PalmFont],
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font_id: u8,
    style: u8,
    no_frame: bool,
    text: &str,
    focused: bool,
) {
    draw_form_button_with_metrics(
        target,
        fonts,
        x,
        y,
        w,
        h,
        font_id,
        style,
        no_frame,
        text,
        focused,
        PalmMetrics::for_density(PalmDensity::Low),
    );
}

pub fn draw_form_button_hi<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    fonts: &[PalmFont],
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font_id: u8,
    style: u8,
    no_frame: bool,
    text: &str,
    focused: bool,
) {
    draw_form_button_with_metrics(
        target,
        fonts,
        x,
        y,
        w,
        h,
        font_id,
        style,
        no_frame,
        text,
        focused,
        PalmMetrics::for_density(PalmDensity::High),
    );
}

pub fn draw_form_button_with_metrics<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    fonts: &[PalmFont],
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font_id: u8,
    style: u8,
    no_frame: bool,
    text: &str,
    focused: bool,
    metrics: PalmMetrics,
) {
    let bx = x;
    let mut by = y;
    let mut bw = w.max(8);
    let mut bh = h.max(8);
    if style == 1 {
        by -= 1;
        bw += 2;
        bh += 2;
    }
    if bw <= 0 || bh <= 0 {
        return;
    }

    if style == 5 {
        if focused {
            let _ = Rectangle::new(
                Point::new(bx.max(0), by.max(0)),
                Size::new(bw.max(1) as u32, bh.max(1) as u32),
            )
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, metrics.focus_stroke))
            .draw(target);
        }
        if focused && bw > 2 && bh > 2 {
            let _ = Rectangle::new(
                Point::new((bx + 1).max(0), (by + 1).max(0)),
                Size::new((bw - 2).max(1) as u32, (bh - 2).max(1) as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
            .draw(target);
        }
        let up = matches!(text, "^" | "~");
        draw_repeating_button_glyph(
            target,
            bx,
            by,
            bw,
            bh,
            up,
            if focused { BinaryColor::On } else { BinaryColor::Off },
            matches!(text, "~" | "V"),
        );
        return;
    }

    if !no_frame {
        draw_button_outline(target, bx, by, bw, bh, style, metrics);
        if focused && bw > 4 && bh > 4 {
            let _ = Rectangle::new(Point::new(bx + 1, by + 1), Size::new((bw - 2) as u32, (bh - 2) as u32))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                .draw(target);
        }
    } else if focused {
        let _ = Rectangle::new(
            Point::new(bx.max(0), by.max(0)),
            Size::new(bw.max(1) as u32, bh.max(1) as u32),
        )
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, metrics.focus_stroke))
        .draw(target);
    }

    let tw = palm_text_width(text, font_id, fonts, 1);
    let th = palm_text_height(font_id, fonts, 1);
    let mut tx = bx + ((bw - tw) / 2).max(1);
    if style == 1 {
        tx += 1;
    }
    let ty = by + ((bh - th) / 2).max(1);
    if no_frame && focused {
        let _ = Rectangle::new(
            Point::new((tx - 1).max(0), (ty - 1).max(0)),
            Size::new((tw + 2).max(1) as u32, (th + 2).max(1) as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
        .draw(target);
    }
    draw_palm_text(
        target,
        text,
        tx,
        ty,
        font_id,
        fonts,
        1,
        if focused { BinaryColor::On } else { BinaryColor::Off },
    );
}

fn draw_button_outline<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    bx: i32,
    by: i32,
    bw: i32,
    bh: i32,
    style: u8,
    metrics: PalmMetrics,
) {
    if style == 1 {
        let _ = Rectangle::new(Point::new(bx, by), Size::new(bw.max(1) as u32, bh.max(1) as u32))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 1))
            .draw(target);
    } else {
        draw_button_frame_with_metrics(target, bx, by, bw, bh, BinaryColor::Off, metrics);
    }
}

fn draw_repeating_button_glyph<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    bx: i32,
    by: i32,
    bw: i32,
    bh: i32,
    up: bool,
    color: BinaryColor,
    dithered: bool,
) {
    let cx = bx + bw / 2;
    let cy = by + bh / 2;
    let half = ((bw.min(bh) - 2) / 2).max(2);
    for row in 0..=half {
        let y = if up { cy - half / 2 + row } else { cy + half / 2 - row };
        for dx in -row..=row {
            if !dithered || (((cx + dx) + y) & 1) == 0 {
                let _ = Pixel(Point::new(cx + dx, y), color).draw(target);
            }
        }
    }
}
