/// Background component: renders pane backgrounds, separator, and bell overlay.
/// Emits Scene commands without calling OpenGL directly.
use crate::{RenderContext, Scene, SceneLayer};

/// Marker struct for the background component (used for organization).
pub struct Background;

impl Background {
    /// Emit background-related render commands (terminal bg, editor bg, separator, bell).
    pub fn render(ctx: &RenderContext, scene: &mut Scene) {
        render_backgrounds(ctx, scene);
    }
}

/// Emit background rectangles: terminal, editor, separator, bell overlay.
fn render_backgrounds(ctx: &RenderContext, scene: &mut Scene) {
    let layout = ctx.layout;
    let snapshot = ctx.snapshot;
    let theme = ctx.theme;

    // Apply window opacity to all background colors
    let apply_opacity = |color: [f32; 4]| -> [f32; 4] {
        let mut c = color;
        c[3] = (c[3] * frosted_backdrop_alpha(snapshot.opacity)).clamp(0.0, 1.0);
        c
    };

    // Terminal background: from tab bar to terminal bottom
    scene.rect_to_layer(
        SceneLayer::Background,
        0.0,
        layout.tab_bar_h,
        layout.width,
        layout.terminal_h - layout.tab_bar_h,
        apply_opacity(theme.terminal_bg),
    );

    // Editor background: from editor top to screen bottom
    let editor_bg = if snapshot.editor_disabled {
        let [r, g, b, a] = theme.editor_bg;
        [r * 0.55, g * 0.55, b * 0.55, a]
    } else {
        theme.editor_bg
    };
    scene.rect_to_layer(
        SceneLayer::Background,
        0.0,
        layout.editor_top,
        layout.width,
        layout.height - layout.editor_top,
        apply_opacity(editor_bg),
    );

    // Separator: between terminal and editor
    let separator_color = if snapshot.editor_focused {
        theme.separator_focused
    } else {
        theme.separator
    };
    scene.rect_to_layer(
        SceneLayer::Background,
        0.0,
        layout.terminal_h,
        layout.width,
        layout.editor_top - layout.terminal_h,
        apply_opacity(separator_color),
    );

    // Bell overlay: brief red tint if bell was triggered
    if snapshot.bell_active {
        scene.rect_to_layer(
            SceneLayer::Background,
            0.0,
            layout.tab_bar_h,
            layout.width,
            layout.terminal_h - layout.tab_bar_h,
            apply_opacity([0.60, 0.20, 0.20, 0.15]),
        );
    }
}

