use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::StyledChars;
use crate::cell::{Cell, CellStyle};
use crate::color::{AnsiColor, ansi_cell_tuple_with_palette};
use crate::grid::{Grid, reflow_grid};

#[derive(Debug, Clone)]
pub struct Screen {
    primary: Grid,
    alternate: Grid,
    use_alternate: bool,
    scrollback: VecDeque<(Vec<Cell>, bool)>,
    scrollback_limit: usize,
    current_style: CellStyle,
    saved_cursor: Option<(usize, usize)>,
    scroll_region: Option<(usize, usize)>,
    dirty_rows: Vec<bool>,
    full_redraw: bool,
    version: u64,
    /// Cached result of `dump_text()` for the current `version`.
    text_cache: RefCell<Option<(u64, String)>>,
    /// Cached result of `dump_ansi()` for the current `version`.
    ansi_cache: RefCell<Option<(u64, String)>>,
}

#[derive(Debug, Clone)]
pub struct ScreenSnapshot {
    pub text: Arc<String>,
    pub version: u64,
    pub rows: usize,
    pub cols: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageRegion {
    pub full_redraw: bool,
    pub dirty_rows: Vec<usize>,
    pub version: u64,
}

fn emit_sgr_transition(out: &mut String, cur: &mut CellStyle, new: CellStyle) {
    use std::fmt::Write as _;

    if *cur == new {
        return;
    }
    out.push_str("\x1b[0");
    if new.bold {
        out.push_str(";1");
    }
    if new.italic {
        out.push_str(";3");
    }
    if new.underline {
        out.push_str(";4");
    }
    match new.fg {
        AnsiColor::Default => {}
        AnsiColor::Indexed(n) if n < 8 => {
            let _ = write!(out, ";{}", 30 + n as u32);
        }
        AnsiColor::Indexed(n) if n < 16 => {
            let _ = write!(out, ";{}", 90 + n as u32 - 8);
        }
        AnsiColor::Indexed(n) => {
            let _ = write!(out, ";38;5;{n}");
        }
        AnsiColor::TrueColor(r, g, b) => {
            let _ = write!(out, ";38;2;{r};{g};{b}");
        }
    }
    match new.bg {
        AnsiColor::Default => {}
        AnsiColor::Indexed(n) if n < 8 => {
            let _ = write!(out, ";{}", 40 + n as u32);
        }
        AnsiColor::Indexed(n) if n < 16 => {
            let _ = write!(out, ";{}", 100 + n as u32 - 8);
        }
        AnsiColor::Indexed(n) => {
            let _ = write!(out, ";48;5;{n}");
        }
        AnsiColor::TrueColor(r, g, b) => {
            let _ = write!(out, ";48;2;{r};{g};{b}");
        }
    }
    out.push('m');
    *cur = new;
}

fn encode_ansi_row(out: &mut String, cur: &mut CellStyle, cells: &[Cell]) {
    let last = cells
        .iter()
        .rposition(|c| c.ch != ' ' || c.style != CellStyle::default())
        .map(|i| i + 1)
        .unwrap_or(0);
    for col in 0..last {
        let cell = cells.get(col).copied().unwrap_or_default();
        emit_sgr_transition(out, cur, cell.style);
        out.push(cell.ch);
    }
    if *cur != CellStyle::default() {
        out.push_str("\x1b[0m");
        *cur = CellStyle::default();
    }
}

impl Screen {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            primary: Grid::new(rows, cols),
            alternate: Grid::new(rows, cols),
            use_alternate: false,
            scrollback: VecDeque::new(),
            scrollback_limit: 10_000,
            current_style: CellStyle::default(),
            saved_cursor: None,
            scroll_region: None,
            dirty_rows: vec![true; rows],
            full_redraw: true,
            version: 1,
            text_cache: RefCell::new(None),
            ansi_cache: RefCell::new(None),
        }
    }

    fn active_grid(&self) -> &Grid {
        if self.use_alternate {
            &self.alternate
        } else {
            &self.primary
        }
    }

    fn active_grid_mut(&mut self) -> &mut Grid {
        if self.use_alternate {
            &mut self.alternate
        } else {
            &mut self.primary
        }
    }

    /// Returns the current cursor row (0-based) in the active grid.
    pub fn cursor_row(&self) -> usize {
        self.active_grid().cursor_row
    }

    /// Returns the current cursor column (0-based) in the active grid.
    pub fn cursor_col(&self) -> usize {
        self.active_grid().cursor_col
    }

    /// Returns whether the alternate screen buffer is currently active.
    pub fn is_alternate_screen(&self) -> bool {
        self.use_alternate
    }

    /// Monotonic version counter: incremented on every write to the screen.
    /// Callers can compare against a previously stored value to check whether
    /// the screen content has changed since the last snapshot.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn put_char(&mut self, ch: char) {
        let style = self.current_style;
        if self.use_alternate {
            if self.alternate.pending_wrap {
                self.alternate.pending_wrap = false;
                self.alternate.cursor_col = 0;
                self.alternate.cursor_row += 1;
            }
            if self.alternate.cursor_row >= self.alternate.rows {
                let _ = self.alternate.scroll_up_one();
                self.alternate.cursor_row = self.alternate.rows.saturating_sub(1);
            }
            let row = self.alternate.cursor_row;
            self.alternate.put_char(ch, style);
            self.mark_dirty_row(row);
        } else {
            if self.primary.pending_wrap {
                let wrap_row = self.primary.cursor_row;
                if wrap_row < self.primary.line_wrapped.len() {
                    self.primary.line_wrapped[wrap_row] = true;
                }
                self.primary.pending_wrap = false;
                self.primary.cursor_col = 0;
                self.primary.cursor_row += 1;
            }
            if self.primary.cursor_row >= self.primary.rows {
                let (popped, wrapped) = self.primary.scroll_up_one();
                self.primary.cursor_row = self.primary.rows.saturating_sub(1);
                self.push_scrollback(popped, wrapped);
            }
            let row = self.primary.cursor_row;
            self.primary.put_char(ch, style);
            self.mark_dirty_row(row);
        }
        self.bump_version();
    }

    pub fn linefeed(&mut self) {
        let region = self.scroll_region;
        if self.use_alternate {
            let grid = &mut self.alternate;
            if let Some((top, bottom)) = region {
                if grid.cursor_row < top || grid.cursor_row > bottom {
                    if grid.cursor_row + 1 < grid.rows {
                        grid.cursor_row += 1;
                    } else {
                        let _ = grid.scroll_up_one();
                    }
                } else if grid.cursor_row < bottom {
                    grid.cursor_row += 1;
                } else {
                    Self::scroll_region_up(grid, top, bottom);
                    self.mark_full_redraw();
                }
            } else if grid.cursor_row + 1 < grid.rows {
                grid.cursor_row += 1;
            } else {
                let _ = grid.scroll_up_one();
                self.mark_full_redraw();
            }
        } else {
            if self.primary.cursor_row + 1 < self.primary.rows {
                self.primary.cursor_row += 1;
            } else {
                let (popped, wrapped) = self.primary.scroll_up_one();
                self.push_scrollback(popped, wrapped);
                self.mark_full_redraw();
            }
        }
        self.bump_version();
    }

    pub fn carriage_return(&mut self) {
        let grid = self.active_grid_mut();
        grid.cursor_col = 0;
        grid.pending_wrap = false;
        self.bump_version();
    }

    pub fn backspace(&mut self) {
        let mut dirty_row = None;
        {
            let grid = self.active_grid_mut();
            if grid.cursor_col > 0 {
                grid.cursor_col -= 1;
                let idx = grid.cursor_row * grid.cols + grid.cursor_col;
                grid.cells[idx] = Cell::default();
                dirty_row = Some(grid.cursor_row);
            }
        }
        if let Some(row) = dirty_row {
            self.mark_dirty_row(row);
        }
        self.bump_version();
    }

    pub fn cursor_up(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let delta = n as usize;
        grid.cursor_row = grid.cursor_row.saturating_sub(delta);
        self.bump_version();
    }

    pub fn cursor_down(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let delta = n as usize;
        grid.cursor_row = (grid.cursor_row + delta).min(grid.rows.saturating_sub(1));
        self.bump_version();
    }

    pub fn cursor_forward(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let delta = n as usize;
        grid.cursor_col = (grid.cursor_col + delta).min(grid.cols.saturating_sub(1));
        self.bump_version();
    }

    pub fn cursor_backward(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let delta = n as usize;
        grid.cursor_col = grid.cursor_col.saturating_sub(delta);
        self.bump_version();
    }

    pub fn cursor_position(&mut self, row_1based: u16, col_1based: u16) {
        let grid = self.active_grid_mut();
        grid.cursor_row = row_1based.saturating_sub(1) as usize;
        grid.cursor_col = col_1based.saturating_sub(1) as usize;
        grid.clamp_cursor();
        self.bump_version();
    }

    pub fn erase_in_display(&mut self, mode: u16) {
        let mut dirty_rows = Vec::new();
        let mut full = false;
        let grid = self.active_grid_mut();
        match mode {
            0 => {
                let row = grid.cursor_row;
                grid.clear_range_in_row(row, grid.cursor_col, grid.cols);
                dirty_rows.push(row);
                for r in row + 1..grid.rows {
                    grid.clear_row(r);
                    dirty_rows.push(r);
                }
            }
            1 => {
                let row = grid.cursor_row;
                grid.clear_range_in_row(row, 0, grid.cursor_col + 1);
                dirty_rows.push(row);
                for r in 0..row {
                    grid.clear_row(r);
                    dirty_rows.push(r);
                }
            }
            2 => {
                grid.clear_all();
                full = true;
            }
            _ => {}
        }
        if full {
            self.mark_full_redraw();
        } else {
            for row in dirty_rows {
                self.mark_dirty_row(row);
            }
        }
        self.bump_version();
    }

    pub fn erase_in_line(&mut self, mode: u16) {
        let row = {
            let grid = self.active_grid_mut();
            let row = grid.cursor_row;
            match mode {
                0 => grid.clear_range_in_row(row, grid.cursor_col, grid.cols),
                1 => grid.clear_range_in_row(row, 0, grid.cursor_col + 1),
                2 => grid.clear_row(row),
                _ => {}
            }
            row
        };
        self.mark_dirty_row(row);
        self.bump_version();
    }

    pub fn set_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.current_style = CellStyle::default();
            return;
        }

        let mut i = 0;
        while i < params.len() {
            match params[i] {
                0 => self.current_style = CellStyle::default(),
                1 => self.current_style.bold = true,
                2 => self.current_style.dim = true,
                3 => self.current_style.italic = true,
                4 => self.current_style.underline = true,
                7 => self.current_style.reverse = true,
                9 => self.current_style.strikethrough = true,
                22 => {
                    self.current_style.bold = false;
                    self.current_style.dim = false;
                }
                23 => self.current_style.italic = false,
                24 => self.current_style.underline = false,
                27 => self.current_style.reverse = false,
                29 => self.current_style.strikethrough = false,
                30..=37 => self.current_style.fg = AnsiColor::Indexed((params[i] - 30) as u8),
                38 => match (params.get(i + 1), params.get(i + 2)) {
                    (Some(&5), Some(&n)) => {
                        self.current_style.fg = AnsiColor::Indexed(n as u8);
                        i += 2;
                    }
                    (Some(&2), _) if params.len() > i + 4 => {
                        self.current_style.fg = AnsiColor::TrueColor(
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                            params[i + 4] as u8,
                        );
                        i += 4;
                    }
                    _ => {}
                },
                39 => self.current_style.fg = AnsiColor::Default,
                40..=47 => self.current_style.bg = AnsiColor::Indexed((params[i] - 40) as u8),
                48 => match (params.get(i + 1), params.get(i + 2)) {
                    (Some(&5), Some(&n)) => {
                        self.current_style.bg = AnsiColor::Indexed(n as u8);
                        i += 2;
                    }
                    (Some(&2), _) if params.len() > i + 4 => {
                        self.current_style.bg = AnsiColor::TrueColor(
                            params[i + 2] as u8,
                            params[i + 3] as u8,
                            params[i + 4] as u8,
                        );
                        i += 4;
                    }
                    _ => {}
                },
                49 => self.current_style.bg = AnsiColor::Default,
                90..=97 => self.current_style.fg = AnsiColor::Indexed((params[i] - 90 + 8) as u8),
                100..=107 => {
                    self.current_style.bg = AnsiColor::Indexed((params[i] - 100 + 8) as u8)
                }
                _ => {}
            }
            i += 1;
        }
        self.bump_version();
    }

    pub fn set_alternate_screen(&mut self, enabled: bool) {
        self.use_alternate = enabled;
        self.mark_full_redraw();
        self.bump_version();
    }

    pub fn horizontal_tab(&mut self) {
        let grid = self.active_grid_mut();
        let next = ((grid.cursor_col / 8) + 1) * 8;
        grid.cursor_col = next.min(grid.cols.saturating_sub(1));
        self.bump_version();
    }

    pub fn save_cursor(&mut self) {
        let grid = self.active_grid();
        self.saved_cursor = Some((grid.cursor_row, grid.cursor_col));
        self.bump_version();
    }

    pub fn restore_cursor(&mut self) {
        if let Some((row, col)) = self.saved_cursor {
            let grid = self.active_grid_mut();
            grid.cursor_row = row.min(grid.rows.saturating_sub(1));
            grid.cursor_col = col.min(grid.cols.saturating_sub(1));
        }
        self.bump_version();
    }

    pub fn set_scroll_region(&mut self, top_1based: u16, bottom_1based: u16) {
        let rows = self.active_grid().rows;
        let top = top_1based.saturating_sub(1) as usize;
        let mut bottom = if bottom_1based == 0 {
            rows.saturating_sub(1)
        } else {
            bottom_1based.saturating_sub(1) as usize
        };

        if bottom >= rows {
            bottom = rows.saturating_sub(1);
        }

        if top < bottom {
            self.scroll_region = Some((top, bottom));
            self.cursor_position((top + 1) as u16, 1);
        } else {
            self.scroll_region = None;
        }
        self.bump_version();
    }

    pub fn insert_chars(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let row = grid.cursor_row;
        let col = grid.cursor_col;
        let count = n as usize;

        for c in (col..grid.cols).rev() {
            let src = if c >= col + count { c - count } else { col };
            let dst_idx = row * grid.cols + c;
            let src_idx = row * grid.cols + src;
            grid.cells[dst_idx] = grid.cells[src_idx];
        }
        for c in col..(col + count).min(grid.cols) {
            let idx = row * grid.cols + c;
            grid.cells[idx] = Cell::default();
        }
        self.mark_dirty_row(row);
        self.bump_version();
    }

    pub fn delete_chars(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let row = grid.cursor_row;
        let col = grid.cursor_col;
        let count = n as usize;

        for c in col..grid.cols {
            let src = c + count;
            let dst_idx = row * grid.cols + c;
            if src < grid.cols {
                let src_idx = row * grid.cols + src;
                grid.cells[dst_idx] = grid.cells[src_idx];
            } else {
                grid.cells[dst_idx] = Cell::default();
            }
        }
        self.mark_dirty_row(row);
        self.bump_version();
    }

    pub fn insert_lines(&mut self, n: u16) {
        let count = n as usize;
        let region = self.effective_region();
        let (top, bottom) = region;
        let grid = self.active_grid_mut();
        let row = grid.cursor_row.clamp(top, bottom);

        for _ in 0..count {
            for r in (row + 1..=bottom).rev() {
                Self::copy_row(grid, r - 1, r);
            }
            grid.clear_row(row);
        }
        self.mark_full_redraw();
        self.bump_version();
    }

    pub fn delete_lines(&mut self, n: u16) {
        let count = n as usize;
        let region = self.effective_region();
        let (top, bottom) = region;
        let grid = self.active_grid_mut();
        let row = grid.cursor_row.clamp(top, bottom);

        for _ in 0..count {
            for r in row..bottom {
                Self::copy_row(grid, r + 1, r);
            }
            grid.clear_row(bottom);
        }
        self.mark_full_redraw();
        self.bump_version();
    }

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        if new_rows == 0 || new_cols == 0 {
            return;
        }
        let old_rows = self.primary.rows;
        let old_cols = self.primary.cols;

        self.alternate.resize(new_rows, new_cols);

        let mut reflow_rows: Vec<(Vec<Cell>, bool)> = if new_cols != old_cols {
            reflow_grid(&self.primary, new_cols)
        } else {
            (0..old_rows)
                .map(|row| {
                    (
                        self.primary.row_cells(row),
                        self.primary.line_wrapped.get(row).copied().unwrap_or(false),
                    )
                })
                .collect()
        };

        let reflow_len = reflow_rows.len();
        let mut new_cells = vec![Cell::default(); new_rows * new_cols];
        let mut new_line_wrapped = vec![false; new_rows];
        let new_cursor_row;

        if reflow_len >= new_rows {
            let excess = reflow_len - new_rows;
            for (cells, wrapped) in reflow_rows.drain(0..excess) {
                self.push_scrollback(cells, wrapped);
            }
            for (dst_row, (cells, wrapped)) in reflow_rows.into_iter().enumerate().take(new_rows) {
                let dst = dst_row * new_cols;
                for (col, cell) in cells.into_iter().enumerate().take(new_cols) {
                    new_cells[dst + col] = cell;
                }
                new_line_wrapped[dst_row] = wrapped;
            }
            new_cursor_row = new_rows.saturating_sub(1);
        } else {
            let shortfall = new_rows - reflow_len;
            let to_pull = shortfall.min(self.scrollback.len());
            let sb_start = shortfall - to_pull;
            let vis_start = shortfall;

            let drained_start = self.scrollback.len().saturating_sub(to_pull);
            let pulled: Vec<(Vec<Cell>, bool)> = self.scrollback.drain(drained_start..).collect();
            for (i, (sb_cells, sb_wrapped)) in pulled.into_iter().enumerate() {
                let dst_row = sb_start + i;
                let dst = dst_row * new_cols;
                for (col, cell) in sb_cells.iter().enumerate().take(new_cols) {
                    new_cells[dst + col] = *cell;
                }
                new_line_wrapped[dst_row] = sb_wrapped;
            }

            for (vi, (cells, wrapped)) in reflow_rows.iter().enumerate() {
                let dst_row = vis_start + vi;
                let dst = dst_row * new_cols;
                for (col, &cell) in cells.iter().enumerate().take(new_cols) {
                    new_cells[dst + col] = cell;
                }
                new_line_wrapped[dst_row] = *wrapped;
            }

            let approx = vis_start + self.primary.cursor_row.min(reflow_len.saturating_sub(1));
            new_cursor_row = approx.min(new_rows.saturating_sub(1));
        }

        self.primary.cells = new_cells;
        self.primary.rows = new_rows;
        self.primary.cols = new_cols;
        self.primary.line_wrapped = new_line_wrapped;
        self.primary.cursor_row = new_cursor_row;
        self.primary.cursor_col = self.primary.cursor_col.min(new_cols.saturating_sub(1));
        self.primary.pending_wrap = false;
        self.scroll_region = None;
        self.dirty_rows = vec![true; new_rows];
        self.full_redraw = true;
        self.bump_version();
    }

    pub fn dump_text(&self) -> String {
        let mut cache = self.text_cache.borrow_mut();
        if let Some((cached_version, ref text)) = *cache
            && cached_version == self.version
        {
            return text.clone();
        }
        let text = self.active_grid().dump_text();
        *cache = Some((self.version, text.clone()));
        text
    }

    /// Encodes the scrollback + visible grid as a string of ANSI SGR escape
    /// sequences so that fg/bg colors and text attributes are preserved when
    /// fed back through the terminal parser on next launch.
    pub fn dump_ansi(&self) -> String {
        {
            let cache = self.ansi_cache.borrow();
            if let Some((cached_version, ref text)) = *cache
                && cached_version == self.version
            {
                return text.clone();
            }
        }
        let grid = &self.primary;
        let cols = grid.cols;
        let mut out = String::new();
        let mut cur = CellStyle::default();

        // Scrollback rows (oldest first).
        for (row_cells, _wrapped) in &self.scrollback {
            let len = row_cells.len().min(cols);
            encode_ansi_row(&mut out, &mut cur, &row_cells[..len]);
            out.push('\n');
        }

        // Visible grid rows.
        for row in 0..grid.rows {
            let start = row * cols;
            encode_ansi_row(&mut out, &mut cur, &grid.cells[start..start + cols]);
            if row + 1 < grid.rows {
                out.push('\n');
            }
        }

        *self.ansi_cache.borrow_mut() = Some((self.version, out.clone()));
        out
    }

    pub fn dump_styled(&self) -> StyledChars {
        self.dump_styled_at_offset_with_palette(0, None)
    }

    pub fn dump_styled_at_offset(&self, scroll_offset: usize) -> StyledChars {
        self.dump_styled_at_offset_with_palette(scroll_offset, None)
    }

    /// Like `dump_styled_at_offset` but overrides ANSI indexed colors 0-15
    /// using `palette` when provided.
    pub fn dump_styled_at_offset_with_palette(
        &self,
        scroll_offset: usize,
        palette: Option<&[[f32; 3]; 16]>,
    ) -> StyledChars {
        let grid = self.active_grid();
        let rows = grid.rows;
        let cols = grid.cols;
        let sb_len = self.scrollback.len();
        let offset = scroll_offset.min(sb_len);

        let mut result = Vec::with_capacity(rows * (cols + 1));
        for display_row in 0..rows {
            if display_row < offset {
                let sb_idx = sb_len - offset + display_row;
                let (sb_row, _) = &self.scrollback[sb_idx];
                for col in 0..cols {
                    let cell = sb_row.get(col).copied().unwrap_or_default();
                    result.push(ansi_cell_tuple_with_palette(&cell, palette));
                }
            } else {
                let grid_row = display_row - offset;
                for col in 0..cols {
                    let cell = &grid.cells[grid_row * cols + col];
                    result.push(ansi_cell_tuple_with_palette(cell, palette));
                }
            }
            if display_row + 1 < rows {
                result.push(('\n', None, None, 0u8));
            }
        }
        result
    }

    pub fn snapshot(&self) -> ScreenSnapshot {
        let grid = self.active_grid();
        ScreenSnapshot {
            text: Arc::new(grid.dump_text()),
            version: self.version,
            rows: grid.rows,
            cols: grid.cols,
        }
    }

    pub fn take_damage(&mut self) -> DamageRegion {
        let mut dirty_rows = Vec::new();
        for (row, dirty) in self.dirty_rows.iter_mut().enumerate() {
            if *dirty {
                dirty_rows.push(row);
                *dirty = false;
            }
        }

        let damage = DamageRegion {
            full_redraw: self.full_redraw,
            dirty_rows,
            version: self.version,
        };
        metrics::histogram!("screen_damage_rows").record(damage.dirty_rows.len() as f64);
        self.full_redraw = false;
        damage
    }

    fn push_scrollback(&mut self, row: Vec<Cell>, wrapped: bool) {
        if self.scrollback.len() >= self.scrollback_limit {
            self.scrollback.pop_front();
        }
        self.scrollback.push_back((row, wrapped));
    }

    fn mark_dirty_row(&mut self, row: usize) {
        if let Some(slot) = self.dirty_rows.get_mut(row) {
            *slot = true;
        }
    }

    fn mark_full_redraw(&mut self) {
        self.full_redraw = true;
        for row in &mut self.dirty_rows {
            *row = true;
        }
    }

    fn bump_version(&mut self) {
        self.version = self.version.saturating_add(1);
    }

    fn effective_region(&self) -> (usize, usize) {
        self.scroll_region
            .unwrap_or((0, self.active_grid().rows.saturating_sub(1)))
    }

    fn copy_row(grid: &mut Grid, src_row: usize, dst_row: usize) {
        for col in 0..grid.cols {
            let src = src_row * grid.cols + col;
            let dst = dst_row * grid.cols + col;
            grid.cells[dst] = grid.cells[src];
        }
        if src_row < grid.line_wrapped.len() && dst_row < grid.line_wrapped.len() {
            grid.line_wrapped[dst_row] = grid.line_wrapped[src_row];
        }
    }

    fn scroll_region_up(grid: &mut Grid, top: usize, bottom: usize) {
        for row in top..bottom {
            Self::copy_row(grid, row + 1, row);
        }
        grid.clear_row(bottom);
    }
}

