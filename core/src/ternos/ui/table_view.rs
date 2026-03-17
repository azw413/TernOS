extern crate alloc;

use alloc::{format, string::{String, ToString}, vec, vec::Vec};
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
use crate::palm::runtime::PalmFont;

use super::{
    geom::{Point, Rect},
    runtime::{UiTableCell, UiTableModel, UiTableRow},
    text::{draw_palm_text, palm_text_height, palm_text_width},
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableInteraction {
    pub selected_row: Option<usize>,
    pub selected_col: Option<usize>,
    pub top_row: usize,
    pub activated: bool,
    pub dirty_rects: Vec<Rect>,
}

pub trait TableCellRenderer {
    fn row_height(&self, _table_rect: Rect, row: &UiTableRow) -> i32 {
        (row.height as i32).max(1)
    }

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

pub struct PalmWrappedTextCellRenderer<'a> {
    pub fonts: &'a [PalmFont],
    pub font_id: u8,
    pub padding_x: i32,
    pub padding_y: i32,
    pub line_spacing: i32,
}

impl PalmWrappedTextCellRenderer<'_> {
    fn wrapped_lines(&self, text: &str, cell_width: i32) -> Vec<String> {
        wrap_palm_text_lines(
            text,
            (cell_width - self.padding_x * 2).max(8),
            self.font_id,
            self.fonts,
        )
    }
}