/// Calculate frosted backdrop alpha.
/// Keep backgrounds translucent but not crystal-clear when opacity is low.
/// This approximates a blur/frosted effect on compositors without real blur.
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
        editor_disabled: bool,
        editor_focused: bool,
        bell_active: bool,
        opacity: f32,
    ) -> RenderSnapshot {
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
            editor_focused,
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
            sticky_command_overlay: None,
            terminal_links: Vec::new(),
            request_exit: false,
            cursor_shape: 0,
            bell_active,
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
    fn test_background_render_normal() {
        let snapshot = make_test_snapshot(false, false, false, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Should have 3 commands: terminal bg, editor bg, separator
        assert_eq!(scene.len(), 3);

        // All should be Rect commands
        for command in &scene.background {
            match command {
                RenderCommand::Rect(_) => {}
                _ => panic!("Expected Rect command"),
            }
        }
    }

    #[test]
    fn test_background_render_with_bell() {
        let snapshot = make_test_snapshot(false, false, true, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Should have 4 commands with bell
        assert_eq!(scene.len(), 4);
    }

    #[test]
    fn test_background_terminal_background() {
        let snapshot = make_test_snapshot(false, false, false, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // First rect should be terminal background (in Background layer)
        match &scene.background[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 0.0);
                assert_eq!(cmd.rect.y, layout.tab_bar_h);
                assert_eq!(cmd.rect.w, layout.width);
                assert_eq!(cmd.rect.h, layout.terminal_h - layout.tab_bar_h);
                // Color should match theme with opacity applied
                assert_eq!(cmd.color, ctx.theme.terminal_bg);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_background_editor_background() {
        let snapshot = make_test_snapshot(false, false, false, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Second rect should be editor background
        match &scene.background[1] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 0.0);
                assert_eq!(cmd.rect.y, layout.editor_top);
                assert_eq!(cmd.rect.w, layout.width);
                assert_eq!(cmd.rect.h, layout.height - layout.editor_top);
                // Should match normal editor bg (not dimmed)
                assert_eq!(cmd.color, ctx.theme.editor_bg);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_background_editor_disabled() {
        let snapshot = make_test_snapshot(true, false, false, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Second rect should have dimmed editor background
        match &scene.background[1] {
            RenderCommand::Rect(cmd) => {
                let original = ctx.theme.editor_bg;
                // Dimmed: RGB * 0.55
                assert!(cmd.color[0] < original[0]);
                assert!(cmd.color[1] < original[1]);
                assert!(cmd.color[2] < original[2]);
                // Alpha still affected by opacity but the RGB should be clearly dimmed
                assert!((cmd.color[0] - original[0] * 0.55).abs() < 0.01);
                assert!((cmd.color[1] - original[1] * 0.55).abs() < 0.01);
                assert!((cmd.color[2] - original[2] * 0.55).abs() < 0.01);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_background_separator_unfocused() {
        let snapshot = make_test_snapshot(false, false, false, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Third rect should be separator with unfocused color
        match &scene.background[2] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 0.0);
                assert_eq!(cmd.rect.y, layout.terminal_h);
                assert_eq!(cmd.rect.w, layout.width);
                assert_eq!(cmd.rect.h, layout.editor_top - layout.terminal_h);
                // Color should be unfocused separator
                assert_eq!(cmd.color, ctx.theme.separator);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_background_separator_focused() {
        let snapshot = make_test_snapshot(false, true, false, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Third rect should be separator with focused color
        match &scene.background[2] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.color, ctx.theme.separator_focused);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_background_bell_overlay() {
        let snapshot = make_test_snapshot(false, false, true, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Fourth rect should be bell overlay
        match &scene.background[3] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 0.0);
                assert_eq!(cmd.rect.y, layout.tab_bar_h);
                assert_eq!(cmd.rect.w, layout.width);
                assert_eq!(cmd.rect.h, layout.terminal_h - layout.tab_bar_h);
                // Should be reddish with low alpha
                let [r, g, b, a] = cmd.color;
                assert!(r > g); // Red > green
                assert!(r > b); // Red > blue
                assert!(a < 0.2); // Very transparent
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_background_opacity_half() {
        let snapshot = make_test_snapshot(false, false, false, 0.5);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Terminal bg should have reduced alpha
        match &scene.background[0] {
            RenderCommand::Rect(cmd) => {
                let original_alpha = ctx.theme.terminal_bg[3];
                let expected_alpha = original_alpha * (0.55 + 0.45 * 0.5); // frosted_backdrop_alpha(0.5)
                assert!((cmd.color[3] - expected_alpha).abs() < 0.01);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_background_opacity_zero() {
        let snapshot = make_test_snapshot(false, false, false, 0.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Terminal bg should have frosted alpha (0.55)
        match &scene.background[0] {
            RenderCommand::Rect(cmd) => {
                let original_alpha = ctx.theme.terminal_bg[3];
                let expected_alpha = original_alpha * 0.55; // frosted_backdrop_alpha(0.0)
                assert!((cmd.color[3] - expected_alpha).abs() < 0.01);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_background_with_tabs() {
        let snapshot = make_test_snapshot(false, false, false, 1.0);
        let mut layout = make_test_layout();
        layout.tab_bar_h = 20.0;
        layout.terminal_h = 440.0;
        layout.editor_top = 442.0;

        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Terminal background should start below tab bar
        match &scene.background[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.y, layout.tab_bar_h);
                assert_eq!(cmd.rect.h, layout.terminal_h - layout.tab_bar_h);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_frosted_backdrop_alpha_values() {
        assert_eq!(frosted_backdrop_alpha(1.0), 1.0);
        assert_eq!(frosted_backdrop_alpha(0.0), 0.55);
        assert_eq!(frosted_backdrop_alpha(0.5), 0.775);
        assert_eq!(frosted_backdrop_alpha(2.0), 1.0); // clamped
        assert_eq!(frosted_backdrop_alpha(-1.0), 0.55); // clamped
    }

    #[test]
    fn test_background_all_disabled_and_focused() {
        // Edge case: editor disabled AND focused
        let snapshot = make_test_snapshot(true, true, false, 1.0);
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        Background::render(&ctx, &mut scene);

        // Should still work correctly
        assert_eq!(scene.len(), 3);

        // Editor should be dimmed
        match &scene.background[1] {
            RenderCommand::Rect(cmd) => {
                assert!(cmd.color[0] < ctx.theme.editor_bg[0]);
            }
            _ => panic!("Expected Rect command"),
        }

        // Separator should be focused color
        match &scene.background[2] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.color, ctx.theme.separator_focused);
            }
            _ => panic!("Expected Rect command"),
        }
    }
}
