use crate::{RenderContext, Scene, SceneLayer};

pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(cmd_pal) = &ctx.snapshot.command_palette else { return; };
    let layout = ctx.layout;
    let panel_w = (layout.width * 0.7).max(40.0 * layout.cell_w_px);
    let panel_h = (layout.height * 0.6).max(10.0 * layout.cell_h_px);
    let x = (layout.width - panel_w) * 0.5;
    let y = (layout.height - panel_h) * 0.3;

    scene.rect_to_layer(SceneLayer::Overlay, x, y, panel_w, panel_h, [0.12, 0.14, 0.18, 0.97]);
    let border_color = [0.4, 0.5, 0.6, 0.95];
    scene.rect_to_layer(SceneLayer::Overlay, x - 1.0, y - 1.0, panel_w + 2.0, 1.0, border_color);
    scene.rect_to_layer(SceneLayer::Overlay, x - 1.0, y + panel_h, panel_w + 2.0, 1.0, border_color);
    scene.rect_to_layer(SceneLayer::Overlay, x - 1.0, y, 1.0, panel_h, border_color);
    scene.rect_to_layer(SceneLayer::Overlay, x + panel_w, y, 1.0, panel_h, border_color);

    let input_y = y + layout.cell_h_px * 0.5;
    scene.text_to_layer(SceneLayer::Overlay, x + layout.cell_w_px * 0.5, input_y, "> ", [0.7, 0.8, 0.9, 1.0]);
    scene.text_to_layer(SceneLayer::Overlay, x + layout.cell_w_px * 2.5, input_y, &cmd_pal.query, [0.95, 0.95, 1.0, 1.0]);

    let list_y = input_y + layout.cell_h_px * 1.5;
    let cmd_count = cmd_pal.items.len();
    scene.text_to_layer(SceneLayer::Overlay, x + layout.cell_w_px * 0.5, list_y, &format!("{} commands", cmd_count), [0.7, 0.7, 0.8, 0.85]);
}
