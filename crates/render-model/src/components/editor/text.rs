/// Editor text rendering: emits text commands for all visible editor lines.
use crate::{RenderContext, Scene, SceneLayer};

fn char_col_width(ch: char) -> usize {
    let cp = ch as u32;
    if matches!(cp,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x9FFF
        | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF01..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F000..=0x1FAFF
    ) {
        2
    } else {
        1
    }
}

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
        let mut vcol = 0usize;
        for (char_idx, ch) in line.chars().enumerate() {
            let cw = char_col_width(ch);
            if vcol + cw <= hscroll {
                vcol += cw;
                continue;
            }

            let x = layout.padding_h + vcol.saturating_sub(hscroll) as f32 * layout.cell_w_px;
            let w = cw as f32 * layout.cell_w_px;
            if x + w > max_x {
                break;
            }

            let idx = line_char_start + char_idx;
            let color = snapshot
                .editor_fg_colors
                .get(idx)
                .and_then(|c| *c)
                .map(|c| [c[0] * dim, c[1] * dim, c[2] * dim, 1.0])
                .unwrap_or(default_fg);

            scene.text_to_layer(SceneLayer::Main, x, y, ch.to_string(), color);
            vcol += cw;
        }

        line_char_start = line_char_start.saturating_add(line_len + 1);
    }
}
