use embedded_graphics::{
    Drawable,
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Point, Primitive, Size},
    primitives::{PrimitiveStyle, Rectangle},
};

use crate::ternos::ui::prc_components;

pub fn draw_alert_frame<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    header_h: i32,
) {
    prc_components::draw_alert_frame(target, x, y, w, h, header_h);
}

pub fn draw_done_button<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    let _ = Rectangle::new(Point::new(x, y), Size::new(w as u32, h as u32))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target);
    prc_components::draw_button_frame(target, x, y, w, h, BinaryColor::Off);
}

pub fn draw_scroll_indicator<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y: i32,
    up: bool,
    down: bool,
) {
    prc_components::draw_scroll_indicator(target, x, y, up, down);
}
