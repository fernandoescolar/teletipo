/// Resize overlay: visual indicator when user is resizing panes.
///
/// Rendered in the Overlay layer to show the active resize boundary when dragging to resize.
/// Emits geometry only; text rendering is deferred to the old painter path.

use crate::{RenderContext, Scene, SceneLayer, Color, FrameLayout, RenderSnapshot};

/// Render resize overlay based on snapshot state.
/// Called from GlPainter to emit overlay geometry into the Scene.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    // Only render if there's an active resize
    if ctx.snapshot.resize_overlay.is_none() {
        return;
    }

    let layout = ctx.layout;
    let w = 8.0; // Simple indicator width
    let h = layout.height - layout.tab_bar_h - layout.cell_h_px;
    let x = layout.width * 0.5 - w * 0.5;
    let y = layout.tab_bar_h;

    // Main overlay area (semi-transparent blue)
    let overlay_color: Color = [0.3, 0.5, 0.8, 0.15];
    scene.rect_to_layer(SceneLayer::Overlay, x, y, w, h, overlay_color);

    // Border
    let border_color: Color = [0.6, 0.7, 0.9, 0.6];
    let border_width = 1.0;

    // Left border
    scene.rect_to_layer(SceneLayer::Overlay, x, y, border_width, h, border_color);
    // Right border
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x + w - border_width,
        y,
        border_width,
        h,
        border_color,
    );
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
        let mut snapshot = make_test_snapshot();
        snapshot.resize_overlay = Some("50/50".to_string());  // Active resize
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render(&ctx, &mut scene);

        // Should have 1 background + 2 borders (left + right) = 3 rects
        assert_eq!(scene.overlay.len(), 3);

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
