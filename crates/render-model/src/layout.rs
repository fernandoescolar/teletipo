/// Frame layout computation: backend-independent calculation of pane geometries.

use crate::RenderSnapshot;

/// Frame layout constants.
pub const TAB_HEIGHT_MULTIPLIER: f32 = 1.0;
pub const SEPARATOR_WIDTH_PX: f32 = 2.0;

/// Target display dimensions and scale information.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderTarget {
    pub width: f32,
    pub height: f32,
}

impl RenderTarget {
    pub fn new(width: f32, height: f32) -> Self {
        RenderTarget {
            width: width.max(1.0),
            height: height.max(1.0),
        }
    }
}

/// Character cell dimensions in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
}

impl CellMetrics {
    pub fn new(width: f32, height: f32) -> Self {
        CellMetrics { width, height }
    }
}

/// Per-frame calculated layout geometry for terminal and editor panes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameLayout {
    /// Total width of the frame in pixels.
    pub width: f32,
    /// Total height of the frame in pixels.
    pub height: f32,
    /// Height of the tab bar (0 if no tabs visible).
    pub tab_bar_h: f32,
    /// Bottom edge of terminal pane (includes tab bar).
    pub terminal_h: f32,
    /// Top edge of editor pane (after separator).
    pub editor_top: f32,
    /// Top edge of terminal text content (inside padding).
    pub terminal_text_top: f32,
    /// Bottom edge of terminal text content (inside padding).
    pub terminal_text_bottom: f32,
    /// Horizontal padding around content in pixels.
    pub padding_h: f32,
    /// Vertical padding around content in pixels.
    pub padding_v: f32,
    /// Width of a character cell in pixels.
    pub cell_w_px: f32,
    /// Height of a character cell in pixels.
    pub cell_h_px: f32,
}

