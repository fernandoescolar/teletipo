/// Tab bar component: renders tab backgrounds, separators, and drag indicators.
/// Text (labels and buttons) remains on the old path temporarily.

use crate::{RenderContext, Scene};

/// Marker struct for the tab bar component.
pub struct TabBar;

impl TabBar {
    /// Emit tab bar-related render commands.
    /// Handles tab background rectangles, drag indicators, and add button background.
    /// Text rendering (labels, close buttons, add button '+') is left to the old painter path.
    pub fn render(ctx: &RenderContext, scene: &mut Scene) {
        render_tab_bar_backgrounds(ctx, scene);
    }
}

/// Emit tab bar background rectangles and geometry.
/// Does NOT emit text commands yet (see TODOs below).
fn render_tab_bar_backgrounds(ctx: &RenderContext, scene: &mut Scene) {
    let snapshot = ctx.snapshot;
    let layout = ctx.layout;
    let theme = ctx.theme;

    // Don't render if no tabs or tab bar is not visible
    if snapshot.tab_labels.is_empty() || layout.tab_bar_h <= 0.0 {
        return;
    }

    // Apply window opacity
    let apply_opacity = |color: [f32; 4]| -> [f32; 4] {
        let mut c = color;
        c[3] = (c[3] * frosted_backdrop_alpha(snapshot.opacity)).clamp(0.0, 1.0);
        c
    };

    // Color calculations (matching painter.rs logic)
    let tab_bar_bg = clamp_color(theme.terminal_bg, 0.05);
    let tab_inactive = clamp_color(theme.terminal_bg, 0.02);
    let tab_active = mix_color(tab_bar_bg, theme.separator_focused, 0.22);
    let add_btn_bg = [
        (theme.terminal_bg[0] + 0.05).clamp(0.0, 1.0),
        (theme.terminal_bg[1] + 0.10).clamp(0.0, 1.0),
        (theme.terminal_bg[2] + 0.03).clamp(0.0, 1.0),
        0.90,
    ];

    // Tab bar background
    scene.rect(0.0, 0.0, layout.width, layout.tab_bar_h, apply_opacity(tab_bar_bg));

    // Layout calculations
    let n = snapshot.tab_labels.len().max(1);
    let add_w = layout.cell_w_px * 2.0;
    let tab_area_w = (layout.width - add_w).max(layout.cell_w_px * 2.0);
    let tab_w = (tab_area_w / n as f32).max(layout.cell_w_px * 3.0);
    let gap = 1.0;

    // Individual tab backgrounds
    for (i, _label) in snapshot.tab_labels.iter().enumerate() {
        let x0 = i as f32 * tab_w + gap;
        let x1 = ((i + 1) as f32 * tab_w - gap).min(tab_area_w - gap);
        let y0 = 1.0;
        let y1 = (layout.tab_bar_h - 1.0).max(y0 + 1.0);

        let color = if i == snapshot.active_tab {
            tab_active
        } else {
            tab_inactive
        };
        scene.rect(x0, y0, x1 - x0, y1 - y0, apply_opacity(color));

        // TODO: Text rendering (tab label, close button) still uses painter path.
        // Once TextCommand supports per-character glyph rendering with exact positioning,
        // migrate label and close button rendering here.
    }

    // Add button background
    let add_x0 = tab_area_w + gap;
    let add_x1 = (layout.width - gap).max(add_x0 + 1.0);
    scene.rect(
        add_x0,
        1.0,
        add_x1 - add_x0,
        (layout.tab_bar_h - 2.0).max(1.0),
        apply_opacity(add_btn_bg),
    );

    // TODO: Add button '+' glyph still uses painter path.

    // Drag indicator (line showing insertion point)
    if let Some(insert_before) = snapshot.tab_drag_insert_before {
        let ib = insert_before.min(n);
        let x = (ib as f32 * tab_w).clamp(0.0, tab_area_w);
        scene.rect(
            (x - 1.0).max(0.0),
            0.0,
            2.0,
            layout.tab_bar_h,
            apply_opacity(theme.separator_focused),
        );
    }
}

