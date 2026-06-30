/// Editor background: pane background and disabled dimming overlay.

use crate::{RenderContext, Scene, SceneLayer};

/// Emit editor background rectangle.
/// Applies disabled dimming if editor is not focused/enabled.
pub fn render_background(ctx: &RenderContext, scene: &mut Scene) {
    let snapshot = ctx.snapshot;
    let layout = ctx.layout;
    let theme = ctx.theme;

    // Apply window opacity
    let backdrop = frosted_backdrop_alpha(snapshot.opacity);

    // Editor background color
    let editor_bg = if snapshot.editor_disabled {
        let [r, g, b, a] = theme.editor_bg;
        [r * 0.55, g * 0.55, b * 0.55, a]
    } else {
        theme.editor_bg
    };

    let color = [
        editor_bg[0],
        editor_bg[1],
        editor_bg[2],
        (editor_bg[3] * backdrop).clamp(0.0, 1.0),
    ];

    scene.rect_to_layer(
        SceneLayer::Main,
        0.0,
        layout.editor_top,
        layout.width,
        layout.height - layout.editor_top,
        color,
    );
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

    fn make_test_snapshot(editor_disabled: bool, opacity: f32) -> RenderSnapshot {
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
            editor_disabled,
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
            opacity,
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
    fn test_editor_background_normal() {
        let snapshot = make_test_snapshot(false, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_background(&ctx, &mut scene);

        assert_eq!(scene.main.len(), 1);

        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 0.0);
                assert_eq!(cmd.rect.y, layout.editor_top);
                assert_eq!(cmd.rect.w, layout.width);
                assert_eq!(cmd.rect.h, layout.height - layout.editor_top);
                // Color should match theme (not dimmed)
                let original = ctx.theme.editor_bg;
                assert!((cmd.color[0] - original[0]).abs() < 0.01);
                assert!((cmd.color[1] - original[1]).abs() < 0.01);
                assert!((cmd.color[2] - original[2]).abs() < 0.01);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_editor_background_disabled() {
        let snapshot = make_test_snapshot(true, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_background(&ctx, &mut scene);

        assert_eq!(scene.main.len(), 1);

        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                let original = ctx.theme.editor_bg;
                // Color should be dimmed (×0.55)
                assert!((cmd.color[0] - original[0] * 0.55).abs() < 0.01);
                assert!((cmd.color[1] - original[1] * 0.55).abs() < 0.01);
                assert!((cmd.color[2] - original[2] * 0.55).abs() < 0.01);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_editor_background_opacity_applied() {
        let snapshot = make_test_snapshot(false, 0.5);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_background(&ctx, &mut scene);

        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                let original = ctx.theme.editor_bg;
                // With opacity=0.5: frosted_backdrop_alpha(0.5) = 0.775
                let expected_alpha = original[3] * 0.775;
                assert!((cmd.color[3] - expected_alpha).abs() < 0.01);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_editor_background_position() {
        let snapshot = make_test_snapshot(false, 1.0);
        let mut layout = make_test_layout();
        layout.editor_top = 350.0;

        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_background(&ctx, &mut scene);

        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.y, 350.0);
                assert_eq!(cmd.rect.h, 600.0 - 350.0);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_editor_background_disabled_and_opacity() {
        let snapshot = make_test_snapshot(true, 0.75);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_background(&ctx, &mut scene);

        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                let original = ctx.theme.editor_bg;
                // Dimmed: ×0.55
                // Opacity: ×frosted_backdrop_alpha(0.75) = 0.8875
                let expected_r = (original[0] * 0.55 * 0.8875).clamp(0.0, 1.0);
                let expected_a = (original[3] * 0.8875).clamp(0.0, 1.0);

                assert!((cmd.color[0] - expected_r).abs() < 0.01);
                assert!((cmd.color[3] - expected_a).abs() < 0.01);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_editor_background_is_rect_command() {
        let snapshot = make_test_snapshot(false, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        render_background(&ctx, &mut scene);

        // Verify it's a Rect command (not Text/Clip)
        match &scene.main[0] {
            RenderCommand::Rect(_) => {}
            _ => panic!("Expected Rect command"),
        }
    }
}
