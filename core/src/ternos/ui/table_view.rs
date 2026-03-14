extern crate alloc;

use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Size,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::{Point as EgPoint, Primitive},
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
    Drawable,
};
use alloc::{string::String, vec::Vec};

use super::{
    geom::{Point, Rect},
    runtime::{UiTableCell, UiTableModel, UiTableRow},
    view::{RenderQueue, UiContext, View},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableHit {
    Cell { row: usize, col: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableScrollBarHit {
    ArrowUp,
    ArrowDown,
    Track { top_row: usize },
}

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

    pub fn visible_row_count(&self, rect: Rect) -> usize {
        if rect.w <= 0 || rect.h <= 0 {
            return 0;
        }
        let mut y = rect.y;
        let mut count = 0usize;
        for row in self.model.rows.iter().skip(self.model.top_row as usize) {
            let row_h = (row.height as i32).max(1);
            if y + row_h > rect.y + rect.h {
                break;
            }
            count += 1;
            y += row_h;
        }
        count
    }

    pub fn hit_test(&self, rect: Rect, point: Point) -> Option<TableHit> {
        if rect.w <= 0 || rect.h <= 0 || !rect.contains(point) {
            return None;
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

        let mut y = rect.y;
        for (row_index, row) in self.model.rows.iter().enumerate().skip(self.model.top_row as usize) {
            let row_h = (row.height as i32).max(1);
            let row_rect = Rect::new(rect.x, y, rect.w, row_h);
            if point.y >= row_rect.y && point.y < row_rect.y + row_rect.h {
                for col_index in 0..col_count {
                    let cell_left = x_positions[col_index];
                    let cell_right = x_positions[col_index + 1];
                    if point.x >= cell_left && point.x < cell_right {
                        return Some(TableHit::Cell {
                            row: row_index,
                            col: col_index,
                        });
                    }
                }
                return None;
            }
            y += row_h;
            if y >= rect.y + rect.h {
                break;
            }
        }

        None
    }
}

pub struct TableScrollBarView {
    pub top_row: usize,
    pub visible_rows: usize,
    pub total_rows: usize,
}

impl TableScrollBarView {
    pub fn new(top_row: usize, visible_rows: usize, total_rows: usize) -> Self {
        Self {
            top_row,
            visible_rows,
            total_rows,
        }
    }

    pub fn hit_test(&self, rect: Rect, point: Point) -> Option<TableScrollBarHit> {
        if rect.w <= 0 || rect.h <= 0 || !rect.contains(point) {
            return None;
        }
        let arrow_h = 7i32.min((rect.h / 4).max(5));
        let up_rect = Rect::new(rect.x, rect.y, rect.w, arrow_h + 2);
        let down_rect = Rect::new(rect.x, rect.y + rect.h - (arrow_h + 2), rect.w, arrow_h + 2);
        if up_rect.contains(point) {
            return Some(TableScrollBarHit::ArrowUp);
        }
        if down_rect.contains(point) {
            return Some(TableScrollBarHit::ArrowDown);
        }

        let track_top = rect.y + arrow_h + 2;
        let track_bottom = rect.y + rect.h - arrow_h - 3;
        if track_bottom <= track_top || self.total_rows <= self.visible_rows || self.visible_rows == 0 {
            return None;
        }
        let track_rect = Rect::new(rect.x, track_top, rect.w, track_bottom - track_top + 1);
        if !track_rect.contains(point) {
            return None;
        }

        let track_h = track_rect.h.max(1);
        let thumb_h = ((track_h * self.visible_rows as i32) / self.total_rows as i32)
            .max(8)
            .min(track_h);
        let max_top = self.total_rows.saturating_sub(self.visible_rows);
        let relative_y = (point.y - track_rect.y).clamp(0, track_h - 1);
        let centered = (relative_y - (thumb_h / 2)).clamp(0, track_h - thumb_h);
        let top_row = if max_top == 0 || track_h == thumb_h {
            0
        } else {
            (centered as usize * max_top) / (track_h - thumb_h) as usize
        };
        Some(TableScrollBarHit::Track { top_row })
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
        let top_row = self.model.top_row as usize;
        for (row_index, row) in self.model.rows.iter().enumerate().skip(top_row) {
            let row_h = (row.height as i32).max(1);
            let row_top = y;
            let row_bottom = (y + row_h - 1).min(rect.y + rect.h - 1);
            let selected_row = self.model.selected_row == Some(row_index as u16);

            if selected_row {
                let _ = Rectangle::new(
                    EgPoint::new(rect.x, row_top),
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
                        EgPoint::new(cell_left + 4, row_top + 14),
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

impl View for TableScrollBarView {
    fn render(&mut self, ctx: &mut UiContext<'_>, rect: Rect, rq: &mut RenderQueue) {
        if rect.w <= 0 || rect.h <= 0 {
            return;
        }

        let _ = Rectangle::new(EgPoint::new(rect.x, rect.y), Size::new(rect.w as u32, rect.h as u32))
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::Off, 1))
            .draw(ctx.buffers);

        let arrow_h = 7i32.min((rect.h / 4).max(5));
        draw_triangle(ctx.buffers, rect.x + rect.w / 2, rect.y + 2, true, self.top_row > 0);
        draw_triangle(
            ctx.buffers,
            rect.x + rect.w / 2,
            rect.y + rect.h - 3,
            false,
            self.top_row + self.visible_rows < self.total_rows,
        );

        let track_top = rect.y + arrow_h + 2;
        let track_bottom = rect.y + rect.h - arrow_h - 3;
        if track_bottom > track_top {
            let _ = Rectangle::new(
                EgPoint::new(rect.x + rect.w / 2, track_top),
                Size::new(1, (track_bottom - track_top + 1) as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
            .draw(ctx.buffers);

            if self.total_rows > self.visible_rows && self.visible_rows > 0 {
                let track_h = (track_bottom - track_top + 1).max(1);
                let thumb_h = ((track_h * self.visible_rows as i32) / self.total_rows as i32)
                    .max(8)
                    .min(track_h);
                let max_top = self.total_rows.saturating_sub(self.visible_rows);
                let thumb_offset = if max_top == 0 {
                    0
                } else {
                    ((track_h - thumb_h) * self.top_row as i32 / max_top as i32).max(0)
                };
                let thumb_y = track_top + thumb_offset;
                let _ = Rectangle::new(
                    EgPoint::new(rect.x + 1, thumb_y),
                    Size::new((rect.w - 2).max(1) as u32, thumb_h as u32),
                )
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                .draw(ctx.buffers);
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
            let _ = Rectangle::new(EgPoint::new(x, y), Size::new(1, 1))
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
            let _ = Rectangle::new(EgPoint::new(x, y), Size::new(1, 1))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(target);
        }
    }
}

fn draw_triangle<T: DrawTarget<Color = BinaryColor>>(
    target: &mut T,
    cx: i32,
    y: i32,
    up: bool,
    enabled: bool,
) {
    if !enabled {
        return;
    }
    for row in 0..4 {
        let yy = if up { y + row } else { y - row };
        for dx in -row..=row {
            let _ = Rectangle::new(EgPoint::new(cx + dx, yy), Size::new(1, 1))
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                .draw(target);
        }
    }
}
