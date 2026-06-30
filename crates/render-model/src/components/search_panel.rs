use crate::{RenderContext, Scene, SceneLayer};

pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(search) = &ctx.snapshot.search_panel else {
        return;
    };
    let layout = ctx.layout;
    let panel_w = (layout.width * 0.5).min(60.0 * layout.cell_w_px);
    let panel_h = layout.cell_h_px * 3.5;
    let x = (layout.width - panel_w) * 0.5;
    let y = (layout.height - panel_h) * 0.5;

    scene.rect_to_layer(
        SceneLayer::Overlay,
        x,
        y,
        panel_w,
        panel_h,
        [0.15, 0.15, 0.20, 0.95],
    );
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x - 1.0,
        y - 1.0,
        panel_w + 2.0,
        1.0,
        [0.5, 0.5, 0.6, 0.9],
    );
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x - 1.0,
        y + panel_h,
        panel_w + 2.0,
        1.0,
        [0.5, 0.5, 0.6, 0.9],
    );
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x - 1.0,
        y,
        1.0,
        panel_h,
        [0.5, 0.5, 0.6, 0.9],
    );
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x + panel_w,
        y,
        1.0,
        panel_h,
        [0.5, 0.5, 0.6, 0.9],
    );

    let label_x = x + layout.cell_w_px * 0.5;
    let label_y = y + layout.cell_h_px * 0.5;
    scene.text_to_layer(
        SceneLayer::Overlay,
        label_x,
        label_y,
        "Find:",
        ctx.snapshot.theme.text,
    );
    scene.text_to_layer(
        SceneLayer::Overlay,
        label_x + layout.cell_w_px * 8.0,
        label_y,
        &search.query,
        [0.9, 0.9, 0.95, 1.0],
    );

    let counter_y = label_y + layout.cell_h_px * 1.2;
    let counter_text = format!("{} matches", search.match_count);
    scene.text_to_layer(
        SceneLayer::Overlay,
        label_x,
        counter_y,
        counter_text,
        [0.7, 0.7, 0.8, 0.9],
    );
}
