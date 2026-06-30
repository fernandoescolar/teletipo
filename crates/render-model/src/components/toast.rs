/// Toast notifications: transient, temporary UI messages.
///
/// Toasts are rendered in the Toast layer and emit to the bottom-right corner (or other positions).
/// Text rendering is deferred to the old painter path.

use crate::{RenderContext, Scene, SceneLayer, Color, Rect};
use crate::components::panel::{render_panel, PanelStyle};

/// Possible toast positions on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastPosition {
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
    Center,
}

impl ToastPosition {
    /// Compute the rect for a toast at this position.
    /// Toast width/height are provided; position determines alignment.
    pub fn compute_rect(&self, screen_width: f32, screen_height: f32, toast_w: f32, toast_h: f32, margin: f32) -> Rect {
        match self {
            ToastPosition::BottomRight => {
                Rect::new(screen_width - toast_w - margin, screen_height - toast_h - margin, toast_w, toast_h)
            }
            ToastPosition::BottomLeft => {
                Rect::new(margin, screen_height - toast_h - margin, toast_w, toast_h)
            }
            ToastPosition::TopRight => {
                Rect::new(screen_width - toast_w - margin, margin, toast_w, toast_h)
            }
            ToastPosition::TopLeft => {
                Rect::new(margin, margin, toast_w, toast_h)
            }
            ToastPosition::Center => {
                Rect::new(
                    (screen_width - toast_w) / 2.0,
                    (screen_height - toast_h) / 2.0,
                    toast_w,
                    toast_h,
                )
            }
        }
    }
}

/// Toast style configuration.
#[derive(Debug, Clone, Copy)]
pub struct ToastStyle {
    pub bg: Color,
    pub border: Option<Color>,
    pub border_width: f32,
}

impl Default for ToastStyle {
    fn default() -> Self {
        ToastStyle {
            bg: [0.2, 0.6, 0.2, 0.9],  // Green
            border: Some([0.4, 0.8, 0.4, 0.9]),
            border_width: 1.0,
        }
    }
}

/// Render a single toast notification.
pub fn render_toast(
    ctx: &RenderContext,
    scene: &mut Scene,
    position: ToastPosition,
    width: f32,
    height: f32,
    style: ToastStyle,
    margin: f32,
) {
    let rect = position.compute_rect(ctx.target.width, ctx.target.height, width, height, margin);

    // Use panel helper to draw background and border
    let panel_style = PanelStyle {
        bg: style.bg,
        border: style.border,
        border_width: style.border_width,
    };

    render_panel(scene, SceneLayer::Toast, rect, panel_style);

    // TODO: Toast text rendering
    // TODO: Toast dismiss button
}

/// Render multiple toasts (stacked).
pub fn render_toast_stack(
    ctx: &RenderContext,
    scene: &mut Scene,
    toasts: &[ToastInfo],
    position: ToastPosition,
    margin: f32,
) {
    let spacing = 8.0; // Vertical spacing between toasts
    let mut y_offset = 0.0;

    for toast in toasts {
        let rect = match position {
            ToastPosition::BottomRight => {
                Rect::new(
                    ctx.target.width - toast.width - margin,
                    ctx.target.height - toast.height - margin - y_offset,
                    toast.width,
                    toast.height,
                )
            }
            ToastPosition::BottomLeft => {
                Rect::new(
                    margin,
                    ctx.target.height - toast.height - margin - y_offset,
                    toast.width,
                    toast.height,
                )
            }
            ToastPosition::TopRight => {
                Rect::new(
                    ctx.target.width - toast.width - margin,
                    margin + y_offset,
                    toast.width,
                    toast.height,
                )
            }
            ToastPosition::TopLeft => {
                Rect::new(
                    margin,
                    margin + y_offset,
                    toast.width,
                    toast.height,
                )
            }
            ToastPosition::Center => {
                Rect::new(
                    (ctx.target.width - toast.width) / 2.0,
                    (ctx.target.height - toast.height) / 2.0 + y_offset,
                    toast.width,
                    toast.height,
                )
            }
        };

        let panel_style = PanelStyle {
            bg: toast.style.bg,
            border: toast.style.border,
            border_width: toast.style.border_width,
        };

        render_panel(scene, SceneLayer::Toast, rect, panel_style);

        y_offset += toast.height + spacing;

        // TODO: Toast text rendering
    }
}

