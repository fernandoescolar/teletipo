/// Editor text rendering: emits text commands for all visible editor lines.
use crate::{RenderContext, Scene, SceneLayer};

/// Render editor text.
pub fn render_text(ctx: &RenderContext, scene: &mut Scene) {
    let layout = ctx.layout;
    let snapshot = ctx.snapshot;

    if snapshot.editor_text.is_empty() {
        return;
    }

    let lines: Vec<&str> = snapshot.editor_text.lines().collect();
    let max_y = layout.height - layout.padding_v;
    let max_x = layout.width - layout.padding_h;
    let row_offset = snapshot.editor_scroll_offset;
    let dim = if snapshot.editor_disabled { 0.35 } else { 1.0 };
    let default_fg = [
        snapshot.theme.text[0] * dim,
        snapshot.theme.text[1] * dim,
        snapshot.theme.text[2] * dim,
        1.0,
    ];
    let mut line_char_start = 0usize;

    for (line_idx, line) in lines.iter().copied().enumerate() {
        let line_len = line.chars().count();
        // Skip scrolled-out lines
        if line_idx < row_offset {
            line_char_start = line_char_start.saturating_add(line_len + 1);
            continue;
        }

        let row = line_idx - row_offset;
        let y = layout.editor_top + layout.padding_v + row as f32 * layout.cell_h_px;
        if y + layout.cell_h_px > max_y {
            break;
        }

        let hscroll = snapshot.editor_horizontal_scroll_offset;
        for (col, ch) in line.chars().enumerate() {
            if col < hscroll {
                continue;
            }
            let x = layout.padding_h + (col - hscroll) as f32 * layout.cell_w_px;
            if x + layout.cell_w_px > max_x {
                break;
            }

            let idx = line_char_start + col;
            let color = snapshot
                .editor_fg_colors
                .get(idx)
                .and_then(|c| *c)
                .map(|c| [c[0] * dim, c[1] * dim, c[2] * dim, 1.0])
                .unwrap_or(default_fg);

            scene.text_to_layer(SceneLayer::Main, x, y, ch.to_string(), color);
        }

        line_char_start = line_char_start.saturating_add(line_len + 1);
    }
}
