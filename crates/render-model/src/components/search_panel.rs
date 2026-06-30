use crate::{RenderContext, Scene, SceneLayer};

pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(search) = &ctx.snapshot.search_panel else {
        return;
    };

    let layout = ctx.layout;
    let text = format!(
        "Find: {} [{}/{}]{}{}",
        search.query,
        search.current_match,
        search.match_count,
        if search.regex_mode { " R" } else { "" },
        if search.case_sensitive { " C" } else { "" }
    );
    let panel_w = (text.chars().count() as f32 * layout.cell_w_px + layout.cell_w_px * 2.0)
        .min(layout.width * 0.65);
    let panel_h = layout.cell_h_px * 1.6;
    let x = (layout.width - panel_w - layout.padding_h).max(0.0);
    let y = layout.tab_bar_h + layout.padding_v;

    scene.rect_to_layer(
        SceneLayer::Overlay,
        x - 1.0,
        y - 1.0,
        panel_w + 2.0,
        panel_h + 2.0,
        [0.30, 0.45, 0.70, 0.95],
    );
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x,
        y,
        panel_w,
        panel_h,
        [0.09, 0.11, 0.18, 0.96],
    );
    scene.text_to_layer(
        SceneLayer::Overlay,
        x + layout.cell_w_px * 0.6,
        y + (panel_h - layout.cell_h_px) * 0.5,
        text,
        [0.92, 0.94, 0.98, 1.0],
    );

    if let Some(err) = &search.error {
        let ey = y + panel_h + 2.0;
        let ew = (err.chars().count() as f32 * layout.cell_w_px + layout.cell_w_px)
            .min(layout.width * 0.70);
        scene.rect_to_layer(
            SceneLayer::Overlay,
            x,
            ey,
            ew,
            layout.cell_h_px * 1.3,
            [0.22, 0.08, 0.08, 0.96],
        );
        let err_text: String = err.chars().take(48).collect();
        scene.text_to_layer(
            SceneLayer::Overlay,
            x + 4.0,
            ey + 2.0,
            err_text,
            [1.0, 0.9, 0.9, 1.0],
        );
    }
}
