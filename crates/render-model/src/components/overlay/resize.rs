/// Resize overlay: visual indicator when user is resizing panes.
///
/// Rendered in the Overlay layer to show the active resize boundary.
/// Text rendering is deferred to the old painter path.

use crate::{RenderContext, Scene, SceneLayer, Color};

/// Render a resize overlay indicator (visual feedback during drag-to-resize).
pub fn render_resize_overlay(
    _ctx: &RenderContext,
    scene: &mut Scene,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    // Main resize area (semi-transparent overlay)
    let overlay_color: Color = [0.3, 0.5, 0.8, 0.2];  // Semi-transparent blue
    scene.rect_to_layer(SceneLayer::Overlay, x, y, width, height, overlay_color);

    // Border to make it visible
    let border_color: Color = [0.6, 0.7, 0.9, 0.5];
    let border_width = 2.0;

    // Top border
    scene.rect_to_layer(SceneLayer::Overlay, x, y, width, border_width, border_color);
    // Bottom border
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x,
        y + height - border_width,
        width,
        border_width,
        border_color,
    );
    // Left border
    scene.rect_to_layer(SceneLayer::Overlay, x, y, border_width, height, border_color);
    // Right border
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x + width - border_width,
        y,
        border_width,
        height,
        border_color,
    );

    // TODO: Text rendering (e.g., "Split: 70/30")
}

/// Render horizontal split resize indicator.
pub fn render_horizontal_split_resize(
    _ctx: &RenderContext,
    scene: &mut Scene,
    x: f32,
    y: f32,
    width: f32,
) {
    let height = 4.0;
    let color: Color = [0.6, 0.7, 0.9, 0.8];

    scene.rect_to_layer(SceneLayer::Overlay, x, y - height / 2.0, width, height, color);

    // TODO: Cursor indication (e.g., resize cursor icon)
}

/// Render vertical split resize indicator.
pub fn render_vertical_split_resize(
    _ctx: &RenderContext,
    scene: &mut Scene,
    x: f32,
    y: f32,
    height: f32,
) {
    let width = 4.0;
    let color: Color = [0.6, 0.7, 0.9, 0.8];

    scene.rect_to_layer(SceneLayer::Overlay, x - width / 2.0, y, width, height, color);

    // TODO: Cursor indication (e.g., resize cursor icon)
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
    fn test_render_resize_overlay() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_resize_overlay(&ctx, &mut scene, 0.0, 300.0, 800.0, 2.0);

        // Should have 1 background + 4 borders = 5 rects
        assert_eq!(scene.overlay.len(), 5);

        // Verify all are Rect commands
        for command in &scene.overlay {
            match command {
                RenderCommand::Rect(_) => {}
                _ => panic!("Expected Rect command"),
            }
        }
    }

    #[test]
    fn test_render_horizontal_split_resize() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_horizontal_split_resize(&ctx, &mut scene, 0.0, 300.0, 800.0);

        // Should have 1 rect
        assert_eq!(scene.overlay.len(), 1);

        match &scene.overlay[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.y, 300.0 - 2.0); // Centered at y=300
                assert_eq!(cmd.rect.h, 4.0);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_render_vertical_split_resize() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_vertical_split_resize(&ctx, &mut scene, 400.0, 0.0, 600.0);

        // Should have 1 rect
        assert_eq!(scene.overlay.len(), 1);

        match &scene.overlay[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 400.0 - 2.0); // Centered at x=400
                assert_eq!(cmd.rect.w, 4.0);
            }
            _ => panic!("Expected Rect command"),
        }
    }
}
