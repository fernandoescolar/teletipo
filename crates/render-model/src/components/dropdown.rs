/// Suggestion dropdown: list of auto-complete suggestions.
///
/// Renders:
/// - Dropdown background and border
/// - Suggestion items
/// - Selection highlight
/// - Scrollbar if needed
use crate::{RenderContext, Scene, SceneLayer};

/// Render suggestion dropdown.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(dropdown) = &ctx.snapshot.suggestion_dropdown else {
        return;
    };
    if dropdown.items.is_empty() {
        return;
    }

    let snapshot = ctx.snapshot;
    let layout = ctx.layout;
    let max_visible = 8usize;
    let start = dropdown.scroll_offset.min(dropdown.items.len());
    let visible = dropdown.items.len().saturating_sub(start).min(max_visible);
    if visible == 0 {
        return;
    }

    let row_h = layout.cell_h_px * 1.2;
    let panel_w = (layout.cell_w_px * 40.0).min(layout.width * 0.75);
    let panel_h = visible as f32 * row_h;
    let x0 = layout.padding_h;
    let x1 = x0 + panel_w;
    let y1 = layout.editor_top;
    let y0 = (y1 - panel_h).max(layout.tab_bar_h);

    scene.rect_to_layer(
        SceneLayer::Floating,
        x0 - 1.0,
        y0 - 1.0,
        panel_w + 2.0,
        panel_h + 2.0,
        [0.30, 0.45, 0.70, 0.95],
    );
    scene.rect_to_layer(
        SceneLayer::Floating,
        x0,
        y0,
        panel_w,
        panel_h,
        [0.09, 0.11, 0.18, 0.97],
    );

    for i in 0..visible {
        let idx = start + i;
        let row_y = y0 + i as f32 * row_h;
        if idx == dropdown.selected {
            scene.rect_to_layer(
                SceneLayer::Floating,
                x0,
                row_y,
                panel_w,
                row_h,
                [0.20, 0.32, 0.58, 0.70],
            );
        }
        let fg = if idx == dropdown.selected {
            [0.92, 0.94, 0.98, 1.0]
        } else {
            let [r, g, b, _] = snapshot.theme.text;
            [r * 0.72, g * 0.72, b * 0.72, 0.9]
        };
        let item_text: String = dropdown.items[idx].chars().take(36).collect();
        scene.text_to_layer(
            SceneLayer::Floating,
            x0 + layout.cell_w_px * 0.6,
            row_y + (row_h - layout.cell_h_px) * 0.5,
            item_text,
            fg,
        );
    }

    let total = dropdown.items.len();
    if total > visible {
        let sb_w = (layout.cell_w_px * 0.35).max(3.0);
        let sb_x0 = x1 - sb_w;
        let sb_x1 = x1;
        scene.rect_to_layer(
            SceneLayer::Floating,
            sb_x0,
            y0,
            sb_x1 - sb_x0,
            y1 - y0,
            [0.17, 0.19, 0.26, 0.97],
        );
        let thumb_frac = visible as f32 / total as f32;
        let thumb_h = panel_h * thumb_frac;
        let max_scroll = (total - visible) as f32;
        let scroll_frac = dropdown.scroll_offset as f32 / max_scroll;
        let thumb_top = y0 + scroll_frac * (panel_h - thumb_h);
        scene.rect_to_layer(
            SceneLayer::Floating,
            sb_x0,
            thumb_top,
            sb_x1 - sb_x0,
            thumb_h,
            [0.30, 0.45, 0.70, 0.95],
        );
    }
}
