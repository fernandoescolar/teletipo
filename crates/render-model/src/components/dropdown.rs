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

    let layout = ctx.layout;
    let dropdown_w = 25.0 * layout.cell_w_px;
    let max_visible = 8;
    let dropdown_h = (dropdown.items.len().min(max_visible) as f32 * layout.cell_h_px * 1.1)
        .max(layout.cell_h_px * 2.0);
    let x = layout.padding_h;
    let y = layout.editor_top + layout.cell_h_px * 2.0;

    // Dropdown background
    scene.rect_to_layer(
        SceneLayer::Floating,
        x,
        y,
        dropdown_w,
        dropdown_h,
        [0.16, 0.17, 0.22, 0.94],
    );

    // Dropdown border
    let border_color = [0.40, 0.50, 0.65, 0.90];
    scene.rect_to_layer(
        SceneLayer::Floating,
        x - 1.0,
        y - 1.0,
        dropdown_w + 2.0,
        1.0,
        border_color,
    );
    scene.rect_to_layer(
        SceneLayer::Floating,
        x - 1.0,
        y + dropdown_h,
        dropdown_w + 2.0,
        1.0,
        border_color,
    );
    scene.rect_to_layer(
        SceneLayer::Floating,
        x - 1.0,
        y,
        1.0,
        dropdown_h,
        border_color,
    );
    scene.rect_to_layer(
        SceneLayer::Floating,
        x + dropdown_w,
        y,
        1.0,
        dropdown_h,
        border_color,
    );

    // Dropdown items
    for (i, item) in dropdown.items.iter().take(max_visible).enumerate() {
        let item_y = y + i as f32 * layout.cell_h_px * 1.1;
        let item_color = if i == dropdown.selected {
            [0.25, 0.65, 0.95, 1.0]
        } else {
            [0.75, 0.78, 0.82, 0.9]
        };
        scene.text_to_layer(
            SceneLayer::Floating,
            x + layout.cell_w_px * 0.3,
            item_y,
            item,
            item_color,
        );
    }
}