/// Information about a single toast.
#[derive(Debug, Clone)]
pub struct ToastInfo {
    pub text: String,
    pub width: f32,
    pub height: f32,
    pub style: ToastStyle,
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
    fn test_toast_position_bottom_right() {
        let pos = ToastPosition::BottomRight;
        let rect = pos.compute_rect(800.0, 600.0, 200.0, 80.0, 16.0);

        assert_eq!(rect.x, 800.0 - 200.0 - 16.0);
        assert_eq!(rect.y, 600.0 - 80.0 - 16.0);
        assert_eq!(rect.w, 200.0);
        assert_eq!(rect.h, 80.0);
    }

    #[test]
    fn test_toast_position_top_left() {
        let pos = ToastPosition::TopLeft;
        let rect = pos.compute_rect(800.0, 600.0, 200.0, 80.0, 16.0);

        assert_eq!(rect.x, 16.0);
        assert_eq!(rect.y, 16.0);
        assert_eq!(rect.w, 200.0);
        assert_eq!(rect.h, 80.0);
    }

    #[test]
    fn test_toast_position_center() {
        let pos = ToastPosition::Center;
        let rect = pos.compute_rect(800.0, 600.0, 200.0, 100.0, 16.0);

        assert_eq!(rect.x, (800.0 - 200.0) / 2.0);
        assert_eq!(rect.y, (600.0 - 100.0) / 2.0);
    }

    #[test]
    fn test_render_toast_single() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let mut scene = Scene::new();
        let style = ToastStyle::default();
        render_toast(&ctx, &mut scene, ToastPosition::BottomRight, 200.0, 80.0, style, 16.0);

        // Should have background + 4 borders = 5 rects
        assert_eq!(scene.toast.len(), 5);

        // Verify all are Rect commands
        for command in &scene.toast {
            match command {
                RenderCommand::Rect(_) => {}
                _ => panic!("Expected Rect command"),
            }
        }
    }

    #[test]
    fn test_render_toast_stack() {
        let snapshot = make_test_snapshot();
        let layout = make_test_layout();
        let target = RenderTarget::new(800.0, 600.0);
        let metrics = CellMetrics::new(10.0, 20.0);
        let ctx = RenderContext::new(&snapshot, &layout, target, metrics);

        let toasts = vec![
            ToastInfo {
                text: "Toast 1".to_string(),
                width: 200.0,
                height: 60.0,
                style: ToastStyle::default(),
            },
            ToastInfo {
                text: "Toast 2".to_string(),
                width: 200.0,
                height: 60.0,
                style: ToastStyle {
                    bg: [0.6, 0.2, 0.2, 0.9],  // Red error toast
                    border: Some([0.8, 0.4, 0.4, 0.9]),
                    border_width: 1.0,
                },
            },
        ];

        let mut scene = Scene::new();
        render_toast_stack(&ctx, &mut scene, &toasts, ToastPosition::BottomRight, 16.0);

        // Should have 2 toasts × (1 bg + 4 borders) = 10 rects
        assert_eq!(scene.toast.len(), 10);
    }

    #[test]
    fn test_toast_style_colors() {
        let style = ToastStyle {
            bg: [0.1, 0.5, 0.9, 0.85],
            border: Some([0.3, 0.7, 1.0, 0.9]),
            border_width: 2.0,
        };

        assert_eq!(style.bg[0], 0.1);
        assert_eq!(style.border.unwrap()[2], 1.0);
        assert_eq!(style.border_width, 2.0);
    }
}