impl TableCellRenderer for PalmWrappedTextCellRenderer<'_> {
    fn row_height(&self, table_rect: Rect, row: &UiTableRow) -> i32 {
        let cell_text = row.cells.first().map(|cell| cell.text.as_str()).unwrap_or("");
        let line_h = (palm_text_height(self.font_id, self.fonts, 1) + self.line_spacing).max(8);
        let lines = self.wrapped_lines(cell_text, table_rect.w);
        (self.padding_y * 2 + line_h * lines.len() as i32).max(line_h + self.padding_y * 2)
    }

    fn render_cell(
        &self,
        ctx: &mut UiContext<'_>,
        cell_rect: Rect,
        _row: &UiTableRow,
        cell: &UiTableCell,
        _row_index: usize,
        _col_index: usize,
        selected: bool,
    ) {
        let line_h = (palm_text_height(self.font_id, self.fonts, 1) + self.line_spacing).max(8);
        for (line_idx, line) in self.wrapped_lines(&cell.text, cell_rect.w).iter().enumerate() {
            draw_palm_text(
                ctx.buffers,
                line,
                cell_rect.x + self.padding_x,
                cell_rect.y + self.padding_y + line_idx as i32 * line_h,
                self.font_id,
                self.fonts,
                1,
                if selected {
                    BinaryColor::On
                } else {
                    BinaryColor::Off
                },
            );
        }
    }
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

    fn row_height_for(&self, rect: Rect, row: &UiTableRow) -> i32 {
        self.renderer
            .map(|renderer| renderer.row_height(rect, row))
            .unwrap_or_else(|| (row.height as i32).max(1))
            .max(1)
    }

    pub fn visible_row_count_from(&self, rect: Rect, top_row: usize) -> usize {
        if rect.w <= 0 || rect.h <= 0 {
            return 0;
        }
        let mut y = rect.y;
        let mut count = 0usize;
        for row in self.model.rows.iter().skip(top_row) {
            let row_h = self.row_height_for(rect, row);
            if y + row_h > rect.y + rect.h {
                break;
            }
            count += 1;
            y += row_h;
        }
        count
    }

    pub fn visible_row_count(&self, rect: Rect) -> usize {
        self.visible_row_count_from(rect, self.model.top_row as usize)
    }

    pub fn hit_test(&self, rect: Rect, point: Point) -> Option<TableHit> {
        if rect.w <= 0 || rect.h <= 0 || !rect.contains(point) {
            return None;
        }

        let col_count = self.model.cols.max(1) as usize;
        for row_index in self.model.top_row as usize..self.model.rows.len() {
            let Some(row_rect) = self.row_rect(rect, row_index) else {
                continue;
            };
            if point.y >= row_rect.y && point.y < row_rect.y + row_rect.h {
                for col_index in 0..col_count {
                    let Some(cell_rect) = self.cell_rect(rect, row_index, col_index) else {
                        continue;
                    };
                    if cell_rect.contains(point) {
                        return Some(TableHit::Cell { row: row_index, col: col_index });
                    }
                }
                return None;
            }
        }

        None
    }

    pub fn row_rect_from(&self, rect: Rect, top_row: usize, row_index: usize) -> Option<Rect> {
        if rect.w <= 0 || rect.h <= 0 || row_index < top_row {
            return None;
        }
        let mut y = rect.y;
        for (idx, row) in self
            .model
            .rows
            .iter()
            .enumerate()
            .skip(top_row)
        {
            let row_h = self.row_height_for(rect, row);
            let row_rect = Rect::new(rect.x, y, rect.w, row_h);
            if idx == row_index {
                return (row_rect.y + row_rect.h <= rect.y + rect.h).then_some(row_rect);
            }
            y += row_h;
            if y >= rect.y + rect.h {
                break;
            }
        }
        None
    }

    pub fn row_rect(&self, rect: Rect, row_index: usize) -> Option<Rect> {
        self.row_rect_from(rect, self.model.top_row as usize, row_index)
    }

    pub fn cell_rect(&self, rect: Rect, row_index: usize, col_index: usize) -> Option<Rect> {
        let row_rect = self.row_rect(rect, row_index)?;
        let x_positions = self.column_edges(rect);
        if col_index + 1 >= x_positions.len() {
            return None;
        }
        let cell_left = x_positions[col_index];
        let cell_right = x_positions[col_index + 1] - 1;
        Some(Rect::new(
            cell_left,
            row_rect.y,
            (cell_right - cell_left + 1).max(1),
            row_rect.h,
        ))
    }

    fn column_edges(&self, rect: Rect) -> Vec<i32> {
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
        x_positions
    }

    pub fn render_row(&self, ctx: &mut UiContext<'_>, rect: Rect, row_index: usize) {
        if rect.w <= 0 || rect.h <= 0 {
            return;
        }
        let Some(row) = self.model.rows.get(row_index) else {
            return;
        };
        let Some(row_rect) = self.row_rect(rect, row_index) else {
            return;
        };
        let col_count = self.model.cols.max(1) as usize;
        let x_positions = self.column_edges(rect);
        let header_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        let row_top = row_rect.y;
        let row_bottom = row_rect.y + row_rect.h - 1;
        let selected_row = self.model.selected_row == Some(row_index as u16);

        let _ = Rectangle::new(
            EgPoint::new(row_rect.x, row_top),
            Size::new(row_rect.w as u32, row_rect.h.max(1) as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(ctx.buffers);

        for col_index in 0..col_count {
            let cell_left = x_positions[col_index];
            let cell_right = x_positions[col_index + 1] - 1;
            let cell_rect = Rect::new(
                cell_left,
                row_top,
                (cell_right - cell_left + 1).max(1),
                row_rect.h.max(1),
            );
            let selected = selected_row && self.model.selected_col == Some(col_index as u16);
            let _ = Rectangle::new(
                EgPoint::new(cell_rect.x, cell_rect.y),
                Size::new(cell_rect.w.max(1) as u32, cell_rect.h.max(1) as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(if selected {
                BinaryColor::Off
            } else {
                BinaryColor::On
            }))
            .draw(ctx.buffers);
            let fallback_cell = UiTableCell {
                text: String::new(),
            };
            let cell = row.cells.get(col_index).unwrap_or(&fallback_cell);
            if let Some(renderer) = self.renderer {
                renderer.render_cell(ctx, cell_rect, row, cell, row_index, col_index, selected);
            } else {
                let style = MonoTextStyle::new(
                    &FONT_10X20,
                    if selected { BinaryColor::On } else { BinaryColor::Off },
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
    }

    pub fn move_selection(&self, rect: Rect, delta: i32, scrollbar_rect: Option<Rect>) -> Option<TableInteraction> {
        if self.model.rows.is_empty() {
            return None;
        }
        let old_selected = self.model.selected_row.map(|row| row as usize);
        let old_selected_col = self.model.selected_col.map(|col| col as usize);
        let mut new_selected = old_selected.unwrap_or(self.model.top_row as usize);
        if delta < 0 {
            if new_selected == 0 {
                return None;
            }
            new_selected = new_selected.saturating_sub(1);
        } else if delta > 0 {
            if new_selected + 1 >= self.model.rows.len() {
                return None;
            }
            new_selected += 1;
        } else {
            return None;
        }

        let mut top_row = self.model.top_row as usize;
        let visible_rows = self.visible_row_count_from(rect, top_row).max(1);
        if new_selected < top_row {
            top_row = new_selected;
        } else {
            let bottom = top_row.saturating_add(visible_rows.saturating_sub(1));
            if new_selected > bottom {
                top_row = new_selected.saturating_sub(visible_rows.saturating_sub(1));
            }
        }

        Some(self.interaction_for_state_change(
            rect,
            scrollbar_rect,
            old_selected,
            old_selected_col,
            Some(new_selected),
            old_selected_col,
            top_row,
            false,
        ))
    }

    pub fn move_selection_horizontal(
        &self,
        rect: Rect,
        delta: i32,
        scrollbar_rect: Option<Rect>,
    ) -> Option<TableInteraction> {
        if self.model.rows.is_empty() || self.model.cols <= 1 || delta == 0 {
            return None;
        }
        let row = self.model.selected_row.map(|row| row as usize)?;
        let old_col = self.model.selected_col.unwrap_or(0) as usize;
        let col_count = self.model.cols.max(1) as usize;
        let new_col = old_col as i32 + delta;
        if new_col < 0 || new_col >= col_count as i32 {
            return None;
        }
        Some(self.interaction_for_state_change(
            rect,
            scrollbar_rect,
            Some(row),
            Some(old_col),
            Some(row),
            Some(new_col as usize),
            self.model.top_row as usize,
            false,
        ))
    }

    pub fn activate_selection(&self, rect: Rect, scrollbar_rect: Option<Rect>) -> Option<TableInteraction> {
        let selected = self.model.selected_row.map(|row| row as usize)?;
        let selected_col = self.model.selected_col.map(|col| col as usize);
        Some(self.interaction_for_state_change(
            rect,
            scrollbar_rect,
            Some(selected),
            selected_col,
            Some(selected),
            selected_col,
            self.model.top_row as usize,
            true,
        ))
    }

    pub fn apply_scrollbar_hit(
        &self,
        rect: Rect,
        scrollbar_rect: Rect,
        hit: TableScrollBarHit,
    ) -> Option<TableInteraction> {
        let visible_rows = self.visible_row_count(rect).max(1);
        let max_top = self.model.rows.len().saturating_sub(visible_rows);
        let mut top_row = self.model.top_row as usize;
        match hit {
            TableScrollBarHit::ArrowUp => {
                if top_row == 0 {
                    return None;
                }
                top_row = top_row.saturating_sub(1);
            }
            TableScrollBarHit::ArrowDown => {
                if top_row >= max_top {
                    return None;
                }
                top_row += 1;
            }
            TableScrollBarHit::Track { top_row: hit_top } => {
                let next = hit_top.min(max_top);
                if next == top_row {
                    return None;
                }
                top_row = next;
            }
        }

        let old_selected = self.model.selected_row.map(|row| row as usize);
        let old_selected_col = self.model.selected_col.map(|col| col as usize);
        let mut selected = old_selected;
        if let Some(row) = selected {
            let bottom = top_row.saturating_add(visible_rows.saturating_sub(1));
            if row < top_row {
                selected = Some(top_row);
            } else if row > bottom {
                selected = Some(bottom.min(self.model.rows.len().saturating_sub(1)));
            }
        }

        Some(self.interaction_for_state_change(
            rect,
            Some(scrollbar_rect),
            old_selected,
            old_selected_col,
            selected,
            old_selected_col,
            top_row,
            false,
        ))
    }

    pub fn select_cell(
        &self,
        rect: Rect,
        scrollbar_rect: Option<Rect>,
        row: usize,
        col: usize,
        activated: bool,
    ) -> Option<TableInteraction> {
        let row = row.min(self.model.rows.len().saturating_sub(1));
        let old_selected = self.model.selected_row.map(|selected| selected as usize);
        let old_selected_col = self.model.selected_col.map(|selected| selected as usize);
        let col = col.min(self.model.cols.saturating_sub(1) as usize);
        Some(self.interaction_for_state_change(
            rect,
            scrollbar_rect,
            old_selected,
            old_selected_col,
            Some(row),
            Some(col),
            self.model.top_row as usize,
            activated,
        ))
    }

    fn interaction_for_state_change(
        &self,
        rect: Rect,
        scrollbar_rect: Option<Rect>,
        old_selected: Option<usize>,
        old_selected_col: Option<usize>,
        new_selected: Option<usize>,
        new_selected_col: Option<usize>,
        top_row: usize,
        activated: bool,
    ) -> TableInteraction {
        let mut dirty_rects = Vec::new();
        let top_changed = top_row != self.model.top_row as usize;
        if top_changed {
            dirty_rects.push(rect);
            if let Some(scrollbar_rect) = scrollbar_rect {
                dirty_rects.push(scrollbar_rect);
            }
        } else {
            if let Some(old_row) = old_selected.and_then(|row| self.row_rect(rect, row)) {
                dirty_rects.push(old_row);
            }
            if let Some(new_row) = new_selected.and_then(|row| self.row_rect_from(rect, top_row, row)) {
                dirty_rects.push(new_row);
            }
            if old_selected == new_selected && old_selected_col != new_selected_col {
                if let Some(row_rect) = new_selected.and_then(|row| self.row_rect_from(rect, top_row, row)) {
                    dirty_rects.push(row_rect);
                }
            }
        }
        TableInteraction {
            selected_row: new_selected,
            selected_col: new_selected_col,
            top_row,
            activated,
            dirty_rects,
        }
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

        let top_row = self.model.top_row as usize;
        for row_index in top_row..self.model.rows.len() {
            let Some(row_rect) = self.row_rect(rect, row_index) else {
                break;
            };
            self.render_row(ctx, rect, row_index);
            if row_rect.y + row_rect.h >= rect.y + rect.h {
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

fn wrap_palm_text_lines(
    text: &str,
    max_w: i32,
    font_id: u8,
    fonts: &[PalmFont],
) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current, word)
        };
        if palm_text_width(&candidate, font_id, fonts, 1) <= max_w || current.is_empty() {
            current = candidate;
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(text.to_string());
    }
    lines
}
