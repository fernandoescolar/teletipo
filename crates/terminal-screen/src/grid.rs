use unicode_width::UnicodeWidthChar as _;

use crate::cell::Cell;

#[derive(Debug, Clone)]
pub(crate) struct Grid {
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    pub(crate) cells: Vec<Cell>,
    pub(crate) cursor_col: usize,
    pub(crate) cursor_row: usize,
    /// Auto-wrap pending flag (deferred wrap).
    pub(crate) pending_wrap: bool,
    /// Per-row wrap flag.
    pub(crate) line_wrapped: Vec<bool>,
}

impl Grid {
    pub(crate) fn new(rows: usize, cols: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![Cell::default(); rows * cols],
            cursor_col: 0,
            cursor_row: 0,
            pending_wrap: false,
            line_wrapped: vec![false; rows],
        }
    }

    pub(crate) fn put_char(&mut self, ch: char, style: crate::cell::CellStyle) {
        if self.cursor_row >= self.rows || self.cursor_col >= self.cols {
            return;
        }

        // Unicode display width: 2 for emoji and CJK wide chars, 1 for everything else.
        let char_width = ch.width().unwrap_or(1).max(1);

        let idx = self.cursor_row * self.cols + self.cursor_col;
        self.cells[idx] = Cell { ch, style };
        self.pending_wrap = false;

        // For wide characters (display width 2) fill the trailing cell with a
        // null placeholder so the renderer skips it without advancing columns.
        if char_width == 2 && self.cursor_col + 1 < self.cols {
            self.cells[idx + 1] = Cell {
                ch: '\0',
                style: crate::cell::CellStyle::default(),
            };
        }

        self.cursor_col += char_width;
        if self.cursor_col >= self.cols {
            self.cursor_col = self.cols - 1;
            self.pending_wrap = true;
        }
    }

    pub(crate) fn row_cells(&self, row: usize) -> Vec<Cell> {
        let start = row * self.cols;
        self.cells[start..start + self.cols].to_vec()
    }

    pub(crate) fn clear_row(&mut self, row: usize) {
        let start = row * self.cols;
        for cell in &mut self.cells[start..start + self.cols] {
            *cell = Cell::default();
        }
        if row < self.line_wrapped.len() {
            self.line_wrapped[row] = false;
        }
    }

    pub(crate) fn clear_range_in_row(&mut self, row: usize, start_col: usize, end_col: usize) {
        let start = row * self.cols + start_col;
        let end = row * self.cols + end_col;
        for cell in &mut self.cells[start..end] {
            *cell = Cell::default();
        }
    }

    pub(crate) fn clear_all(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
    }

    pub(crate) fn clamp_cursor(&mut self) {
        self.cursor_row = self.cursor_row.min(self.rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(self.cols.saturating_sub(1));
    }

    pub(crate) fn scroll_up_one(&mut self) -> (Vec<Cell>, bool) {
        let first_row = self.row_cells(0);
        let first_wrapped = self.line_wrapped.first().copied().unwrap_or(false);
        for row in 1..self.rows {
            let src_start = row * self.cols;
            let dst_start = (row - 1) * self.cols;
            for offset in 0..self.cols {
                self.cells[dst_start + offset] = self.cells[src_start + offset];
            }
            if row < self.line_wrapped.len() {
                let w = self.line_wrapped[row];
                self.line_wrapped[row - 1] = w;
            }
        }
        self.clear_row(self.rows - 1);
        (first_row, first_wrapped)
    }

    pub(crate) fn dump_text(&self) -> String {
        let mut out = String::with_capacity((self.cols + 1) * self.rows);
        for row in 0..self.rows {
            for col in 0..self.cols {
                out.push(self.cells[row * self.cols + col].ch);
            }
            if row + 1 < self.rows {
                out.push('\n');
            }
        }
        out
    }

    pub(crate) fn resize(&mut self, new_rows: usize, new_cols: usize) {
        if new_rows == self.rows && new_cols == self.cols {
            return;
        }
        let mut new_cells = vec![Cell::default(); new_rows * new_cols];
        let mut new_line_wrapped = vec![false; new_rows];
        let copy_rows = self.rows.min(new_rows);
        let copy_cols = self.cols.min(new_cols);
        for row in 0..copy_rows {
            for col in 0..copy_cols {
                new_cells[row * new_cols + col] = self.cells[row * self.cols + col];
            }
            new_line_wrapped[row] = self.line_wrapped.get(row).copied().unwrap_or(false);
        }
        self.cells = new_cells;
        self.rows = new_rows;
        self.cols = new_cols;
        self.line_wrapped = new_line_wrapped;
        self.pending_wrap = false;
        self.clamp_cursor();
    }
}

/// Split a single logical line's cells into visual rows of `cols` cells and push
/// them into `out`. Each row is paired with a `line_wrapped` flag.
pub(crate) fn emit_logical_line(cells: &[Cell], cols: usize, out: &mut Vec<(Vec<Cell>, bool)>) {
    if cells.is_empty() {
        out.push((vec![Cell::default(); cols], false));
        return;
    }
    let chunks: Vec<&[Cell]> = cells.chunks(cols).collect();
    let n = chunks.len();
    for (i, chunk) in chunks.iter().enumerate() {
        let mut row = vec![Cell::default(); cols];
        for (col, &cell) in chunk.iter().enumerate() {
            row[col] = cell;
        }
        out.push((row, i + 1 < n));
    }
}

/// Reflow the visible grid to `new_cols`.
pub(crate) fn reflow_grid(grid: &Grid, new_cols: usize) -> Vec<(Vec<Cell>, bool)> {
    let mut result: Vec<(Vec<Cell>, bool)> = Vec::new();
    let mut current_logical: Vec<Cell> = Vec::new();

    for row in 0..grid.rows {
        let wrapped = grid.line_wrapped.get(row).copied().unwrap_or(false);
        let row_cells = &grid.cells[row * grid.cols..(row + 1) * grid.cols];
        let content_end = row_cells
            .iter()
            .rposition(|c| c.ch != ' ')
            .map(|i| i + 1)
            .unwrap_or(0);
        current_logical.extend_from_slice(&row_cells[..content_end]);
        if !wrapped {
            emit_logical_line(&current_logical, new_cols, &mut result);
            current_logical.clear();
        }
    }
    if !current_logical.is_empty() {
        emit_logical_line(&current_logical, new_cols, &mut result);
    }
    result
}
