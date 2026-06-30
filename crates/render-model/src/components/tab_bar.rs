/// Tab bar: multi-document interface tabs.
///
/// Renders:
/// - Background
/// - Tab rectangles (active vs inactive colors)
/// - Tab labels and close buttons as text
/// - Add button (+)
/// - Drag insertion indicator line
use crate::{RenderContext, Scene, SceneLayer};

/// Helper: lighten/darken RGB by delta, keeping alpha.
fn clamp_color(mut c: [f32; 4], d: f32) -> [f32; 4] {
    c[0] = (c[0] + d).clamp(0.0, 1.0);
    c[1] = (c[1] + d).clamp(0.0, 1.0);
    c[2] = (c[2] + d).clamp(0.0, 1.0);
    c
}

/// Helper: linear interpolation between two colors.
fn mix_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// Helper: apply window opacity to color.
fn frosted_backdrop_alpha(opacity: f32) -> f32 {
    let opacity = opacity.clamp(0.0, 1.0);
    0.55 + 0.45 * opacity
}

fn with_backdrop_alpha(mut color: [f32; 4], opacity: f32) -> [f32; 4] {
    color[3] = (color[3] * frosted_backdrop_alpha(opacity)).clamp(0.0, 1.0);
    color
}

/// Render tab bar with tabs, labels, and controls.
/// Called from GlPainter to emit tab bar geometry and text into the Scene.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    if ctx.snapshot.tab_labels.is_empty() || ctx.layout.tab_bar_h <= 0.0 {
        return;
    }

    let layout = ctx.layout;
    let snapshot = ctx.snapshot;

    // Color palette
    let apply_opacity = |c| with_backdrop_alpha(c, snapshot.opacity);
    let tab_bar_bg = clamp_color(snapshot.theme.terminal_bg, 0.05);
    let tab_inactive = clamp_color(snapshot.theme.terminal_bg, 0.02);
    let tab_active = mix_color(tab_bar_bg, snapshot.theme.separator_focused, 0.22);
    let add_btn_bg = [
        (snapshot.theme.terminal_bg[0] + 0.05).clamp(0.0, 1.0),
        (snapshot.theme.terminal_bg[1] + 0.10).clamp(0.0, 1.0),
        (snapshot.theme.terminal_bg[2] + 0.03).clamp(0.0, 1.0),
        0.90,
    ];
    let text_color = snapshot.theme.text;

    // === Tab bar background ===
    scene.rect_to_layer(
        SceneLayer::Main,
        0.0,
        0.0,
        layout.width,
        layout.tab_bar_h,
        apply_opacity(tab_bar_bg),
    );

    // === Layout calculations ===
    let n = snapshot.tab_labels.len().max(1);
    let add_w = layout.cell_w_px * 2.0;
    let tab_area_w = (layout.width - add_w).max(layout.cell_w_px * 2.0);
    let tab_w = (tab_area_w / n as f32).max(layout.cell_w_px * 3.0);
    let gap = 1.0;

    // === Render each tab ===
    for (i, label) in snapshot.tab_labels.iter().enumerate() {
        let x0 = i as f32 * tab_w + gap;
        let x1 = ((i + 1) as f32 * tab_w - gap).min(tab_area_w - gap);
        let y0 = 1.0;
        let y1 = (layout.tab_bar_h - 1.0).max(y0 + 1.0);

        // Tab background (color depends on active state)
        let tab_color = if i == snapshot.active_tab {
            tab_active
        } else {
            tab_inactive
        };
        scene.rect_to_layer(
            SceneLayer::Main,
            x0,
            y0,
            x1 - x0,
            y1 - y0,
            apply_opacity(tab_color),
        );

        // Tab label text
        let text_x = x0 + layout.cell_w_px * 0.5;
        let text_y = (layout.tab_bar_h - layout.cell_h_px).max(0.0) * 0.5;

        // Truncate label to 18 chars max
        let label_truncated: String = label.chars().take(18).collect();
        scene.text_to_layer(
            SceneLayer::Main,
            text_x,
            text_y,
            label_truncated,
            text_color,
        );

        // Close button '×' at the right edge
        let close_x = x1 - layout.cell_w_px * 1.4;
        let close_color = [text_color[0], text_color[1], text_color[2], 0.65];
        scene.text_to_layer(SceneLayer::Main, close_x, text_y, "×", close_color);
    }

    // === Add button ===
    let add_x0 = tab_area_w + gap;
    let add_x1 = (layout.width - gap).max(add_x0 + 1.0);
    scene.rect_to_layer(
        SceneLayer::Main,
        add_x0,
        1.0,
        add_x1 - add_x0,
        (layout.tab_bar_h - 1.0).max(2.0),
        apply_opacity(add_btn_bg),
    );

    // Add button text '+'
    let add_text_x = add_x0 + (add_w - layout.cell_w_px) * 0.5;
    let add_text_y = (layout.tab_bar_h - layout.cell_h_px).max(0.0) * 0.5;
    scene.text_to_layer(SceneLayer::Main, add_text_x, add_text_y, "+", text_color);

    // === Drag insertion indicator ===
    if let Some(insert_before) = snapshot.tab_drag_insert_before {
        let ib = insert_before.min(n);
        let x = (ib as f32 * tab_w).clamp(0.0, tab_area_w);
        scene.rect_to_layer(
            SceneLayer::Main,
            (x - 1.0).max(0.0),
            0.0,
            2.0,
            layout.tab_bar_h,
            [0.8, 0.8, 0.8, 0.90],
        );
    }
}