#[cfg(test)]
mod tests {
    use super::Screen;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn resize_keeps_cursor_in_bounds(new_rows in 1usize..16, new_cols in 1usize..16) {
            let mut screen = Screen::new(3, 3);
            screen.put_char('a');
            screen.put_char('b');
            screen.cursor_position(3, 3);

            screen.resize(new_rows, new_cols);

            prop_assert!(screen.cursor_row() < new_rows);
            prop_assert!(screen.cursor_col() < new_cols);
        }
    }

    #[test]
    fn writes_text_and_newline() {
        let mut screen = Screen::new(2, 4);
        for ch in "ab".chars() {
            screen.put_char(ch);
        }
        screen.linefeed();
        screen.carriage_return();
        for ch in "cd".chars() {
            screen.put_char(ch);
        }

        assert_eq!(screen.dump_text(), "ab  \ncd  ");
    }

    #[test]
    fn supports_alt_screen_toggle() {
        let mut screen = Screen::new(2, 4);
        screen.put_char('x');
        screen.set_alternate_screen(true);
        screen.put_char('y');
        assert!(screen.dump_text().contains('y'));

        screen.set_alternate_screen(false);
        assert!(screen.dump_text().contains('x'));
    }

    #[test]
    fn alternate_screen_accessor_tracks_toggle() {
        let mut screen = Screen::new(2, 4);
        assert!(!screen.is_alternate_screen());
        screen.set_alternate_screen(true);
        assert!(screen.is_alternate_screen());
        screen.set_alternate_screen(false);
        assert!(!screen.is_alternate_screen());
    }

    #[test]
    fn scrolls_with_scrollback() {
        let mut screen = Screen::new(2, 2);
        for ch in "abcd".chars() {
            screen.put_char(ch);
        }
        screen.linefeed();
        screen.carriage_return();
        for ch in "ef".chars() {
            screen.put_char(ch);
        }

        assert!(screen.scrollback_len() >= 1);
    }

    #[test]
    fn supports_save_restore_cursor() {
        let mut screen = Screen::new(2, 8);
        screen.put_char('a');
        screen.save_cursor();
        screen.cursor_position(2, 1);
        screen.put_char('b');
        screen.restore_cursor();
        screen.put_char('c');

        assert!(screen.dump_text().contains("ac"));
    }

    #[test]
    fn supports_tab_and_insert_delete_chars() {
        let mut screen = Screen::new(1, 16);
        screen.put_char('a');
        screen.horizontal_tab();
        screen.put_char('b');
        screen.cursor_position(1, 2);
        screen.insert_chars(2);
        screen.put_char('x');
        screen.cursor_position(1, 9);
        screen.delete_chars(1);

        let line = screen.dump_text();
        assert!(line.contains('x'));
        assert!(line.contains('b'));
    }

    #[test]
    fn exposes_damage_and_snapshot() {
        let mut screen = Screen::new(2, 4);
        let initial_damage = screen.take_damage();
        assert!(initial_damage.full_redraw);

        screen.put_char('z');
        let damage = screen.take_damage();
        assert!(!damage.dirty_rows.is_empty());

        let snap = screen.snapshot();
        assert!(snap.text.contains('z'));
    }

    #[test]
    fn resize_clamps_cursor_after_wide_chars() {
        let mut screen = Screen::new(2, 4);
        screen.put_char('你');
        screen.put_char('好');
        screen.cursor_position(2, 4);

        screen.resize(2, 2);

        assert!(screen.cursor_row() < 2);
        assert!(screen.cursor_col() < 2);
    }
}