/// Compute frame layout geometry based on snapshot state, target size, and cell metrics.
pub fn compute_frame_layout(
    snapshot: &RenderSnapshot,
    target: RenderTarget,
    metrics: CellMetrics,
) -> FrameLayout {
    let width = target.width;
    let height = target.height;

    // Tab bar is only shown if there are tab labels
    let tab_bar_h = if snapshot.tab_labels.is_empty() {
        0.0
    } else {
        (metrics.height * TAB_HEIGHT_MULTIPLIER).max(1.0)
    };

    let available_h = (height - tab_bar_h).max(1.0);

    // Terminal fullscreen ignores split ratio
    let split_ratio = if snapshot.terminal_fullscreen {
        1.0
    } else {
        snapshot.split_ratio.clamp(0.05, 0.95)
    };

    let terminal_h = (tab_bar_h + available_h * split_ratio).floor();
    let editor_top = (terminal_h + SEPARATOR_WIDTH_PX).min(height);

    // Terminal content vertical layout
    let terminal_rows = snapshot.terminal_rows_len() as f32;
    let padding_v_px = snapshot.padding_v as f32;
    let padding_h_px = snapshot.padding_h as f32;

    let effective_term_h = (available_h * split_ratio - 2.0 * padding_v_px).max(0.0);
    let content_h_px = (terminal_rows * metrics.height).min(effective_term_h);
    let terminal_text_top = tab_bar_h + padding_v_px + (effective_term_h - content_h_px).max(0.0);
    let terminal_text_bottom = terminal_h - padding_v_px;

    FrameLayout {
        width,
        height,
        tab_bar_h,
        terminal_h,
        editor_top,
        terminal_text_top,
        terminal_text_bottom,
        padding_h: padding_h_px,
        padding_v: padding_v_px,
        cell_w_px: metrics.width,
        cell_h_px: metrics.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DamageRegion, RenderRow};

    // Helper to create a minimal snapshot for testing
    fn make_snapshot(
        tab_labels: Vec<String>,
        terminal_fullscreen: bool,
        split_ratio: f32,
        terminal_rows: usize,
        padding_h: u32,
        padding_v: u32,
    ) -> RenderSnapshot {
        use std::sync::Arc;

        // Create empty RenderRow entries
        let rows = vec![RenderRow::default(); terminal_rows];

        RenderSnapshot {
            terminal_rows: rows,
            terminal_damage: Arc::new(DamageRegion::default()),
            terminal_text: String::new(),
            terminal_fg_colors: Vec::new(),
            terminal_bg_colors: Vec::new(),
            terminal_styles: Vec::new(),
            editor_text: String::new(),
            editor_fg_colors: Vec::new(),
            editor_cursor_offset: 0,
            scroll_offset: 0,
            scrollback_lines: 0,
            editor_focused: false,
            editor_disabled: false,
            split_ratio,
            resize_overlay: None,
            editor_line_count: 0,
            editor_scroll_offset: 0,
            editor_horizontal_scroll_offset: 0,
            editor_selection: None,
            selection: None,
            search_highlights: Vec::new(),
            search_current_highlight: None,
            copy_mode_highlights: Vec::new(),
            copy_mode_cursor: None,
            terminal_images: Vec::new(),
            tab_labels,
            active_tab: 0,
            context_menu: None,
            tab_drag_from: None,
            tab_drag_insert_before: None,
            theme: Default::default(),
            padding_h,
            padding_v,
            settings_overlay: None,
            keybindings_overlay: None,
            title_cwd: String::new(),
            editor_suggestion: String::new(),
            suggestion_dropdown: None,
            search_panel: None,
            terminal_links: Vec::new(),
            request_exit: false,
            cursor_shape: 0,
            bell_active: false,
            cursor_blink_on: true,
            terminal_cursor_row: 0,
            terminal_cursor_col: 0,
            terminal_fullscreen,
            terminal_screen_version: 0,
            toast_stack: Vec::new(),
            command_palette: None,
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    #[test]
    fn test_layout_no_tabs() {
        let snapshot = make_snapshot(vec![], false, 0.7, 24, 8, 4);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let layout = compute_frame_layout(&snapshot, target, metrics);

        assert_eq!(layout.width, 800.0);
        assert_eq!(layout.height, 600.0);
        assert_eq!(layout.tab_bar_h, 0.0); // No tabs
    }

    #[test]
    fn test_layout_with_tabs() {
        let snapshot = make_snapshot(
            vec!["Tab 1".to_string(), "Tab 2".to_string()],
            false,
            0.7,
            24,
            8,
            4,
        );
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let layout = compute_frame_layout(&snapshot, target, metrics);

        // Tab bar height = cell_h * TAB_HEIGHT_MULTIPLIER = 20.0 * 1.0 = 20.0
        assert_eq!(layout.tab_bar_h, 20.0);
        assert!(layout.terminal_h > layout.tab_bar_h);
    }

    #[test]
    fn test_layout_terminal_fullscreen() {
        let snapshot = make_snapshot(vec!["Tab".to_string()], true, 0.3, 24, 8, 4);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let layout = compute_frame_layout(&snapshot, target, metrics);

        // With fullscreen, split_ratio should be 1.0, so terminal_h should be near height
        // terminal_h = tab_bar_h + (height - tab_bar_h) * 1.0 = height
        assert_eq!(layout.terminal_h, 600.0);
    }

    #[test]
    fn test_layout_split_ratio() {
        let snapshot = make_snapshot(vec![], false, 0.5, 24, 8, 4);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let layout = compute_frame_layout(&snapshot, target, metrics);

        // available_h = 600 - 0 = 600
        // terminal_h = 0 + 600 * 0.5 = 300
        assert_eq!(layout.terminal_h, 300.0);
    }

    #[test]
    fn test_layout_split_ratio_clamping() {
        // Test minimum clamp (0.05)
        let snapshot_min = make_snapshot(vec![], false, 0.01, 24, 8, 4);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let layout_min = compute_frame_layout(&snapshot_min, target, metrics);
        // split_ratio clamped to 0.05: 0 + 600 * 0.05 = 30
        assert_eq!(layout_min.terminal_h, 30.0);

        // Test maximum clamp (0.95)
        let snapshot_max = make_snapshot(vec![], false, 0.99, 24, 8, 4);
        let layout_max = compute_frame_layout(&snapshot_max, target, metrics);
        // split_ratio clamped to 0.95: 0 + 600 * 0.95 = 570
        assert_eq!(layout_max.terminal_h, 570.0);
    }

    #[test]
    fn test_layout_editor_top() {
        let snapshot = make_snapshot(vec![], false, 0.6, 24, 8, 4);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let layout = compute_frame_layout(&snapshot, target, metrics);

        // editor_top = terminal_h + SEPARATOR_WIDTH_PX
        // terminal_h = 0 + 600 * 0.6 = 360
        // editor_top = 360 + 2 = 362
        assert_eq!(layout.editor_top, 362.0);
    }

    #[test]
    fn test_layout_padding() {
        let snapshot = make_snapshot(vec![], false, 0.7, 24, 16, 8);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let layout = compute_frame_layout(&snapshot, target, metrics);

        assert_eq!(layout.padding_h, 16.0);
        assert_eq!(layout.padding_v, 8.0);
    }

    #[test]
    fn test_layout_terminal_text_bounds() {
        let snapshot = make_snapshot(vec![], false, 0.7, 24, 8, 4);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let layout = compute_frame_layout(&snapshot, target, metrics);

        // terminal_text_top should be inside the terminal region and below tab bar
        assert!(layout.terminal_text_top >= layout.tab_bar_h);
        // terminal_text_bottom should be inside the terminal region
        assert!(layout.terminal_text_bottom <= layout.terminal_h);
        // Make sure text bounds are valid
        assert!(layout.terminal_text_bottom > layout.terminal_text_top);
    }

    #[test]
    fn test_render_target_new() {
        let target = RenderTarget::new(800.0, 600.0);
        assert_eq!(target.width, 800.0);
        assert_eq!(target.height, 600.0);

        // Should clamp to minimum of 1.0
        let target_zero = RenderTarget::new(0.0, 0.0);
        assert_eq!(target_zero.width, 1.0);
        assert_eq!(target_zero.height, 1.0);
    }

    #[test]
    fn test_cell_metrics_new() {
        let metrics = CellMetrics::new(10.0, 20.0);
        assert_eq!(metrics.width, 10.0);
        assert_eq!(metrics.height, 20.0);
    }

    #[test]
    fn test_layout_consistency() {
        // Ensure layout is internally consistent
        let snapshot = make_snapshot(vec!["Tab".to_string()], false, 0.65, 24, 8, 4);
        let target = RenderTarget::new(1024.0, 768.0);
        let metrics = CellMetrics::new(8.0, 16.0);

        let layout = compute_frame_layout(&snapshot, target, metrics);

        // Check ranges
        assert!(layout.width > 0.0);
        assert!(layout.height > 0.0);
        assert!(layout.tab_bar_h >= 0.0);
        assert!(layout.terminal_h > layout.tab_bar_h);
        assert!(layout.editor_top > layout.terminal_h);
        assert!(layout.editor_top <= layout.height);
        assert!(layout.cell_w_px > 0.0);
        assert!(layout.cell_h_px > 0.0);
    }
}
