/// Scene builder: backend-independent function to construct scenes from snapshots.

use crate::{Background, CellMetrics, Editor, FrameLayout, RenderSnapshot, RenderTarget, Scene, TabBar, Terminal};
use crate::theme::ColorTheme;

/// Rendering context: immutable data passed to all component render functions.
#[derive(Debug, Clone)]
pub struct RenderContext<'a> {
    pub snapshot: &'a RenderSnapshot,
    pub layout: &'a FrameLayout,
    pub target: RenderTarget,
    pub metrics: CellMetrics,
    pub theme: &'a ColorTheme,
}

impl<'a> RenderContext<'a> {
    /// Create a new render context from snapshot and layout.
    pub fn new(
        snapshot: &'a RenderSnapshot,
        layout: &'a FrameLayout,
        target: RenderTarget,
        metrics: CellMetrics,
    ) -> Self {
        RenderContext {
            snapshot,
            layout,
            target,
            metrics,
            theme: &snapshot.theme,
        }
    }
}

/// Build a scene with background, tab bar, terminal, editor, and basic geometry.
/// This emits rectangles for backgrounds, tab bars, terminal cell backgrounds, editor background, separator, and bell overlay.
/// Additional components (text, overlays, etc.) are rendered separately.
pub fn build_scene(
    snapshot: &RenderSnapshot,
    layout: &FrameLayout,
    target: RenderTarget,
    metrics: CellMetrics,
) -> Scene {
    let ctx = RenderContext::new(snapshot, layout, target, metrics);
    let mut scene = Scene::new();

    // Emit background component
    Background::render(&ctx, &mut scene);

    // Emit tab bar component (backgrounds only; text uses old path)
    TabBar::render(&ctx, &mut scene);

    // Emit terminal component (background colors only; text uses old path)
    Terminal::render(&ctx, &mut scene);

    // Emit editor component (background only; text, cursor, selection use old path)
    Editor::render(&ctx, &mut scene);

    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DamageRegion, RenderRow, compute_frame_layout};
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
    fn test_build_scene_creates_scene() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let scene = build_scene(&snapshot, &layout, target, metrics);

        // Should have commands from the background component
        assert!(!scene.is_empty());
    }

    #[test]
    fn test_render_context_new() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        assert_eq!(ctx.target.width, 800.0);
        assert_eq!(ctx.target.height, 600.0);
        assert_eq!(ctx.metrics.width, 10.0);
        assert_eq!(ctx.metrics.height, 20.0);
        assert!(std::ptr::eq(ctx.snapshot, &snapshot));
        assert!(std::ptr::eq(ctx.layout, &layout));
        assert!(std::ptr::eq(ctx.theme, &snapshot.theme));
    }

    #[test]
    fn test_build_scene_has_all_layers() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let scene = build_scene(&snapshot, &layout, target, metrics);

        // Should have background commands from components
        assert!(!scene.background.is_empty(), "Background layer should not be empty");
        // Should have main layer commands from tab bar, terminal, editor
        assert!(!scene.main.is_empty(), "Main layer should not be empty");
        // Other layers may be empty depending on snapshot state
    }

    #[test]
    fn test_build_scene_layer_ordering() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let scene = build_scene(&snapshot, &layout, target, metrics);

        // Verify we can iterate through layers in order
        let mut layer_count = 0;
        for (_layer, commands) in scene.iter_layers() {
            if !commands.is_empty() {
                layer_count += 1;
            }
        }

        assert!(layer_count > 0, "At least one layer should have commands");
    }

    #[test]
    fn test_build_scene_total_command_count() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let scene = build_scene(&snapshot, &layout, target, metrics);

        // Should have at least some commands from background component
        assert!(scene.len() > 0, "Scene should have commands from components");
    }

    #[test]
    fn test_build_scene_with_different_layouts() {
        let snapshot = make_test_snapshot();
        let target = RenderTarget::new(1024.0, 768.0);
        let metrics = CellMetrics::new(8.0, 16.0);
        let layout = compute_frame_layout(&snapshot, target, metrics);

        let scene = build_scene(&snapshot, &layout, target, metrics);

        // Should still create a valid scene with different dimensions
        assert!(!scene.is_empty());
        assert!(scene.background.len() > 0);
    }

    #[test]
    fn test_build_scene_preserves_layer_semantics() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);

        let scene = build_scene(&snapshot, &layout, target, metrics);

        // Background should be layer 0 (rendered first)
        assert!(!scene.background.is_empty(), "Background layer should have content");

        // Main should be layer 1
        assert!(!scene.main.is_empty(), "Main layer should have content");

        // Verify total count equals sum of all layers
        let sum_of_layers = scene.background.len() + scene.main.len() + scene.floating.len() +
                           scene.overlay.len() + scene.toast.len() + scene.debug.len();
        assert_eq!(scene.len(), sum_of_layers);
    }
}
