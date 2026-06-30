/// Reusable panel drawing helper.
/// Panels are rectangular UI containers with background and border.
use crate::{Color, Rect, Scene, SceneLayer};

/// Configuration for a panel's visual appearance.
#[derive(Debug, Clone, Copy)]
pub struct PanelStyle {
    /// Background color
    pub bg: Color,
    /// Border color (if Any)
    pub border: Option<Color>,
    /// Border width in pixels
    pub border_width: f32,
}

impl Default for PanelStyle {
    fn default() -> Self {
        PanelStyle {
            bg: [0.1, 0.1, 0.1, 0.9],
            border: Some([0.5, 0.5, 0.5, 0.8]),
            border_width: 1.0,
        }
    }
}

/// Render a rectangular panel with optional border.
pub fn render_panel(scene: &mut Scene, layer: SceneLayer, rect: Rect, style: PanelStyle) {
    // Background
    scene.rect_to_layer(layer, rect.x, rect.y, rect.w, rect.h, style.bg);

    // Border (simple 1px outline)
    if let Some(border_color) = style.border {
        let bw = style.border_width;

        // Top border
        if bw > 0.0 {
            scene.rect_to_layer(layer, rect.x, rect.y, rect.w, bw, border_color);
            // Bottom border
            scene.rect_to_layer(
                layer,
                rect.x,
                rect.y + rect.h - bw,
                rect.w,
                bw,
                border_color,
            );
            // Left border
            scene.rect_to_layer(layer, rect.x, rect.y, bw, rect.h, border_color);
            // Right border
            scene.rect_to_layer(
                layer,
                rect.x + rect.w - bw,
                rect.y,
                bw,
                rect.h,
                border_color,
            );
        }
    }
}

/// Render a list row (used by menus and select lists).
pub fn render_list_row(
    scene: &mut Scene,
    layer: SceneLayer,
    rect: Rect,
    bg_color: Color,
    selected: bool,
) {
    // Row background
    scene.rect_to_layer(layer, rect.x, rect.y, rect.w, rect.h, bg_color);

    // Selection highlight (left bar)
    if selected {
        let highlight_width = 3.0;
        scene.rect_to_layer(
            layer,
            rect.x,
            rect.y,
            highlight_width,
            rect.h,
            [1.0, 0.8, 0.2, 0.9], // Orange highlight
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenderCommand;

    #[test]
    fn test_render_panel_background() {
        let mut scene = Scene::new();
        let rect = Rect::new(10.0, 20.0, 100.0, 80.0);
        let style = PanelStyle::default();

        render_panel(&mut scene, SceneLayer::Overlay, rect, style);

        // Should have background + 4 borders = 5 rects
        assert_eq!(scene.overlay.len(), 5);

        match &scene.overlay[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 10.0);
                assert_eq!(cmd.rect.y, 20.0);
                assert_eq!(cmd.rect.w, 100.0);
                assert_eq!(cmd.rect.h, 80.0);
                assert_eq!(cmd.color, style.bg);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_render_panel_no_border() {
        let mut scene = Scene::new();
        let rect = Rect::new(0.0, 0.0, 50.0, 50.0);
        let style = PanelStyle {
            border: None,
            ..Default::default()
        };

        render_panel(&mut scene, SceneLayer::Floating, rect, style);

        // Should have only background rect
        assert_eq!(scene.floating.len(), 1);
    }

    #[test]
    fn test_render_list_row_not_selected() {
        let mut scene = Scene::new();
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let bg = [0.2, 0.2, 0.2, 0.8];

        render_list_row(&mut scene, SceneLayer::Overlay, rect, bg, false);

        // Only background, no selection highlight
        assert_eq!(scene.overlay.len(), 1);

        match &scene.overlay[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.color, bg);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_render_list_row_selected() {
        let mut scene = Scene::new();
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let bg = [0.2, 0.2, 0.2, 0.8];

        render_list_row(&mut scene, SceneLayer::Overlay, rect, bg, true);

        // Background + selection highlight
        assert_eq!(scene.overlay.len(), 2);

        // Second rect should be the highlight bar
        match &scene.overlay[1] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 0.0);
                assert_eq!(cmd.rect.w, 3.0); // Highlight width
                assert!(cmd.color[2] < 0.5); // Orange-ish
            }
            _ => panic!("Expected Rect command"),
        }
    }
}
