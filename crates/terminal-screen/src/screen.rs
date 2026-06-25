use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::StyledChars;
use crate::cell::{Cell, CellStyle};
use crate::color::{AnsiColor, ansi_cell_tuple_with_palette};
use crate::grid::{Grid, reflow_grid};
use crate::hyperlink::HyperlinkInterner;

/// Terminal screen model: cell grid, scrollback, cursor, and damage tracking.
///
/// Applies ANSI parser actions to a primary/alternate grid pair and exposes
/// snapshots, ANSI re-emission, and per-row damage flags for the renderer.
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
    /// Intern table for OSC 8 hyperlink URIs. Shared across primary and
    /// scrollback; cleared when the alternate screen is entered/exited so
    /// stale IDs cannot bleed through.
    pub(crate) hyperlinks: HyperlinkInterner,
    /// The hyperlink ID to stamp on the next printed character.
    /// 0 means "no active hyperlink".
    pub(crate) current_hyperlink_id: u16,
    /// Cached result of `dump_text()` for the current `version`.
    text_cache: RefCell<Option<(u64, String)>>,
    // Note: held as Arc<String> so callers can clone the handle in O(1) instead
    // of copying potentially-large terminal dumps on every hit. See PERF-1.
    /// Cached result of `dump_ansi()` for the current `version`.
    ansi_cache: RefCell<Option<(u64, Arc<String>)>>,
}

/// Lightweight, cloneable view of the visible grid for the renderer.
///
/// `text` is the flattened character buffer; `version` is the monotonic
/// counter from the source [`Screen`] so callers can skip work when it has
/// not advanced.
#[derive(Debug, Clone)]
pub struct ScreenSnapshot {
    /// Flattened visible grid contents.
    pub text: Arc<String>,
    /// Monotonic version counter from the source `Screen`.
    pub version: u64,
    /// Row count at the time the snapshot was taken.
    pub rows: usize,
    /// Column count at the time the snapshot was taken.
    pub cols: usize,
}

