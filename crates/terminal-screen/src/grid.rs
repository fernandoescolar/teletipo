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

    pub(crate) fn put_char(&mut self, ch: char, style: crate::cell::CellStyle, hyperlink_id: u16) {
        if self.cursor_row >= self.rows || self.cursor_col >= self.cols {
            return;
        }

        // Unicode display width: 2 for emoji and CJK wide chars, 1 for everything else.
        let char_width = ch.width().unwrap_or(1).max(1);

        let idx = self.cursor_row * self.cols + self.cursor_col;
        self.cells[idx] = Cell {
            ch,
            style,
            hyperlink_id,
        };
        self.pending_wrap = false;

        // For wide characters (display width 2) fill the trailing cell with a
        // null placeholder so the renderer skips it without advancing columns.
        if char_width == 2 && self.cursor_col + 1 < self.cols {
            self.cells[idx + 1] = Cell {
                ch: '\0',
                style: crate::cell::CellStyle::default(),
                hyperlink_id: 0,
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

fn emit_logical_line(cells: &[Cell], cols: usize, out: &mut Vec<(Vec<Cell>, bool)>) {
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

/// Reflow the visible grid to `new_cols`, also mapping `(cursor_row,
/// cursor_col)` to the equivalent position in the reflowed output.
///
/// Returns `(rows, new_cursor_row, new_cursor_col)`.
pub(crate) fn reflow_grid(
    grid: &Grid,
    new_cols: usize,
    cursor_row: usize,
    cursor_col: usize,
) -> (Vec<(Vec<Cell>, bool)>, usize, usize) {
    let mut result: Vec<(Vec<Cell>, bool)> = Vec::new();
    let mut new_cursor_row = 0usize;
    let mut new_cursor_col = 0usize;

    let mut row = 0;
    while row < grid.rows {
        let logical_result_start = result.len();
        let mut logical_cells: Vec<Cell> = Vec::new();
        let mut cursor_logical_offset: Option<usize> = None;

        // Accumulate wrapped rows into one logical line.
        loop {
            let wrapped = grid.line_wrapped.get(row).copied().unwrap_or(false);
            let row_cells = &grid.cells[row * grid.cols..(row + 1) * grid.cols];
            let content_end = row_cells
                .iter()
                .rposition(|c| c.ch != ' ')
                .map(|i| i + 1)
                .unwrap_or(0);

            if row == cursor_row {
                // Cursor offset within this logical line = cells already
                // accumulated + the cursor column (clamped to content).
                cursor_logical_offset =
                    Some(logical_cells.len() + cursor_col.min(content_end));
            }

            logical_cells.extend_from_slice(&row_cells[..content_end]);
            row += 1;
            if !wrapped || row >= grid.rows {
                break;
            }
        }

        emit_logical_line(&logical_cells, new_cols, &mut result);

        if let Some(off) = cursor_logical_offset {
            new_cursor_row = logical_result_start + off / new_cols.max(1);
            new_cursor_col = off % new_cols.max(1);
        }
    }

    (result, new_cursor_row, new_cursor_col)
}

