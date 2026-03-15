extern crate alloc;

use alloc::vec::Vec;

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::{DrawTarget, Point as EgPoint, Primitive},
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
    Drawable,
};

use crate::palm::runtime::PalmFont;

use super::{
    geom::{Point, Rect},
    prc_components::{draw_palm_pull_down_box, draw_palm_text_scaled, palm_text_height_scaled, palm_text_width_scaled},
    view::{RenderQueue, UiContext, View},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupHit {
    Trigger,
    Item(usize),
    Outside,
}

pub struct PopupMenuView<'a> {
    pub items: &'a [&'a str],
    pub selected: usize,
    pub open: bool,
    pub trigger_label: &'a str,
    pub trigger_rect: Rect,
    pub popup_rect: Rect,
    pub item_height: i32,
    pub palm_fonts: &'a [PalmFont],
}

impl<'a> PopupMenuView<'a> {
    pub fn category_menu(
        width: i32,
        trigger_label: &'a str,
        items: &'a [&'a str],
        selected: usize,
        open: bool,
        palm_fonts: &'a [PalmFont],
    ) -> Self {
        let item_font_id = 0u8;
        let item_scale_num = 6;
        let item_scale_den = 5;
        let item_text_h = if !palm_fonts.is_empty() {
            palm_text_height_scaled(
                item_font_id,
                palm_fonts,
                item_scale_num,
                item_scale_den,
            )
        } else {
            10
        };
        let max_text_w = if !palm_fonts.is_empty() {
            items.iter()
                .map(|label| {
                    palm_text_width_scaled(
                        label,
                        item_font_id,
                        palm_fonts,
                        item_scale_num,
                        item_scale_den,
                    )
                })
                .max()
                .unwrap_or(0)
        } else {
            items.iter().map(|label| (label.len() as i32) * 6).max().unwrap_or(0)
        };
        let item_height = item_text_h + 6;
        let form_x = 2;
        let form_y = 36;
        let form_w = (width - 4).max(1);
        let menu_w = (max_text_w + 14).max(48);
        let menu_x = form_x + form_w - menu_w - 4;
        let menu_y = form_y + 1;
        let menu_h = item_height * items.len() as i32 + 4;
        let trigger_rect = Rect::new(width - 96, form_y, 92, 24);
        Self {
            items,
            selected,
            open,
            trigger_label,
            trigger_rect,
            popup_rect: Rect::new(menu_x, menu_y, menu_w, menu_h),
            item_height,
            palm_fonts,
        }
    }

    pub fn item_rect(&self, index: usize) -> Rect {
        Rect::new(
            self.popup_rect.x + 1,
            self.popup_rect.y + 3 + (index as i32 * self.item_height),
            self.popup_rect.w - 2,
            self.item_height - 1,
        )
    }

    pub fn hit_test(&self, point: Point) -> Option<PopupHit> {
        if self.trigger_rect.contains(point) {
            return Some(PopupHit::Trigger);
        }
        if self.open {
            if !self.popup_rect.contains(point) {
                return Some(PopupHit::Outside);
            }
            for index in 0..self.items.len() {
                if self.item_rect(index).contains(point) {
                    return Some(PopupHit::Item(index));
                }
            }
            return Some(PopupHit::Outside);
        }
        None
    }

    pub fn object_rects(&self) -> Vec<Rect> {
        let mut rects = Vec::with_capacity(1 + self.items.len());
        rects.push(self.trigger_rect);
        if self.open {
            for index in 0..self.items.len() {
                rects.push(self.item_rect(index));
            }
        }
        rects
    }

    pub fn render_category_trigger<T: DrawTarget<Color = BinaryColor>>(
        &self,
        target: &mut T,
        focused: bool,
    ) {
        let category_font_id = 0u8;
        let ui_scale_num = 6;
        let ui_scale_den = 5;
        let cat_w = if !self.palm_fonts.is_empty() {
            palm_text_width_scaled(
                self.trigger_label,
                category_font_id,
                self.palm_fonts,
                ui_scale_num,
                ui_scale_den,
            )
        } else {
            (self.trigger_label.len() as i32) * 10
        };
        let cat_h = if !self.palm_fonts.is_empty() {
            palm_text_height_scaled(
                category_font_id,
                self.palm_fonts,
                ui_scale_num,
                ui_scale_den,
            )
        } else {
            20
        };
        let right_edge = self.trigger_rect.x + self.trigger_rect.w - 2;
        let arrow_x = right_edge - cat_w - 14;
        let text_x = right_edge - cat_w;
        let trigger_y = self.trigger_rect.y + 1;
        if focused {
            Rectangle::new(
                EgPoint::new(arrow_x - 4, trigger_y - 1),
                embedded_graphics::geometry::Size::new((cat_w + 18) as u32, (cat_h + 2) as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
            .draw(target)
            .ok();
        }
        let text_color = if focused { BinaryColor::On } else { BinaryColor::Off };
        if !self.palm_fonts.is_empty() {
            draw_palm_text_scaled(
                target,
                self.trigger_label,
                text_x,
                trigger_y,
                category_font_id,
                self.palm_fonts,
                ui_scale_num,
                ui_scale_den,
                text_color,
            );
        } else {
            Text::new(
                self.trigger_label,
                EgPoint::new(text_x, trigger_y + 11),
                MonoTextStyle::new(&FONT_6X10, text_color),
            )
            .draw(target)
            .ok();
        }
        let arrow_y = trigger_y + (cat_h / 2) - 1;
        for (dx, w) in [(0, 7), (1, 5), (2, 3), (3, 1)] {
            Rectangle::new(
                EgPoint::new(arrow_x + dx, arrow_y + dx),
                embedded_graphics::geometry::Size::new(w as u32, 1),
            )
            .into_styled(PrimitiveStyle::with_fill(text_color))
            .draw(target)
            .ok();
        }
    }
}

impl View for PopupMenuView<'_> {
    fn render(&mut self, ctx: &mut UiContext<'_>, _rect: Rect, rq: &mut RenderQueue) {
        let item_font_id = 0u8;
        let item_scale_num = 6;
        let item_scale_den = 5;
        self.render_category_trigger(ctx.buffers, false);
        if !self.open {
            rq.push(self.trigger_rect, crate::display::RefreshMode::Fast);
            return;
        }
        draw_palm_pull_down_box(
            ctx.buffers,
            self.popup_rect.x,
            self.popup_rect.y,
            self.popup_rect.w,
            self.popup_rect.h,
        );
        for (i, label) in self.items.iter().enumerate() {
            let item_rect = self.item_rect(i);
            let selected = self.selected == i;
            if selected {
                Rectangle::new(
                    EgPoint::new(item_rect.x, item_rect.y - 1),
                    embedded_graphics::geometry::Size::new(item_rect.w as u32, item_rect.h as u32),
                )
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                .draw(ctx.buffers)
                .ok();
            }
            if !self.palm_fonts.is_empty() {
                draw_palm_text_scaled(
                    ctx.buffers,
                    label,
                    item_rect.x + 5,
                    item_rect.y + 3,
                    item_font_id,
                    self.palm_fonts,
                    item_scale_num,
                    item_scale_den,
                    if selected { BinaryColor::On } else { BinaryColor::Off },
                );
            } else {
                Text::new(
                    label,
                    EgPoint::new(item_rect.x + 5, item_rect.y + 10),
                    MonoTextStyle::new(
                        &FONT_6X10,
                        if selected { BinaryColor::On } else { BinaryColor::Off },
                    ),
                )
                .draw(ctx.buffers)
                .ok();
            }
        }
        rq.push(self.trigger_rect, crate::display::RefreshMode::Fast);
        rq.push(self.popup_rect, crate::display::RefreshMode::Fast);
    }
}
