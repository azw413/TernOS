extern crate alloc;

use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Size,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::{Point, Primitive},
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
    Drawable,
};
use alloc::{string::String, vec::Vec};

use super::{
    geom::Rect,
    runtime::{UiTableCell, UiTableModel, UiTableRow},
    view::{RenderQueue, UiContext, View},
};

pub trait TableCellRenderer {
    fn render_cell(
        &self,
        ctx: &mut UiContext<'_>,
        cell_rect: Rect,
        row: &UiTableRow,
        cell: &UiTableCell,
        row_index: usize,
        col_index: usize,
        selected: bool,
    );
}

pub struct TableView<'a> {
    pub model: &'a UiTableModel,
    pub clear: bool,
    pub draw_grid: bool,
    pub renderer: Option<&'a dyn TableCellRenderer>,
}

impl<'a> TableView<'a> {
    pub fn new(model: &'a UiTableModel) -> Self {
        Self {
            model,
            clear: false,
            draw_grid: true,
            renderer: None,
        }
    }
}

impl View for TableView<'_> {
    fn render(&mut self, ctx: &mut UiContext<'_>, rect: Rect, rq: &mut RenderQueue) {
        if self.clear {
            ctx.buffers.clear(BinaryColor::On).ok();
        }
        if rect.w <= 0 || rect.h <= 0 {
            return;
        }

        let col_count = self.model.cols.max(1) as usize;
        let mut x_positions = Vec::with_capacity(col_count + 1);
        x_positions.push(rect.x);
        let mut remaining_w = rect.w;
        let mut remaining_cols = col_count as i32;
        for col_idx in 0..col_count {
            let explicit = self
                .model
                .columns
                .get(col_idx)
                .map(|c| c.width as i32)
                .filter(|w| *w > 0);
            let width = explicit.unwrap_or_else(|| (remaining_w / remaining_cols.max(1)).max(1));
            let last_x = *x_positions.last().unwrap_or(&rect.x);
            x_positions.push(last_x + width);
            remaining_w -= width;
            remaining_cols -= 1;
        }

        let header_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        let mut y = rect.y;
        for (row_index, row) in self.model.rows.iter().enumerate() {
            let row_h = (row.height as i32).max(1);
            let row_top = y;
            let row_bottom = (y + row_h - 1).min(rect.y + rect.h - 1);
            let selected_row = self.model.selected_row == Some(row_index as u16);

            if selected_row {
                let _ = Rectangle::new(
                    Point::new(rect.x, row_top),
                    Size::new(rect.w as u32, (row_bottom - row_top + 1).max(1) as u32),
                )
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                .draw(ctx.buffers);
            }

            for col_index in 0..col_count {
                let cell_left = x_positions[col_index];
                let cell_right = x_positions[col_index + 1] - 1;
                let cell_rect = Rect::new(
                    cell_left,
                    row_top,
                    (cell_right - cell_left + 1).max(1),
                    (row_bottom - row_top + 1).max(1),
                );
                let selected = selected_row && self.model.selected_col == Some(col_index as u16);
                let fallback_cell = UiTableCell {
                    text: String::new(),
                };
                let cell = row.cells.get(col_index).unwrap_or(&fallback_cell);
                if let Some(renderer) = self.renderer {
                    renderer.render_cell(ctx, cell_rect, row, cell, row_index, col_index, selected);
                } else {
                    let style = MonoTextStyle::new(
                        &FONT_10X20,
                        if selected_row { BinaryColor::On } else { BinaryColor::Off },
                    );
                    Text::new(
                        &cell.text,
                        Point::new(cell_left + 4, row_top + 14),
                        if row.cells.is_empty() { header_style } else { style },
                    )
                    .draw(ctx.buffers)
                    .ok();
                }

                if self.draw_grid && col_index + 1 < col_count {
                    draw_dotted_vline(ctx.buffers, cell_right, row_top, row_bottom, BinaryColor::Off);
                }
            }

            if self.draw_grid && row_index + 1 < self.model.rows.len() {
                draw_dotted_hline(ctx.buffers, rect.x, rect.x + rect.w - 1, row_bottom, BinaryColor::Off);
            }
            y += row_h;
            if y >= rect.y + rect.h {
                break;
            }
        }

        rq.push(rect, crate::display::RefreshMode::Fast);
    }
}

fn draw_dotted_hline<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x0: i32,
    x1: i32,
    y: i32,
    color: BinaryColor,
) {
    let start = x0.min(x1);
    let end = x0.max(x1);
    for x in start..=end {
        if ((x - start) & 1) == 0 {
            let _ = Rectangle::new(Point::new(x, y), Size::new(1, 1))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(target);
        }
    }
}

fn draw_dotted_vline<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    x: i32,
    y0: i32,
    y1: i32,
    color: BinaryColor,
) {
    let start = y0.min(y1);
    let end = y0.max(y1);
    for y in start..=end {
        if ((y - start) & 1) == 0 {
            let _ = Rectangle::new(Point::new(x, y), Size::new(1, 1))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(target);
        }
    }
}
