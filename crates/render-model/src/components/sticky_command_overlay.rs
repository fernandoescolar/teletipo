use crate::{RenderContext, Scene, SceneLayer};

fn with_alpha(mut c: [f32; 4], alpha: f32) -> [f32; 4] {
    c[3] = alpha.clamp(0.0, 1.0);
    c
}

pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(overlay) = &ctx.snapshot.sticky_command_overlay else {
        return;
    };
    if overlay.text.is_empty() {
        return;
    }

    let layout = ctx.layout;
    let theme = &ctx.snapshot.theme;
    let x = 0.0;
    let y = layout.tab_bar_h;
    let w = layout.width;
    let h = layout.cell_h_px * 1.4;

    scene.rect_to_layer(
        SceneLayer::Overlay,
        x,
        y,
        w,
        h,
        with_alpha(theme.terminal_bg, 0.94),
    );
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x,
        y + h - 1.0,
        w,
        1.0,
        with_alpha(theme.separator_focused, 0.98),
    );
    scene.text_to_layer(
        SceneLayer::Overlay,
        x + layout.cell_w_px * 0.8,
        y + (h - layout.cell_h_px) * 0.5,
        overlay.text.clone(),
        with_alpha(theme.text, 1.0),
    );
}
