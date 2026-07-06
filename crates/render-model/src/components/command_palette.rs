use crate::{RenderContext, Scene, SceneLayer};

const PALETTE_MAX_VISIBLE: usize = 10;
const PALETTE_MAX_CHARS: usize = 48;

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        text.to_string()
    } else {
        let mut result: String = text.chars().take(max_chars.saturating_sub(1)).collect();
        result.push('…');
        result
    }
}

fn with_alpha(mut c: [f32; 4], alpha: f32) -> [f32; 4] {
    c[3] = alpha.clamp(0.0, 1.0);
    c
}

fn clamp_color(mut c: [f32; 4], d: f32) -> [f32; 4] {
    c[0] = (c[0] + d).clamp(0.0, 1.0);
    c[1] = (c[1] + d).clamp(0.0, 1.0);
    c[2] = (c[2] + d).clamp(0.0, 1.0);
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

#[allow(clippy::too_many_lines)]
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some(cp) = &ctx.snapshot.command_palette else {
        return;
    };
    let layout = ctx.layout;
    let theme = &ctx.snapshot.theme;
    let border = with_alpha(theme.separator_focused, 0.96);
    let bg = with_alpha(clamp_color(theme.terminal_bg, 0.01), 0.92);
    let header_bg = with_alpha(clamp_color(theme.terminal_bg, -0.01), 0.94);
    let divider = with_alpha(theme.separator_focused, 0.30);
    let label_color = with_alpha(mix(theme.text, theme.cursor, 0.2), 0.95);
    let selected = with_alpha(
        mix(
            clamp_color(theme.terminal_bg, 0.08),
            theme.separator_focused,
            0.20,
        ),
        0.93,
    );
    let text = with_alpha(theme.text, 1.0);
    let dim_text = with_alpha(theme.text, 0.85);
    let visible = cp
        .items
        .len()
        .saturating_sub(cp.scroll_offset)
        .min(PALETTE_MAX_VISIBLE);
    let palette_w = layout.cell_w_px * 50.0;
    let header_h = layout.cell_h_px * 2.2;
    let item_h = layout.cell_h_px * 1.4;
    let cx = layout.width * 0.5;
    let x0 = (cx - palette_w * 0.5).max(0.0);
    let x1 = (cx + palette_w * 0.5).min(layout.width);
    let y0 = layout.tab_bar_h + layout.height * 0.08;

    if let Some(label) = &cp.sub_prompt_label {
        // Split label into lines and calculate dynamic height
        let label_lines: Vec<&str> = label.lines().collect();
        let line_h = layout.cell_h_px * 1.4;
        let label_h = line_h * (label_lines.len() as f32).max(1.0);
        let input_h = layout.cell_h_px * 1.8;
        let total_h = label_h + input_h;
        let y1 = (y0 + total_h).min(layout.height);

        scene.rect_to_layer(
            SceneLayer::Overlay,
            x0 - 2.0,
            y0 - 2.0,
            (x1 + 2.0) - (x0 - 2.0),
            (y1 + 2.0) - (y0 - 2.0),
            border,
        );
        scene.rect_to_layer(SceneLayer::Overlay, x0, y0, x1 - x0, y1 - y0, bg);
        // Header background (darker, like settings title bar)
        scene.rect_to_layer(SceneLayer::Overlay, x0, y0, x1 - x0, label_h, header_bg);
        scene.rect_to_layer(
            SceneLayer::Overlay,
            x0,
            y0 + label_h - 1.0,
            x1 - x0,
            1.0,
            divider,
        );

        // Render each line of the label
        let mut y_offset = y0;
        for line in label_lines {
            scene.text_to_layer(
                SceneLayer::Overlay,
                x0 + layout.cell_w_px * 0.8,
                y_offset + (line_h - layout.cell_h_px) * 0.5,
                truncate_chars(line, PALETTE_MAX_CHARS),
                label_color,
            );
            y_offset += line_h;
        }

        scene.text_to_layer(
            SceneLayer::Overlay,
            x0 + layout.cell_w_px * 0.8,
            y0 + label_h + (input_h - layout.cell_h_px) * 0.5,
            format!("> {}", cp.query),
            text,
        );
        return;
    }

    let palette_h = header_h + item_h * visible as f32;
    let y1 = (y0 + palette_h).min(layout.height);
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x0 - 2.0,
        y0 - 2.0,
        (x1 + 2.0) - (x0 - 2.0),
        (y1 + 2.0) - (y0 - 2.0),
        border,
    );
    scene.rect_to_layer(SceneLayer::Overlay, x0, y0, x1 - x0, y1 - y0, bg);
    // Header background (darker, like settings title bar)
    scene.rect_to_layer(SceneLayer::Overlay, x0, y0, x1 - x0, header_h, header_bg);
    scene.rect_to_layer(
        SceneLayer::Overlay,
        x0,
        y0 + header_h - 1.0,
        x1 - x0,
        1.0,
        divider,
    );
    scene.text_to_layer(
        SceneLayer::Overlay,
        x0 + layout.cell_w_px * 0.8,
        y0 + (header_h - layout.cell_h_px) * 0.5,
        truncate_chars(&format!("> {}", cp.query), PALETTE_MAX_CHARS),
        text,
    );

    for i in 0..visible {
        let idx = cp.scroll_offset + i;
        if idx >= cp.items.len() {
            break;
        }
        let row_y = y0 + header_h + i as f32 * item_h;
        if idx == cp.selected {
            scene.rect_to_layer(SceneLayer::Overlay, x0, row_y, x1 - x0, item_h, selected);
        }
        scene.text_to_layer(
            SceneLayer::Overlay,
            x0 + layout.cell_w_px * 0.8,
            row_y + (item_h - layout.cell_h_px) * 0.5,
            truncate_chars(&cp.items[idx], PALETTE_MAX_CHARS),
            if idx == cp.selected { text } else { dim_text },
        );
    }
}
