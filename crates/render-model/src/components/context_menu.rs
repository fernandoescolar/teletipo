use crate::{RenderContext, Scene, SceneLayer};

pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(menu) = &ctx.snapshot.context_menu else {
        return;
    };
    let layout = ctx.layout;
    let menu_w = 20.0 * layout.cell_w_px;
    let menu_h = menu.items.len() as f32 * layout.cell_h_px * 1.2;
    let x = (menu.x_px).min(layout.width - menu_w);
    let y = (menu.y_px).min(layout.height - menu_h);

    scene.rect_to_layer(
        SceneLayer::Floating,
        x,
        y,
        menu_w,
        menu_h,
        [0.18, 0.18, 0.22, 0.96],
    );
    let border_color = [0.45, 0.50, 0.55, 0.90];
    scene.rect_to_layer(
        SceneLayer::Floating,
        x - 1.0,
        y - 1.0,
        menu_w + 2.0,
        1.0,
        border_color,
    );
    scene.rect_to_layer(
        SceneLayer::Floating,
        x - 1.0,
        y + menu_h,
        menu_w + 2.0,
        1.0,
        border_color,
    );
    scene.rect_to_layer(SceneLayer::Floating, x - 1.0, y, 1.0, menu_h, border_color);
    scene.rect_to_layer(
        SceneLayer::Floating,
        x + menu_w,
        y,
        1.0,
        menu_h,
        border_color,
    );

    for (i, item) in menu.items.iter().enumerate() {
        let item_y = y + i as f32 * layout.cell_h_px * 1.2;
        let item_color = [0.8, 0.8, 0.85, 0.9];
        scene.text_to_layer(
            SceneLayer::Floating,
            x + layout.cell_w_px * 0.5,
            item_y,
            item,
            item_color,
        );
    }
}
