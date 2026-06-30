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
    let Some(search) = &ctx.snapshot.search_panel else {
        return;
    };

    let layout = ctx.layout;
    let theme = &ctx.snapshot.theme;
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
    let border = with_alpha(theme.separator_focused, 0.95);
    let bg = with_alpha(theme.terminal_bg, 0.96);
    let fg = with_alpha(theme.text, 1.0);
    let err_bg = with_alpha(
        [
            theme.ansi_palette[1][0],
            theme.ansi_palette[1][1],
            theme.ansi_palette[1][2],
            1.0,
        ],
        0.30,
    );
    let err_fg = with_alpha(mix(theme.text, theme.cursor, 0.15), 1.0);

    scene.rect_to_layer(
        SceneLayer::Overlay,
        x - 1.0,
        y - 1.0,
        panel_w + 2.0,
        panel_h + 2.0,
        border,
    );
    scene.rect_to_layer(SceneLayer::Overlay, x, y, panel_w, panel_h, bg);
    scene.text_to_layer(
        SceneLayer::Overlay,
        x + layout.cell_w_px * 0.6,
        y + (panel_h - layout.cell_h_px) * 0.5,
        text,
        fg,
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
            err_bg,
        );
        let err_text: String = err.chars().take(48).collect();
        scene.text_to_layer(SceneLayer::Overlay, x + 4.0, ey + 2.0, err_text, err_fg);
    }
}