/// Clamp color by adding delta to each RGB component (from painter.rs).
fn clamp_color(mut c: [f32; 4], d: f32) -> [f32; 4] {
    c[0] = (c[0] + d).clamp(0.0, 1.0);
    c[1] = (c[1] + d).clamp(0.0, 1.0);
    c[2] = (c[2] + d).clamp(0.0, 1.0);
    c
}

/// Mix two colors linearly (from painter.rs).
fn mix_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// Calculate frosted backdrop alpha.
fn frosted_backdrop_alpha(opacity: f32) -> f32 {
    let opacity = opacity.clamp(0.0, 1.0);
    0.55 + 0.45 * opacity
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CellMetrics, DamageRegion, FrameLayout, RenderCommand, RenderRow, RenderSnapshot, RenderTarget};
    use std::sync::Arc;

    fn make_test_snapshot(tab_count: usize, active_tab: usize, opacity: f32) -> RenderSnapshot {
        let mut tab_labels = Vec::new();
        for i in 0..tab_count {
            tab_labels.push(format!("Tab {}", i + 1));
        }

        RenderSnapshot {
            terminal_rows: vec![RenderRow::default(); 24],
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
            tab_labels,
            active_tab,
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

    fn make_test_layout(tab_bar_h: f32) -> FrameLayout {
        FrameLayout {
            width: 800.0,
            height: 600.0,
            tab_bar_h,
            terminal_h: 420.0,
            editor_top: 422.0,
            terminal_text_top: 4.0 + tab_bar_h,
            terminal_text_bottom: 416.0,
            padding_h: 8.0,
            padding_v: 4.0,
            cell_w_px: 10.0,
            cell_h_px: 20.0,
        }
    }

    #[test]
    fn test_tab_bar_no_tabs() {
        let snapshot = make_test_snapshot(0, 0, 1.0);
        let layout = make_test_layout(0.0);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        TabBar::render(&ctx, &mut scene);

        // No tabs: should render nothing
        assert_eq!(scene.len(), 0);
    }

    #[test]
    fn test_tab_bar_invisible() {
        let snapshot = make_test_snapshot(1, 0, 1.0);
        let layout = make_test_layout(0.0); // tab_bar_h == 0
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        TabBar::render(&ctx, &mut scene);

        // Tab bar not visible: should render nothing
        assert_eq!(scene.len(), 0);
    }

    #[test]
    fn test_tab_bar_one_tab() {
        let snapshot = make_test_snapshot(1, 0, 1.0);
        let layout = make_test_layout(20.0);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        TabBar::render(&ctx, &mut scene);

        // With 1 tab: tab_bar_bg + tab + add_button = 3 rects
        assert_eq!(scene.len(), 3);

        // All should be Rect commands
        for command in &scene.main {
            match command {
                RenderCommand::Rect(_) => {}
                _ => panic!("Expected Rect command"),
            }
        }
    }

    #[test]
    fn test_tab_bar_multiple_tabs() {
        let snapshot = make_test_snapshot(3, 1, 1.0);
        let layout = make_test_layout(20.0);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        TabBar::render(&ctx, &mut scene);

        // With 3 tabs: tab_bar_bg + 3_tabs + add_button = 5 rects
        assert_eq!(scene.len(), 5);
    }

    #[test]
    fn test_tab_bar_background_rect() {
        let snapshot = make_test_snapshot(1, 0, 1.0);
        let layout = make_test_layout(20.0);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        TabBar::render(&ctx, &mut scene);

        // First rect should be tab bar background
        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 0.0);
                assert_eq!(cmd.rect.y, 0.0);
                assert_eq!(cmd.rect.w, layout.width);
                assert_eq!(cmd.rect.h, layout.tab_bar_h);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_tab_bar_individual_tabs() {
        let snapshot = make_test_snapshot(2, 0, 1.0);
        let layout = make_test_layout(20.0);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        TabBar::render(&ctx, &mut scene);

        // Commands: tab_bar_bg, tab0, tab1, add_button = 4
        assert_eq!(scene.len(), 4);

        // Second command: first tab
        match &scene.main[1] {
            RenderCommand::Rect(cmd) => {
                // Should be active tab (index 0)
                assert!(cmd.rect.y >= 1.0);
                assert!(cmd.rect.y < layout.tab_bar_h);
            }
            _ => panic!("Expected Rect command"),
        }

        // Third command: second tab
        match &scene.main[2] {
            RenderCommand::Rect(cmd) => {
                // Should be inactive tab
                assert!(cmd.rect.y >= 1.0);
                assert!(cmd.rect.y < layout.tab_bar_h);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_tab_bar_add_button_rect() {
        let snapshot = make_test_snapshot(1, 0, 1.0);
        let layout = make_test_layout(20.0);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        TabBar::render(&ctx, &mut scene);

        // Last rect should be add button
        match &scene.main[scene.len() - 1] {
            RenderCommand::Rect(cmd) => {
                // Add button should be on the right
                assert!(cmd.rect.x > layout.width * 0.8);
                assert!(cmd.rect.y >= 1.0);
                assert!(cmd.rect.y < layout.tab_bar_h);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_tab_bar_drag_indicator() {
        let mut snapshot = make_test_snapshot(2, 0, 1.0);
        snapshot.tab_drag_insert_before = Some(1); // Drag indicator before tab 1

        let layout = make_test_layout(20.0);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        TabBar::render(&ctx, &mut scene);

        // With drag indicator: tab_bar_bg + tab0 + tab1 + add_button + drag_line = 5
        assert_eq!(scene.len(), 5);

        // Last rect should be drag indicator (thin vertical line)
        match &scene.main[scene.len() - 1] {
            RenderCommand::Rect(cmd) => {
                // Drag indicator should be thin (width ~2.0)
                assert!(cmd.rect.w <= 2.5);
                // Should span full tab bar height
                assert_eq!(cmd.rect.h, layout.tab_bar_h);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_tab_bar_opacity_applied() {
        let snapshot = make_test_snapshot(1, 0, 0.5);
        let layout = make_test_layout(20.0);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        TabBar::render(&ctx, &mut scene);

        // All rects should have reduced alpha due to opacity
        for command in &scene.main {
            match command {
                RenderCommand::Rect(cmd) => {
                    // With opacity=0.5: frosted_backdrop_alpha(0.5) = 0.775
                    // So alpha should be less than or equal to original color's alpha
                    assert!(cmd.color[3] <= 1.0);
                }
                _ => panic!("Expected Rect command"),
            }
        }
    }

    #[test]
    fn test_clamp_color_values() {
        let color = [0.5, 0.5, 0.5, 1.0];
        let clamped = clamp_color(color, 0.1);
        assert_eq!(clamped[0], 0.6);
        assert_eq!(clamped[1], 0.6);
        assert_eq!(clamped[2], 0.6);
        assert_eq!(clamped[3], 1.0); // Alpha unchanged

        // Test clamping at boundaries
        let max_color = [0.95, 0.95, 0.95, 1.0];
        let over_clamped = clamp_color(max_color, 0.1);
        assert_eq!(over_clamped[0], 1.0);
        assert_eq!(over_clamped[1], 1.0);
        assert_eq!(over_clamped[2], 1.0);
    }

    #[test]
    fn test_mix_color_values() {
        let a = [0.0, 0.0, 0.0, 1.0];
        let b = [1.0, 1.0, 1.0, 1.0];

        let mixed_half = mix_color(a, b, 0.5);
        assert_eq!(mixed_half[0], 0.5);
        assert_eq!(mixed_half[1], 0.5);
        assert_eq!(mixed_half[2], 0.5);

        let mixed_zero = mix_color(a, b, 0.0);
        assert_eq!(mixed_zero, a);

        let mixed_one = mix_color(a, b, 1.0);
        assert_eq!(mixed_one, b);
    }

    #[test]
    fn test_frosted_backdrop_alpha() {
        assert_eq!(frosted_backdrop_alpha(1.0), 1.0);
        assert_eq!(frosted_backdrop_alpha(0.0), 0.55);
        assert_eq!(frosted_backdrop_alpha(0.5), 0.775);
    }

    #[test]
    fn test_tab_bar_all_rects() {
        let snapshot = make_test_snapshot(2, 1, 1.0);
        let layout = make_test_layout(20.0);
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        TabBar::render(&ctx, &mut scene);

        // All commands should be Rect (no Text or Clip commands)
        for command in &scene.main {
            match command {
                RenderCommand::Rect(_) => {}
                _ => panic!("Expected only Rect commands, got {:?}", command),
            }
        }
    }
}
