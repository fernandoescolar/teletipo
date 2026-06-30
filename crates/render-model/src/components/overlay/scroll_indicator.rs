/// Scroll indicator: visual representation of scroll position within pane.
///
/// Rendered in the Overlay layer to show how much content is visible.
/// Can be a scrollbar-like indicator or a simpler position marker.

use crate::{RenderContext, Scene, SceneLayer, Color};

/// Configuration for scroll indicator appearance.
#[derive(Debug, Clone, Copy)]
pub struct ScrollIndicatorStyle {
    /// Indicator color
    pub color: Color,
    /// Background track color (if shown)
    pub track_color: Option<Color>,
    /// Width of scrollbar (in pixels)
    pub width: f32,
}

impl Default for ScrollIndicatorStyle {
    fn default() -> Self {
        ScrollIndicatorStyle {
            color: [0.6, 0.6, 0.6, 0.6],
            track_color: Some([0.2, 0.2, 0.2, 0.3]),
            width: 8.0,
        }
    }
}

/// Render a vertical scroll indicator (scrollbar-like).
pub fn render_vertical_scroll_indicator(
    _ctx: &RenderContext,
    scene: &mut Scene,
    x: f32,
    y: f32,
    height: f32,
    scroll_position: f32,      // 0.0 to 1.0: position from top
    visible_portion: f32,       // 0.0 to 1.0: what fraction is visible
    style: ScrollIndicatorStyle,
) {
    // Draw track (optional background)
    if let Some(track_color) = style.track_color {
        scene.rect_to_layer(SceneLayer::Overlay, x, y, style.width, height, track_color);
    }

    // Draw indicator thumb (the actual scrollbar position)
    let thumb_height = (height * visible_portion).max(4.0); // Minimum thumb height
    let thumb_y = y + (height - thumb_height) * scroll_position;

    scene.rect_to_layer(SceneLayer::Overlay, x, thumb_y, style.width, thumb_height, style.color);
}

/// Render a horizontal scroll indicator.
pub fn render_horizontal_scroll_indicator(
    _ctx: &RenderContext,
    scene: &mut Scene,
    x: f32,
    y: f32,
    width: f32,
    scroll_position: f32,      // 0.0 to 1.0: position from left
    visible_portion: f32,       // 0.0 to 1.0: what fraction is visible
    style: ScrollIndicatorStyle,
) {
    // Draw track (optional background)
    if let Some(track_color) = style.track_color {
        scene.rect_to_layer(SceneLayer::Overlay, x, y, width, style.width, track_color);
    }

    // Draw indicator thumb
    let thumb_width = (width * visible_portion).max(4.0); // Minimum thumb width
    let thumb_x = x + (width - thumb_width) * scroll_position;

    scene.rect_to_layer(SceneLayer::Overlay, thumb_x, y, thumb_width, style.width, style.color);
}

/// Render a simple position indicator (small marker instead of full scrollbar).
/// Useful for showing scroll position without taking up much space.
pub fn render_position_indicator(
    _ctx: &RenderContext,
    scene: &mut Scene,
    x: f32,
    y: f32,
    height: f32,
    scroll_position: f32,  // 0.0 to 1.0
    color: Color,
) {
    let indicator_height = 2.0;
    let position_y = y + (height - indicator_height) * scroll_position;

    scene.rect_to_layer(SceneLayer::Overlay, x, position_y, 1.0, indicator_height, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CellMetrics, DamageRegion, FrameLayout, RenderCommand, RenderRow, RenderSnapshot, RenderTarget};
    use std::sync::Arc;

    fn make_test_snapshot() -> RenderSnapshot {
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
            opacity: 1.0,
        }
    }

    fn make_test_layout() -> FrameLayout {
        FrameLayout {
            width: 800.0,
            height: 600.0,
            tab_bar_h: 0.0,
            terminal_h: 300.0,
            editor_top: 302.0,
            terminal_text_top: 4.0,
            terminal_text_bottom: 296.0,
            padding_h: 8.0,
            padding_v: 4.0,
            cell_w_px: 10.0,
            cell_h_px: 20.0,
        }
    }

    #[test]
    fn test_render_vertical_scroll_at_top() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        let style = ScrollIndicatorStyle::default();
        render_vertical_scroll_indicator(&ctx, &mut scene, 790.0, 0.0, 600.0, 0.0, 0.5, style);

        // Should have track + indicator = 2 rects
        assert_eq!(scene.overlay.len(), 2);

        match &scene.overlay[1] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.y, 0.0); // At top
                assert_eq!(cmd.rect.h, 300.0); // 50% of 600
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_render_vertical_scroll_at_middle() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        let style = ScrollIndicatorStyle::default();
        render_vertical_scroll_indicator(&ctx, &mut scene, 790.0, 0.0, 600.0, 0.5, 0.5, style);

        match &scene.overlay[1] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.y, 150.0); // Centered
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_render_horizontal_scroll_indicator() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        let style = ScrollIndicatorStyle::default();
        render_horizontal_scroll_indicator(&ctx, &mut scene, 0.0, 590.0, 800.0, 0.3, 0.5, style);

        // Should have track + indicator = 2 rects
        assert_eq!(scene.overlay.len(), 2);

        match &scene.overlay[1] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.y, 590.0);
                assert_eq!(cmd.rect.w, 400.0); // 50% of 800
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_render_position_indicator() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        let color = [0.8, 0.8, 0.8, 0.8];
        render_position_indicator(&ctx, &mut scene, 790.0, 0.0, 600.0, 0.0, color);

        // Should have 1 rect (just the position indicator)
        assert_eq!(scene.overlay.len(), 1);

        match &scene.overlay[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.y, 0.0);
                assert_eq!(cmd.rect.h, 2.0);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_scroll_indicator_minimum_thumb_height() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        let style = ScrollIndicatorStyle::default();
        // Very small visible portion (0.001) -> should be clamped to minimum
        render_vertical_scroll_indicator(&ctx, &mut scene, 790.0, 0.0, 600.0, 0.5, 0.001, style);

        match &scene.overlay[1] {
            RenderCommand::Rect(cmd) => {
                // Minimum height is 4.0 (600 * 0.001 = 0.6, clamped to 4.0)
                assert_eq!(cmd.rect.h, 4.0);
            }
            _ => panic!("Expected Rect command"),
        }
    }
}
