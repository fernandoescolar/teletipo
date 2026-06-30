/// Overlay components: settings, keybindings, command palette, modals, resize, scroll.
///
/// These emit to SceneLayer::Overlay and render on top of main content.
/// Text rendering is deferred to the old painter path for now.

pub mod resize;
pub mod scroll_indicator;

use crate::{RenderContext, Scene, SceneLayer};
use crate::components::panel::{render_panel, PanelStyle};
use crate::Rect;

/// Render a simple modal/overlay panel with title area.
/// Demonstrates the pattern for overlay rendering.
pub fn render_modal_frame(
    ctx: &RenderContext,
    scene: &mut Scene,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    let style = PanelStyle {
        bg: [0.15, 0.15, 0.18, 0.95],
        border: Some([0.5, 0.5, 0.5, 0.8]),
        border_width: 1.0,
    };

    let rect = Rect::new(x, y, width, height);
    render_panel(scene, SceneLayer::Overlay, rect, style);

    // Title bar (slightly darker)
    let title_h = ctx.metrics.height * 1.5;
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x,
        y,
        width,
        title_h,
        [0.10, 0.10, 0.12, 0.95],
    );

    // TODO: Title text rendering
    // TODO: Close button (if applicable)
}

/// Render a dropdown/suggestion panel.
/// Used by suggestion dropdowns, context menus, etc.
pub fn render_dropdown_panel(
    _ctx: &RenderContext,
    scene: &mut Scene,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    layer: SceneLayer,
) {
    let style = PanelStyle {
        bg: [0.20, 0.20, 0.23, 0.95],
        border: Some([0.4, 0.4, 0.4, 0.8]),
        border_width: 1.0,
    };

    let rect = Rect::new(x, y, width, height);
    render_panel(scene, layer, rect, style);

    // TODO: Menu items/rows rendering
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
    fn test_render_modal_frame() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_modal_frame(&ctx, &mut scene, 100.0, 100.0, 400.0, 300.0);

        // Should have frame (1 bg + 4 borders + title bar)
        assert!(scene.overlay.len() >= 6);

        // Verify it's all Rect commands
        for command in &scene.overlay {
            match command {
                RenderCommand::Rect(_) => {}
                _ => panic!("Expected Rect command"),
            }
        }
    }

    #[test]
    fn test_render_dropdown_panel_floating() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_dropdown_panel(&ctx, &mut scene, 50.0, 50.0, 200.0, 150.0, SceneLayer::Floating);

        // Should have background + 4 borders
        assert_eq!(scene.floating.len(), 5);
    }

    #[test]
    fn test_render_dropdown_panel_overlay() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_dropdown_panel(&ctx, &mut scene, 50.0, 50.0, 200.0, 150.0, SceneLayer::Overlay);

        // Should have background + 4 borders in overlay layer
        assert_eq!(scene.overlay.len(), 5);
    }
}