/// Per-frame description of what the screen changed since the last render.
///
/// `full_redraw` overrides per-row flags; `dirty_rows` lists row indices that
/// must be re-rasterised; `version` matches the source `Screen` version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageRegion {
    /// If true the renderer should rebuild every row this frame.
    pub full_redraw: bool,
    /// Indices of rows that changed since the last `take_damage`.
    pub dirty_rows: Vec<usize>,
    /// Source `Screen` version this damage was produced for.
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
    /// Default scrollback line cap. Kept moderate (5 000 lines) so a single
    /// terminal stays under ~20 MB even with wide rows; users who need more
    /// history can raise it via `set_scrollback_limit`.
    pub const DEFAULT_SCROLLBACK_LIMIT: usize = 5_000;

    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            primary: Grid::new(rows, cols),
            alternate: Grid::new(rows, cols),
            use_alternate: false,
            scrollback: VecDeque::new(),
            scrollback_limit: Self::DEFAULT_SCROLLBACK_LIMIT,
            current_style: CellStyle::default(),
            saved_cursor: None,
            scroll_region: None,
            dirty_rows: vec![true; rows],
            full_redraw: true,
            version: 1,
            hyperlinks: HyperlinkInterner::default(),
            current_hyperlink_id: 0,
            text_cache: RefCell::new(None),
            ansi_cache: RefCell::new(None),
        }
    }

    /// Override the scrollback line cap. If the new limit is smaller than the
    /// current backlog, the oldest lines are dropped immediately so memory is
    /// reclaimed on the next allocator pass.
    pub fn set_scrollback_limit(&mut self, limit: usize) {
        self.scrollback_limit = limit;
        while self.scrollback.len() > self.scrollback_limit {
            self.scrollback.pop_front();
        }
        self.scrollback.shrink_to_fit();
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
        let hyperlink_id = self.current_hyperlink_id;
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
            self.alternate.put_char(ch, style, hyperlink_id);
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
            self.primary.put_char(ch, style, hyperlink_id);
            self.mark_dirty_row(row);
        }
        self.bump_version();
    }

    /// Activate an OSC 8 hyperlink. Every subsequent character written to the
    /// screen will carry this link ID until `set_active_hyperlink(None)` is
    /// called (which the terminal emits as the closing `\e]8;;\e\\` sequence).
    ///
    /// Passing `None` or an empty string disables the active hyperlink
    /// (equivalent to `\e]8;;\a`).
    pub fn set_active_hyperlink(&mut self, uri: Option<&str>) {
        match uri.filter(|u| !u.is_empty()) {
            Some(u) => {
                self.current_hyperlink_id = self.hyperlinks.intern(u);
            }
            None => {
                self.current_hyperlink_id = 0;
            }
        }
    }

    /// Resolve a hyperlink ID to its URI string.
    /// Returns `None` for ID 0 (no link) or unknown IDs.
    pub fn hyperlink_uri(&self, id: u16) -> Option<&str> {
        self.hyperlinks.resolve(id)
    }

    /// Collect all hyperlink spans visible at the given scroll offset.
    ///
    /// Returns a `Vec` of `(row, col_start, col_end_exclusive, id)` tuples
    /// covering every run of cells with the same non-zero hyperlink ID in the
    /// visible viewport. Useful for the snapshot builder to build a clickable
    /// link table without walking every cell individually.
    pub fn dump_hyperlink_spans(&self, scroll_offset: usize) -> Vec<(usize, usize, usize, u16)> {
        let grid = if self.use_alternate {
            &self.alternate
        } else {
            &self.primary
        };
        let rows = grid.rows;
        let cols = grid.cols;
        let scrollback_len = self.scrollback.len();

        // Determine where the visible window starts in the combined
        // (scrollback ++ primary) line sequence.
        let first_visible_abs = scrollback_len.saturating_sub(scroll_offset);

        let mut spans: Vec<(usize, usize, usize, u16)> = Vec::new();

        for vis_row in 0..rows {
            let abs_row = first_visible_abs + vis_row;
            let cells: &[Cell] = if abs_row < scrollback_len {
                let (ref line, _wrapped) = self.scrollback[abs_row];
                line.as_slice()
            } else {
                let grid_row = abs_row - scrollback_len;
                if grid_row >= grid.rows {
                    break;
                }
                let start = grid_row * cols;
                &grid.cells[start..start + cols]
            };

            // Walk along the row merging adjacent same-id cells into spans.
            let mut col = 0usize;
            while col < cells.len() {
                let id = cells[col].hyperlink_id;
                if id != 0 {
                    let run_start = col;
                    while col < cells.len() && cells[col].hyperlink_id == id {
                        col += 1;
                    }
                    spans.push((vis_row, run_start, col, id));
                } else {
                    col += 1;
                }
            }
        }

        spans
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
        // BS is a cursor movement control, not an erase operation.  Shell line
        // editors use it while redrawing long input; erasing here damages prompt
        // cells before the following redraw sequence has a chance to update them.
        let grid = self.active_grid_mut();
        grid.cursor_col = grid.cursor_col.saturating_sub(1);
        grid.pending_wrap = false;
        self.bump_version();
    }

    pub fn cursor_up(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let delta = n as usize;
        grid.cursor_row = grid.cursor_row.saturating_sub(delta);
        grid.pending_wrap = false;
        self.bump_version();
    }

    /// ESC M — reverse index: move cursor up one line, scrolling the scroll
    /// region down if the cursor is already at the top margin.
    pub fn reverse_index(&mut self) {
        let top = self.scroll_region.map_or(0, |(t, _)| t);
        let rows = self.active_grid().rows;
        let bottom = self
            .scroll_region
            .map_or(rows.saturating_sub(1), |(_, b)| b);
        let grid = self.active_grid_mut();
        if grid.cursor_row == top {
            // At top margin — insert a blank line by scrolling the region down.
            for row in (top..bottom).rev() {
                Self::copy_row(grid, row, row + 1);
            }
            grid.clear_row(top);
        } else {
            grid.cursor_row = grid.cursor_row.saturating_sub(1);
        }
        grid.pending_wrap = false;
        self.bump_version();
    }

    pub fn cursor_down(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let delta = n as usize;
        grid.cursor_row = (grid.cursor_row + delta).min(grid.rows.saturating_sub(1));
        grid.pending_wrap = false;
        self.bump_version();
    }

    pub fn cursor_forward(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let delta = n as usize;
        grid.cursor_col = (grid.cursor_col + delta).min(grid.cols.saturating_sub(1));
        grid.pending_wrap = false;
        self.bump_version();
    }

    pub fn cursor_backward(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let delta = n as usize;
        grid.cursor_col = grid.cursor_col.saturating_sub(delta);
        grid.pending_wrap = false;
        self.bump_version();
    }

    pub fn cursor_next_line(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        let delta = n as usize;
        grid.cursor_row = (grid.cursor_row + delta).min(grid.rows.saturating_sub(1));
        grid.cursor_col = 0;
        grid.pending_wrap = false;
        self.bump_version();
    }

    pub fn cursor_previous_line(&mut self, n: u16) {
        let grid = self.active_grid_mut();
        grid.cursor_row = grid.cursor_row.saturating_sub(n as usize);
        grid.cursor_col = 0;
        grid.pending_wrap = false;
        self.bump_version();
    }

    pub fn cursor_horizontal_absolute(&mut self, col_1based: u16) {
        let grid = self.active_grid_mut();
        grid.cursor_col = col_1based.saturating_sub(1) as usize;
        grid.clamp_cursor();
        grid.pending_wrap = false;
        self.bump_version();
    }

    pub fn cursor_vertical_absolute(&mut self, row_1based: u16) {
        let grid = self.active_grid_mut();
        grid.cursor_row = row_1based.saturating_sub(1) as usize;
        grid.clamp_cursor();
        grid.pending_wrap = false;
        self.bump_version();
    }

    pub fn cursor_position(&mut self, row_1based: u16, col_1based: u16) {
        let grid = self.active_grid_mut();
        grid.cursor_row = row_1based.saturating_sub(1) as usize;
        grid.cursor_col = col_1based.saturating_sub(1) as usize;
        grid.clamp_cursor();
        grid.pending_wrap = false;
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
        let old_cols = self.primary.cols;
        let old_rows = self.primary.rows;

        self.alternate.resize(new_rows, new_cols);

        // When column width changes, reflow logical lines so that content
        // wraps or un-wraps rather than being truncated. Cursor position is
        // tracked through the reflow so it stays at the equivalent character.
        // SIGWINCH suppression (TabState::suppress_until) prevents the shell's
        // prompt-redraw from appearing on top of the reflowed content.
        let (mut rows, reflow_cursor_row, reflow_cursor_col) = if new_cols != old_cols {
            reflow_grid(
                &self.primary,
                new_cols,
                self.primary.cursor_row,
                self.primary.cursor_col,
            )
        } else {
            let rows = (0..old_rows)
                .map(|row| {
                    (
                        self.primary.row_cells(row),
                        self.primary.line_wrapped.get(row).copied().unwrap_or(false),
                    )
                })
                .collect();
            (rows, self.primary.cursor_row, self.primary.cursor_col)
        };

        let reflow_len = rows.len();
        let mut new_cells = vec![Cell::default(); new_rows * new_cols];
        let mut new_line_wrapped = vec![false; new_rows];
        let new_cursor_row;
        let new_cursor_col;

        if reflow_len >= new_rows {
            let excess = reflow_len - new_rows;
            for (cells, wrapped) in rows.drain(0..excess) {
                self.push_scrollback(cells, wrapped);
            }
            for (dst_row, (cells, wrapped)) in rows.into_iter().enumerate().take(new_rows) {
                let dst = dst_row * new_cols;
                for (col, cell) in cells.into_iter().enumerate().take(new_cols) {
                    new_cells[dst + col] = cell;
                }
                new_line_wrapped[dst_row] = wrapped;
            }
            new_cursor_row = reflow_cursor_row.saturating_sub(excess);
            new_cursor_col = reflow_cursor_col;
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

            for (vi, (cells, wrapped)) in rows.iter().enumerate() {
                let dst_row = vis_start + vi;
                let dst = dst_row * new_cols;
                for (col, &cell) in cells.iter().enumerate().take(new_cols) {
                    new_cells[dst + col] = cell;
                }
                new_line_wrapped[dst_row] = *wrapped;
            }

            new_cursor_row = (vis_start + reflow_cursor_row).min(new_rows.saturating_sub(1));
            new_cursor_col = reflow_cursor_col;
        }

        self.primary.cells = new_cells;
        self.primary.rows = new_rows;
        self.primary.cols = new_cols;
        self.primary.line_wrapped = new_line_wrapped;
        self.primary.cursor_row = new_cursor_row;
        self.primary.cursor_col = new_cursor_col.min(new_cols.saturating_sub(1));
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

    /// Returns plain text for scrollback (oldest first) followed by the
    /// current visible grid.
    pub fn dump_text_with_scrollback(&self) -> String {
        let grid = &self.primary;
        let cols = grid.cols;
        let mut out = String::new();

        for (row_cells, _wrapped) in &self.scrollback {
            for col in 0..cols {
                out.push(row_cells.get(col).copied().unwrap_or_default().ch);
            }
            out.push('\n');
        }

        for row in 0..grid.rows {
            let start = row * cols;
            for col in 0..cols {
                out.push(grid.cells[start + col].ch);
            }
            if row + 1 < grid.rows {
                out.push('\n');
            }
        }

        out
    }

    /// Encodes the scrollback + visible grid as a string of ANSI SGR escape
    /// sequences so that fg/bg colors and text attributes are preserved when
    /// fed back through the terminal parser on next launch.
    ///
    /// Returns an `Arc<String>` so repeated callers in the same frame share the
    /// allocation instead of copying the (potentially large) buffer.
    pub fn dump_ansi(&self) -> Arc<String> {
        {
            let cache = self.ansi_cache.borrow();
            if let Some((cached_version, ref text)) = *cache
                && cached_version == self.version
            {
                return Arc::clone(text);
            }
        }
        let grid = &self.primary;
        let cols = grid.cols;
        let mut out = String::new();
        let mut cur = CellStyle::default();

        // Scrollback rows (oldest first). Only emit \n at logical-line
        // boundaries (when the row is NOT a soft-wrap continuation). This
        // preserves the information that adjacent wrapped rows belong to the
        // same logical line, so that restore at a different width can re-wrap
        // them correctly.
        for (row_cells, wrapped) in &self.scrollback {
            let len = row_cells.len().min(cols);
            encode_ansi_row(&mut out, &mut cur, &row_cells[..len]);
            if !wrapped {
                out.push('\n');
            }
        }

        // Visible grid rows — same logic.
        for row in 0..grid.rows {
            let start = row * cols;
            encode_ansi_row(&mut out, &mut cur, &grid.cells[start..start + cols]);
            let is_wrapped = grid.line_wrapped.get(row).copied().unwrap_or(false);
            if !is_wrapped && row + 1 < grid.rows {
                out.push('\n');
            }
        }

        let arc = Arc::new(out);
        *self.ansi_cache.borrow_mut() = Some((self.version, Arc::clone(&arc)));
        arc
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

    pub fn mark_full_redraw(&mut self) {
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
    fn backspace_moves_cursor_without_erasing_prompt_content() {
        let mut screen = Screen::new(1, 8);
        for ch in "prompt".chars() {
            screen.put_char(ch);
        }

        screen.backspace();

        assert_eq!(screen.cursor_col(), 5);
        assert!(screen.dump_text().starts_with("prompt"));
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
    fn dump_text_with_scrollback_includes_history_rows() {
        let mut screen = Screen::new(2, 2);
        for ch in "abcd".chars() {
            screen.put_char(ch);
        }
        screen.linefeed();
        screen.carriage_return();
        for ch in "ef".chars() {
            screen.put_char(ch);
        }

        let all = screen.dump_text_with_scrollback();
        assert!(all.contains("ab"));
        assert!(all.contains("ef"));
    }

    #[test]
    fn absolute_and_line_cursor_movements_cancel_pending_wrap() {
        let mut screen = Screen::new(3, 4);
        for ch in "abcd".chars() {
            screen.put_char(ch);
        }

        screen.cursor_horizontal_absolute(1);
        screen.put_char('x');
        assert!(screen.dump_text().starts_with("xbcd"));

        screen.cursor_next_line(1);
        screen.put_char('y');
        screen.cursor_previous_line(1);
        screen.cursor_vertical_absolute(3);
        screen.put_char('z');

        assert_eq!(screen.cursor_row(), 2);
        assert!(screen.dump_text().contains('y'));
        assert!(screen.dump_text().contains('z'));
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
