/// Terminal background colors: one rectangle per cell with background color.
use crate::{RenderContext, Scene};

/// Emit background color rectangles for terminal cells.
/// Each cell with a non-default background color gets a colored rectangle.
pub fn render_backgrounds(ctx: &RenderContext, scene: &mut Scene) {
    let snapshot = ctx.snapshot;
    let layout = ctx.layout;

    // Terminal rendering window
    let max_x = layout.width - layout.padding_h;
    let max_y = layout.terminal_text_bottom;
    let terminal_text = snapshot.terminal_text_from_rows();
    let lines: Vec<&str> = terminal_text.lines().collect();

    // Apply window opacity
    let backdrop = frosted_backdrop_alpha(snapshot.opacity);

    let mut line_char_start = 0usize;

    // Draw background color cells
    // Process each row of terminal text
    for (row, line) in lines.iter().copied().enumerate() {
        let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;

        // Stop if we've scrolled past the visible area
        if y >= max_y {
            break;
        }

        // Process each column in this row
        for (col, _) in line.chars().enumerate() {
            let x = layout.padding_h + col as f32 * layout.cell_w_px;

            // Stop if we've scrolled past the right edge
            if x + layout.cell_w_px > max_x {
                break;
            }

            // Get background color for this cell (if any)
            let idx = line_char_start + col;
            if let Some(bg) = snapshot.terminal_bg_colors.get(idx).and_then(|c| *c) {
                // Emit a rectangle for this cell's background
                scene.rect(
                    x,
                    y,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    [bg[0], bg[1], bg[2], backdrop],
                );
            }
        }

        // Advance to next line (including newline character)
        line_char_start = line_char_start.saturating_add(line.chars().count() + 1);
    }
}

