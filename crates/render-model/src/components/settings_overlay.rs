use crate::{RenderContext, Scene, SceneLayer, SettingsOverlay};

const SEARCH_MAX_VISIBLE: usize = 8;

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

fn with_alpha(mut c: [f32; 4], alpha: f32) -> [f32; 4] {
    c[3] = alpha.clamp(0.0, 1.0);
    c
}

fn settings_item_display_val<'a>(
    item: &'a crate::SettingsItem,
    overlay: &'a SettingsOverlay,
    is_focused: bool,
) -> std::borrow::Cow<'a, str> {
    if item.is_searchable && is_focused && overlay.search_buf.is_some() {
        let sbuf = overlay.search_buf.as_deref().unwrap_or("");
        return format!("/ {sbuf}\u{258e}").into();
    }
    if item.is_searchable {
        return format!("{} /", item.value).into();
    }
    if item.is_selectable && !item.is_action && !(is_focused && overlay.editing.is_some()) {
        return format!("\u{2190} {} \u{2192}", item.value).into();
    }
    if !item.is_selectable && is_focused && overlay.editing.is_none() {
        return format!("{}\u{258e}", item.value).into();
    }
    if is_focused && let Some(ref buf) = overlay.editing {
        return buf.as_str().into();
    }
    item.value.as_str().into()
}

