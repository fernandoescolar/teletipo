use crate::{KeybindingsOverlay, RenderContext, Scene, SceneLayer};

fn clamp_color(mut c: [f32; 4], d: f32) -> [f32; 4] {
    c[0] = (c[0] + d).clamp(0.0, 1.0);
    c[1] = (c[1] + d).clamp(0.0, 1.0);
    c[2] = (c[2] + d).clamp(0.0, 1.0);
    c
}

fn mix_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

fn render_rows(
    scene: &mut Scene,
    overlay: &KeybindingsOverlay,
    layout: &crate::FrameLayout,
    theme: &crate::ColorTheme,
    x0: f32,
    y0: f32,
    x1: f32,
    title_h: f32,
    row_h: f32,
    key_col: f32,
    bind_col: f32,
    row_alt: [f32; 4],
    select: [f32; 4],
    record: [f32; 4],
    bg: [f32; 4],
) {
    let n_rows = overlay.rows.len();
    let visible = overlay.visible_rows.min(n_rows);
    let scroll = overlay.scroll_offset;
    let visible_rows = &overlay.rows[scroll..(scroll + visible).min(n_rows)];

    for (i, row) in visible_rows.iter().enumerate() {
        let flat_idx = scroll + i;
        let row_y = y0 + title_h + i as f32 * row_h;
        let is_cursor = flat_idx == overlay.cursor;
        let row_bg = if is_cursor {
            if overlay.recording { record } else { select }
        } else if i % 2 == 1 {
            row_alt
        } else {
            bg
        };
        scene.rect_to_layer(SceneLayer::Overlay, x0, row_y, x1 - x0, row_h, row_bg);

        let text_y = row_y + (row_h - layout.cell_h_px) * 0.5;
        let [r, g, b, _] = theme.text;
        let label_color = if is_cursor {
            theme.text
        } else {
            [r * 0.85, g * 0.85, b * 0.85, 1.0]
        };
        let binding_color = if is_cursor && overlay.recording {
            [1.0, 0.70, 0.25, 1.0]
        } else if row.binding.is_some() && !row.is_default {
            theme.cursor
        } else if row.is_default {
            [r * 0.65, g * 0.65, b * 0.65, 1.0]
        } else {
            [r * 0.40, g * 0.40, b * 0.40, 1.0]
        };

        let binding_text: std::borrow::Cow<str> = if is_cursor && overlay.recording {
            "\u{25cf} press combo\u{2026}".into()
        } else if let Some(ref b) = row.binding {
            if row.is_default {
                format!("{b}  (default)").into()
            } else {
                b.as_str().into()
            }
        } else {
            "(not bound)".into()
        };

        scene.text_to_layer(
            SceneLayer::Overlay,
            key_col,
            text_y,
            &row.label,
            label_color,
        );
        scene.text_to_layer(
            SceneLayer::Overlay,
            bind_col,
            text_y,
            binding_text.as_ref(),
            binding_color,
        );
    }
}

pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(overlay) = &ctx.snapshot.keybindings_overlay else {
        return;
    };
    let layout = ctx.layout;
    let theme = &ctx.snapshot.theme;

    let n_rows = overlay.rows.len();
    let visible = overlay.visible_rows.min(n_rows);
    let row_h = layout.cell_h_px * 1.7;
    let title_h = layout.cell_h_px * 2.2;
    let footer_h = layout.cell_h_px * 2.0;
    let panel_h = title_h + visible as f32 * row_h + footer_h;
    let panel_w = (layout.cell_w_px * 72.0)
        .min(layout.width * 0.92)
        .max(layout.cell_w_px * 44.0);
    let x0 = (layout.width - panel_w) * 0.5;
    let y0 = (layout.height - panel_h) * 0.5;
    let x1 = x0 + panel_w;
    let y1 = y0 + panel_h;
    let key_col = x0 + layout.cell_w_px * 2.0;
    let bind_col = x0 + panel_w * 0.55;

    let bg = clamp_color(theme.terminal_bg, 0.01);
    let border = theme.separator_focused;
    let title = clamp_color(theme.terminal_bg, -0.01);
    let row_alt = clamp_color(theme.terminal_bg, 0.03);
    let select = mix_color(
        clamp_color(theme.terminal_bg, 0.08),
        theme.separator_focused,
        0.22,
    );
    let record = mix_color(
        clamp_color(theme.terminal_bg, 0.06),
        [0.9, 0.5, 0.1, 1.0],
        0.20,
    );

    scene.rect_to_layer(
        SceneLayer::Overlay,
        0.0,
        0.0,
        layout.width,
        layout.height,
        [0.0, 0.0, 0.0, 0.65],
    );
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x0 - 2.0,
        y0 - 2.0,
        panel_w + 4.0,
        panel_h + 4.0,
        border,
    );
    scene.rect_to_layer(SceneLayer::Overlay, x0, y0, panel_w, panel_h, bg);
    scene.rect_to_layer(SceneLayer::Overlay, x0, y0, panel_w, title_h, title);

    let title_str = if overlay.just_saved {
        "  KEYBINDINGS  \u{2713} Saved"
    } else if overlay.recording {
        "  KEYBINDINGS  \u{25cf} Press key combo..."
    } else {
        "  KEYBINDINGS"
    };
    scene.text_to_layer(
        SceneLayer::Overlay,
        x0,
        y0 + (title_h - layout.cell_h_px) * 0.5,
        title_str,
        theme.text,
    );

    render_rows(
        scene, overlay, layout, theme, x0, y0, x1, title_h, row_h, key_col, bind_col, row_alt,
        select, record, bg,
    );

    if n_rows > visible {
        let scroll = overlay.scroll_offset;
        let sb_x = x1 - layout.cell_w_px * 0.4;
        let sb_w = layout.cell_w_px * 0.25;
        let track_h = visible as f32 * row_h;
        let thumb_h = (track_h * visible as f32 / n_rows as f32).max(row_h * 0.5);
        let thumb_frac = scroll as f32 / (n_rows - visible) as f32;
        let thumb_y = y0 + title_h + thumb_frac * (track_h - thumb_h);
        scene.rect_to_layer(
            SceneLayer::Overlay,
            sb_x,
            y0 + title_h,
            sb_w,
            track_h,
            [0.3, 0.3, 0.3, 0.3],
        );
        scene.rect_to_layer(SceneLayer::Overlay, sb_x, thumb_y, sb_w, thumb_h, border);
    }

    let footer_text = if overlay.recording {
        "  Esc \u{2192} cancel"
    } else {
        "  Enter \u{2192} bind    Backspace \u{2192} remove    Esc \u{2192} close"
    };
    let [r, g, b, _] = theme.text;
    scene.text_to_layer(
        SceneLayer::Overlay,
        x0,
        y1 - footer_h + (footer_h - layout.cell_h_px) * 0.5,
        footer_text,
        [r * 0.55, g * 0.55, b * 0.55, 1.0],
    );
}