/// Calculate frosted backdrop alpha.
fn frosted_backdrop_alpha(opacity: f32) -> f32 {
    let opacity = opacity.clamp(0.0, 1.0);
    0.55 + 0.45 * opacity
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CellMetrics, DamageRegion, FrameLayout, RenderCommand, RenderRow, RenderSnapshot,
        RenderTarget,
    };
    use std::sync::Arc;

    fn make_test_snapshot(
        lines: Vec<&'static str>,
        bg_colors: Vec<Option<[f32; 3]>>,
        opacity: f32,
    ) -> RenderSnapshot {
        let terminal_text = lines.join("\n");
        // terminal_text_from_rows() will regenerate text from rows, so we need to populate rows properly
        let mut terminal_rows = Vec::new();
        for line in &lines {
            let row = RenderRow {
                cells: line
                    .chars()
                    .map(|ch| crate::RenderCell {
                        ch,
                        ..crate::RenderCell::default()
                    })
                    .collect(),
                ..RenderRow::default()
            };
            terminal_rows.push(row);
        }

        RenderSnapshot {
            terminal_rows,
            terminal_damage: Arc::new(DamageRegion::default()),
            terminal_text: terminal_text.clone(),
            terminal_fg_colors: vec![None; terminal_text.len()],
            terminal_bg_colors: bg_colors,
            terminal_styles: vec![0; terminal_text.len()],
            editor_text: String::new(),
            editor_fg_colors: Vec::new(),
            editor_cursor_offset: 0,
            scroll_offset: 0,
            scrollback_lines: 0,
            editor_focused: false,
            editor_disabled: false,
            split_ratio: 0.7,
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
            tab_labels: Vec::new(),
            active_tab: 0,
            context_menu: None,
            tab_drag_from: None,
            tab_drag_insert_before: None,
            theme: Default::default(),
            padding_h: 8,
            padding_v: 4,
            settings_overlay: None,
            keybindings_overlay: None,
            title_cwd: String::new(),
            editor_suggestion: String::new(),
            suggestion_dropdown: None,
            search_panel: None,
            sticky_command_overlay: None,
            terminal_links: Vec::new(),
            request_exit: false,
            cursor_shape: 0,
            bell_active: false,
            cursor_blink_on: true,
            terminal_cursor_row: 0,
            terminal_cursor_col: 0,
            terminal_fullscreen: false,
            terminal_screen_version: 0,
            toast_stack: Vec::new(),
            command_palette: None,
            font_size: 14.0,
            opacity,
        }
    }

    fn make_test_layout() -> FrameLayout {
        FrameLayout {
            width: 800.0,
            height: 600.0,
            tab_bar_h: 0.0,
            terminal_h: 420.0,
            editor_top: 422.0,
            terminal_text_top: 4.0,
            terminal_text_bottom: 416.0,
            padding_h: 8.0,
            padding_v: 4.0,
            cell_w_px: 10.0,
            cell_h_px: 20.0,
        }
    }

    #[test]
    fn test_terminal_background_empty() {
        let snapshot = make_test_snapshot(vec![], vec![], 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_backgrounds(&ctx, &mut scene);

        // Empty terminal: no background colors
        assert_eq!(scene.len(), 0);
    }

    #[test]
    fn test_terminal_background_no_colors() {
        let snapshot = make_test_snapshot(
            vec!["hello world"],
            vec![None; 11], // No background colors
            1.0,
        );
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_backgrounds(&ctx, &mut scene);

        // No background colors set: no rectangles
        assert_eq!(scene.len(), 0);
    }

    #[test]
    fn test_terminal_background_single_cell() {
        let bg_colors = vec![
            None,
            None,
            Some([1.0, 0.0, 0.0]), // Red background on 'l'
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ];
        let snapshot = make_test_snapshot(vec!["hello world"], bg_colors, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_backgrounds(&ctx, &mut scene);

        // One cell with background color: one rectangle
        assert_eq!(scene.len(), 1);

        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, layout.padding_h + 2.0 * layout.cell_w_px);
                assert_eq!(cmd.rect.y, layout.terminal_text_top);
                assert_eq!(cmd.rect.w, layout.cell_w_px);
                assert_eq!(cmd.rect.h, layout.cell_h_px);
                // Color should be red with opacity
                assert_eq!(cmd.color[0], 1.0);
                assert_eq!(cmd.color[1], 0.0);
                assert_eq!(cmd.color[2], 0.0);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_terminal_background_multiple_colors() {
        let bg_colors = vec![
            Some([1.0, 0.0, 0.0]), // Red
            Some([0.0, 1.0, 0.0]), // Green
            Some([0.0, 0.0, 1.0]), // Blue
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ];
        let snapshot = make_test_snapshot(vec!["hello world"], bg_colors, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_backgrounds(&ctx, &mut scene);

        // Three cells with colors: three rectangles
        assert_eq!(scene.len(), 3);
    }

    #[test]
    fn test_terminal_background_multiple_lines() {
        let bg_colors = vec![
            Some([1.0, 0.0, 0.0]), // Line 1, cell 0
            None,
            None,
            None,
            None,
            // Newline (index 5)
            None,
            None,
            Some([0.0, 1.0, 0.0]), // Line 2, cell 2
            None,
            None,
        ];
        let snapshot = make_test_snapshot(vec!["hello", "world"], bg_colors, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_backgrounds(&ctx, &mut scene);

        // Two cells with background colors on different lines
        assert_eq!(scene.len(), 2);

        // First rect: first line, first cell
        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.y, layout.terminal_text_top);
                assert_eq!(cmd.color[0], 1.0); // Red
            }
            _ => panic!("Expected Rect command"),
        }

        // Second rect: second line
        match &scene.main[1] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.y, layout.terminal_text_top + layout.cell_h_px);
                assert_eq!(cmd.color[1], 1.0); // Green
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_terminal_background_opacity_applied() {
        let bg_colors = vec![Some([1.0, 0.0, 0.0])];
        let snapshot = make_test_snapshot(vec!["x"], bg_colors, 0.5);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_backgrounds(&ctx, &mut scene);

        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                // With opacity=0.5: frosted_backdrop_alpha(0.5) = 0.775
                let expected_alpha = 0.775;
                assert!((cmd.color[3] - expected_alpha).abs() < 0.01);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_terminal_background_all_rects() {
        let bg_colors = vec![Some([1.0, 0.0, 0.0]); 10];
        let snapshot = make_test_snapshot(vec!["test test x"], bg_colors, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_backgrounds(&ctx, &mut scene);

        // All commands should be Rect
        for command in &scene.main {
            match command {
                RenderCommand::Rect(_) => {}
                _ => panic!("Expected only Rect commands"),
            }
        }
    }
}