#[allow(clippy::too_many_lines)]
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(overlay) = &ctx.snapshot.settings_overlay else {
        return;
    };
    let layout = ctx.layout;
    let theme = &ctx.snapshot.theme;

    scene.rect_to_layer(
        SceneLayer::Overlay,
        0.0,
        0.0,
        layout.width,
        layout.height,
        [0.0, 0.0, 0.0, 0.65],
    );

    let title_h = layout.cell_h_px * 2.2;
    let row_h = layout.cell_h_px * 1.7;
    let footer_h = layout.cell_h_px * 1.9;
    let edit_h = if overlay.editing.is_some() {
        layout.cell_h_px * 1.8
    } else {
        0.0
    };
    let panel_h = title_h + overlay.items.len() as f32 * row_h + edit_h + footer_h;
    let panel_w = (layout.cell_w_px * 72.0)
        .min(layout.width * 0.92)
        .max(layout.cell_w_px * 40.0);
    let x0 = (layout.width - panel_w) * 0.5;
    let y0 = (layout.height - panel_h) * 0.5;
    let key_col = x0 + layout.cell_w_px * 1.5;
    let val_col = x0 + panel_w * 0.50;

    let bg = with_alpha(clamp_color(theme.terminal_bg, 0.01), 0.92);
    let border = with_alpha(theme.separator_focused, 0.96);
    let title = with_alpha(clamp_color(theme.terminal_bg, -0.01), 0.94);
    let section = with_alpha(clamp_color(theme.terminal_bg, 0.04), 0.90);
    let select = with_alpha(
        mix_color(
            clamp_color(theme.terminal_bg, 0.08),
            theme.separator_focused,
            0.20,
        ),
        0.93,
    );
    let edit = with_alpha(
        mix_color(
            clamp_color(theme.terminal_bg, 0.08),
            theme.separator_focused,
            0.28,
        ),
        0.93,
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

    let mut editable_idx = 0usize;
    for (i, item) in overlay.items.iter().enumerate() {
        let ry = y0 + title_h + i as f32 * row_h;
        if item.is_header {
            scene.rect_to_layer(SceneLayer::Overlay, x0, ry, panel_w, row_h, section);
        } else {
            if editable_idx == overlay.cursor {
                let row_color = if overlay.editing.is_some() {
                    edit
                } else {
                    select
                };
                scene.rect_to_layer(SceneLayer::Overlay, x0, ry, panel_w, row_h, row_color);
            }
            editable_idx += 1;
        }
    }
    if overlay.editing.is_some() {
        let ey = y0 + title_h + overlay.items.len() as f32 * row_h;
        scene.rect_to_layer(SceneLayer::Overlay, x0, ey, panel_w, edit_h, edit);
    }

    let mut focused_flat = 0usize;
    let mut flat_editable = 0usize;
    for (i, item) in overlay.items.iter().enumerate() {
        if !item.is_header {
            if flat_editable == overlay.cursor {
                focused_flat = i;
                break;
            }
            flat_editable = flat_editable.saturating_add(1);
        }
    }

    if overlay.search_buf.is_some() {
        let visible = overlay
            .search_matches
            .len()
            .saturating_sub(overlay.search_scroll_offset)
            .clamp(1, SEARCH_MAX_VISIBLE);
        let dy = y0 + title_h + (focused_flat + 1) as f32 * row_h;
        let dh = row_h * visible as f32;
        scene.rect_to_layer(
            SceneLayer::Overlay,
            x0 - 1.0,
            dy - 1.0,
            panel_w + 2.0,
            dh + 2.0,
            [0.35, 0.50, 0.82, 1.0],
        );
        scene.rect_to_layer(
            SceneLayer::Overlay,
            x0,
            dy,
            panel_w,
            dh,
            [0.15, 0.19, 0.30, 1.0],
        );
        let vis_sel = overlay
            .search_selected
            .saturating_sub(overlay.search_scroll_offset);
        if !overlay.search_matches.is_empty() && vis_sel < visible {
            let sy0 = dy + vis_sel as f32 * row_h;
            scene.rect_to_layer(
                SceneLayer::Overlay,
                x0,
                sy0,
                panel_w,
                row_h,
                [0.22, 0.34, 0.62, 1.0],
            );
        }
    }

    let title_text = if overlay.just_saved {
        "  SETTINGS  \u{2713} Saved"
    } else {
        "  SETTINGS  (Cmd+,)"
    };
    scene.text_to_layer(
        SceneLayer::Overlay,
        x0,
        y0 + (title_h - layout.cell_h_px) * 0.5,
        title_text,
        theme.text,
    );

    let search_cover_end = if overlay.search_buf.is_some() {
        let n_vis = overlay
            .search_matches
            .len()
            .saturating_sub(overlay.search_scroll_offset)
            .min(SEARCH_MAX_VISIBLE);
        focused_flat + n_vis
    } else {
        0
    };

    let mut focused_flat_idx = 0usize;
    editable_idx = 0;
    for (i, item) in overlay.items.iter().enumerate() {
        if overlay.search_buf.is_some() && i > focused_flat && i <= search_cover_end {
            continue;
        }

        let row_y = y0 + title_h + i as f32 * row_h + (row_h - layout.cell_h_px) * 0.5;
        if item.is_header {
            scene.text_to_layer(
                SceneLayer::Overlay,
                key_col,
                row_y,
                &item.key,
                theme.separator_focused,
            );
            continue;
        }

        let is_focused = editable_idx == overlay.cursor;
        if is_focused {
            focused_flat_idx = i;
        }
        editable_idx += 1;

        let [r, g, b, _] = theme.text;
        let [cr, cg, cb, _] = theme.cursor;
        let (key_color, val_color) = if is_focused {
            (theme.text, theme.cursor)
        } else {
            (
                [r * 0.85, g * 0.85, b * 0.85, 1.0],
                [cr * 0.75, cg * 0.85, cb * 0.85, 0.85],
            )
        };
        let display_val = settings_item_display_val(item, overlay, is_focused);

        scene.text_to_layer(SceneLayer::Overlay, key_col, row_y, &item.key, key_color);
        scene.text_to_layer(
            SceneLayer::Overlay,
            val_col,
            row_y,
            display_val.as_ref(),
            val_color,
        );
    }

    if overlay.search_buf.is_none() {
        let footer_y = y0
            + title_h
            + overlay.items.len() as f32 * row_h
            + edit_h
            + (footer_h - layout.cell_h_px) * 0.5;
        let footer_text = if overlay.editing.is_some() {
            "  Enter: confirm   Esc: cancel"
        } else {
            "  ↑↓ navigate   ←→ change   Enter: edit/search   Esc: close & save"
        };
        let [r, g, b, _] = theme.text;
        scene.text_to_layer(
            SceneLayer::Overlay,
            x0,
            footer_y,
            footer_text,
            [r * 0.55, g * 0.55, b * 0.55, 0.90],
        );
        return;
    }

    let n_visible = overlay
        .search_matches
        .len()
        .saturating_sub(overlay.search_scroll_offset)
        .min(SEARCH_MAX_VISIBLE);
    let visible_end = overlay.search_scroll_offset + n_visible;
    let vis_sel = overlay
        .search_selected
        .saturating_sub(overlay.search_scroll_offset);
    let drop_top_px = y0 + title_h + (focused_flat_idx + 1) as f32 * row_h;

    if overlay.search_matches.is_empty() {
        let [r, g, b, _] = theme.text;
        scene.text_to_layer(
            SceneLayer::Overlay,
            key_col,
            drop_top_px + (row_h - layout.cell_h_px) * 0.5,
            "(no results)",
            [r * 0.45, g * 0.45, b * 0.45, 0.70],
        );
        return;
    }

    for (i, match_str) in overlay.search_matches[overlay.search_scroll_offset..visible_end]
        .iter()
        .enumerate()
    {
        let row_y = drop_top_px + i as f32 * row_h + (row_h - layout.cell_h_px) * 0.5;
        let is_sel = i == vis_sel;
        let [r, g, b, _] = theme.text;
        let color = if is_sel {
            theme.text
        } else {
            [r * 0.60, g * 0.60, b * 0.60, 1.0]
        };
        let label = if is_sel {
            format!("\u{25b6} {match_str}")
        } else {
            format!("  {match_str}")
        };
        scene.text_to_layer(SceneLayer::Overlay, key_col, row_y, label, color);
    }
}
