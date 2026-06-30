use crate::{RenderContext, Scene, SceneLayer};

fn with_alpha(mut c: [f32; 4], alpha: f32) -> [f32; 4] {
    c[3] = alpha.clamp(0.0, 1.0);
    c
}

fn mix(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(menu) = &ctx.snapshot.context_menu else {
        return;
    };
    if menu.items.is_empty() {
        return;
    }

    let layout = ctx.layout;
    let theme = &ctx.snapshot.theme;
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

    let border = with_alpha(theme.separator_focused, 0.95);
    let bg = with_alpha(theme.terminal_bg, 0.96);
    let hover = with_alpha(mix(theme.cursor, theme.separator_focused, 0.35), 0.88);
    let text = with_alpha(theme.text, 1.0);

    scene.rect_to_layer(
        SceneLayer::Floating,
        x - 1.0,
        y - 1.0,
        menu_w + 2.0,
        menu_h + 2.0,
        border,
    );
    scene.rect_to_layer(SceneLayer::Floating, x, y, menu_w, menu_h, bg);

    for (i, item) in menu.items.iter().enumerate() {
        let item_y = y + i as f32 * row_h;
        if Some(i) == menu.hovered_item {
            scene.rect_to_layer(SceneLayer::Floating, x, item_y, menu_w, row_h, hover);
        }
        let item_text: String = item.chars().take(36).collect();
        scene.text_to_layer(SceneLayer::Floating, x + 6.0, item_y + 2.0, item_text, text);
    }
}
