/// Centralises the terminal/editor grid-size calculations that depend on
/// window dimensions, cell metrics, and the per-tab split ratio.
///
/// Construct one per code path that needs to compute `rows`/`cols`, passing
/// the tab-bar height appropriate for that context (0.0 when there is only
/// one tab, `cell_h` when a tab bar is present).
pub(crate) struct LayoutMetrics {
    available_h: f32,
    pad_h: f32,
    pad_v: f32,
    cell_w: f32,
    cell_h: f32,
    window_width: u32,
}

impl LayoutMetrics {
    pub(crate) fn new(
        window_width: u32,
        window_height: u32,
        tab_bar_h: f32,
        cell_w: f32,
        cell_h: f32,
        pad_h: f32,
        pad_v: f32,
    ) -> Self {
        Self {
            available_h: window_height as f32 - tab_bar_h,
            pad_h,
            pad_v,
            cell_w,
            cell_h,
            window_width,
        }
    }

    /// Number of terminal columns for the current window width.
    pub(crate) fn cols(&self) -> u16 {
        ((self.window_width as f32 - 2.0 * self.pad_h) / self.cell_w).max(1.0) as u16
    }

    /// Number of terminal rows for the given split ratio.
    pub(crate) fn term_rows(&self, split_ratio: f32) -> u16 {
        let term_h = (self.available_h * split_ratio - 2.0 * self.pad_v).max(self.cell_h);
        (term_h / self.cell_h).max(1.0) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::LayoutMetrics;

    #[test]
    fn cols_uses_padding() {
        let m = LayoutMetrics::new(800, 600, 0.0, 8.0, 16.0, 4.0, 4.0);
        // (800 - 2*4) / 8 = 99
        assert_eq!(m.cols(), 99);
    }

    #[test]
    fn term_rows_uses_split_and_padding() {
        let m = LayoutMetrics::new(800, 600, 16.0, 8.0, 16.0, 4.0, 4.0);
        // available_h = 600 - 16 = 584; term_h = 584*0.7 - 2*4 = 408.8 - 8 = 400.8; rows = 400.8/16 = 25
        assert_eq!(m.term_rows(0.7), 25);
    }

    #[test]
    fn cols_clamps_to_one() {
        let m = LayoutMetrics::new(1, 100, 0.0, 8.0, 16.0, 4.0, 4.0);
        assert_eq!(m.cols(), 1);
    }

    #[test]
    fn term_rows_clamps_to_one() {
        let m = LayoutMetrics::new(800, 1, 0.0, 8.0, 16.0, 4.0, 100.0);
        assert_eq!(m.term_rows(0.7), 1);
    }
}
