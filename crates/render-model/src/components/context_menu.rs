use crate::{RenderContext, Scene, SceneLayer};

pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(menu) = &ctx.snapshot.context_menu else {
        return;
    };
    if menu.items.is_empty() {
        return;
    }

    let layout = ctx.layout;
    let max_chars = menu
        .items
        .iter()
        .map(|s| s.chars().count())
        .max()
        .unwrap_or(8) as f32;
    let row_h = layout.cell_h_px * 1.4;
    let menu_w = (max_chars * layout.cell_w_px + layout.cell_w_px * 2.0).min(layout.width * 0.5);
    let menu_h = row_h * menu.items.len() as f32;
    let x = menu.x_px.clamp(0.0, (layout.width - menu_w).max(0.0));
    let y = menu.y_px.clamp(0.0, (layout.height - menu_h).max(0.0));

    scene.rect_to_layer(
        SceneLayer::Floating,
        x - 1.0,
        y - 1.0,
        menu_w + 2.0,
        menu_h + 2.0,
        [0.35, 0.55, 0.90, 1.0],
    );
    scene.rect_to_layer(
        SceneLayer::Floating,
        x,
        y,
        menu_w,
        menu_h,
        [0.09, 0.11, 0.18, 1.0],
    );

    for (i, item) in menu.items.iter().enumerate() {
        let item_y = y + i as f32 * row_h;
        if Some(i) == menu.hovered_item {
            scene.rect_to_layer(
                SceneLayer::Floating,
                x,
                item_y,
                menu_w,
                row_h,
                [0.20, 0.32, 0.58, 1.0],
            );
        }
        let item_text: String = item.chars().take(36).collect();
        scene.text_to_layer(
            SceneLayer::Floating,
            x + 6.0,
            item_y + 2.0,
            item_text,
            [0.92, 0.94, 0.98, 1.0],
        );
    }
}
