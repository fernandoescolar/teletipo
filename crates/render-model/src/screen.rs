#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderCell {
    pub ch: char,
    pub fg: Option<[f32; 3]>,
    pub bg: Option<[f32; 3]>,
    pub style: u8,
}

impl Default for RenderCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: None,
            bg: None,
            style: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderRow {
    pub cells: Vec<RenderCell>,
    /// True when the row was touched in the source damage model for this frame.
    pub dirty: bool,
}

impl Default for RenderRow {
    fn default() -> Self {
        Self {
            cells: Vec::new(),
            dirty: false,
        }
    }
}

impl RenderRow {
    pub fn text(&self) -> String {
        self.cells.iter().map(|c| c.ch).collect()
    }
}

#[derive(Debug, Clone)]
pub struct DamageRegion {
    pub full_redraw: bool,
    pub dirty_rows: Vec<usize>,
    /// Number of columns in the terminal grid used to index `dirty_cells`.
    pub cols: usize,
    /// Cell-level damage bitset in row-major order. Length = rows * cols.
    pub dirty_cells: Vec<bool>,
}

impl DamageRegion {
    pub fn is_empty(&self) -> bool {
        !self.full_redraw && self.dirty_rows.is_empty() && !self.dirty_cells.iter().any(|v| *v)
    }

    pub fn row_is_dirty(&self, row: usize) -> bool {
        if self.full_redraw || self.dirty_rows.contains(&row) {
            return true;
        }
        if self.cols == 0 {
            return false;
        }
        let start = row.saturating_mul(self.cols);
        let end = start.saturating_add(self.cols).min(self.dirty_cells.len());
        self.dirty_cells[start..end].iter().any(|v| *v)
    }

    pub fn merge_from(&mut self, other: &DamageRegion) {
        if other.full_redraw {
            self.full_redraw = true;
        }
        self.cols = self.cols.max(other.cols);
        self.dirty_rows.extend(other.dirty_rows.iter().copied());
        if self.dirty_cells.len() < other.dirty_cells.len() {
            self.dirty_cells.resize(other.dirty_cells.len(), false);
        }
        for (idx, dirty) in other.dirty_cells.iter().copied().enumerate() {
            if dirty {
                self.dirty_cells[idx] = true;
            }
        }
    }

    pub fn clear(&mut self) {
        self.full_redraw = false;
        self.dirty_rows.clear();
        for slot in &mut self.dirty_cells {
            *slot = false;
        }
    }
}

impl Default for DamageRegion {
    fn default() -> Self {
        Self {
            full_redraw: true,
            dirty_rows: Vec::new(),
            cols: 0,
            dirty_cells: Vec::new(),
        }
    }
}
