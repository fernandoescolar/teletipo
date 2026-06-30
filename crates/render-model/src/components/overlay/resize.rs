/// Resize overlay: visual indicator when user is resizing panes.
///
/// Rendered in the Overlay layer as a centered panel with the new terminal size.
use crate::{Color, RenderContext, Scene, SceneLayer};

/// Render resize overlay based on snapshot state.
/// Called from GlPainter to emit overlay geometry into the Scene.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(text) = &ctx.snapshot.resize_overlay else {
        return;
    };
    if text.is_empty() {
        return;
    };

    let layout = ctx.layout;
    let border_w = 1.0;
    let panel_w = (text.chars().count() as f32 * layout.cell_w_px + layout.cell_w_px * 2.0)
        .min(layout.width * 0.8);
    let panel_h = layout.cell_h_px * 1.4;
    let x = (layout.width - panel_w) * 0.5;
    let y = (layout.height - panel_h) * 0.5;

    let border_color: Color = [0.35, 0.55, 0.90, 1.0];
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x - border_w,
        y - border_w,
        panel_w + border_w * 2.0,
        panel_h + border_w * 2.0,
        border_color,
    );

    let bg_color: Color = [0.08, 0.10, 0.16, 1.0];
    scene.rect_to_layer(SceneLayer::Overlay, x, y, panel_w, panel_h, bg_color);

    let text_color: Color = [0.92, 0.94, 0.98, 1.0];
    let text_x = x + (panel_w - text.chars().count() as f32 * layout.cell_w_px) * 0.5;
    let text_y = y + (panel_h - layout.cell_h_px) * 0.5;
    scene.text_to_layer(SceneLayer::Overlay, text_x, text_y, text, text_color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CellMetrics, DamageRegion, FrameLayout, RenderCommand, RenderRow, RenderSnapshot,
        RenderTarget,
    };
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
        snapshot.resize_overlay = Some("50/50".to_string()); // Active resize
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render(&ctx, &mut scene);

        // Should have border + panel + text = 3 commands
        assert_eq!(scene.overlay.len(), 3);

        assert!(matches!(scene.overlay[0], RenderCommand::Rect(_)));
        assert!(matches!(scene.overlay[1], RenderCommand::Rect(_)));
        assert!(matches!(scene.overlay[2], RenderCommand::Text(_)));
    }
}
