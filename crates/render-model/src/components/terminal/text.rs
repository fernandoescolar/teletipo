/// Terminal text rendering: emits text commands for all visible terminal lines.
/// Uses per-character colors to support ANSI color codes.
use crate::{RenderContext, Scene, SceneLayer};

/// Render terminal text with per-character colors.
pub fn render_text(ctx: &RenderContext, scene: &mut Scene) {
    let layout = ctx.layout;
    let snapshot = ctx.snapshot;

    let terminal_text = snapshot.terminal_text_from_rows();
    let max_y = layout.terminal_text_bottom;
    let lines: Vec<&str> = terminal_text.lines().collect();
    let mut char_offset = 0usize;

    for (row, line) in lines.iter().copied().enumerate() {
        let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;
        if y >= max_y {
            break;
        }

        let x = layout.padding_h;
        let text = line.to_string();
        let char_count = text.chars().count();

        // Collect per-character colors from snapshot
        let colors: Vec<[f32; 4]> = (0..char_count)
            .map(|i| {
                let idx = char_offset + i;
                snapshot
                    .terminal_fg_colors
                    .get(idx)
                    .and_then(|c| *c)
                    .map(|c| [c[0], c[1], c[2], 1.0])
                    .unwrap_or(snapshot.theme.text)
            })
            .collect();

        // Emit TextCommand with per-character colors
        if !colors.is_empty() && colors.len() == char_count {
            scene.text_with_colors_to_layer(
                SceneLayer::Main,
                x,
                y,
                text,
                colors,
                snapshot.theme.text,
            );
        } else {
            scene.text_to_layer(SceneLayer::Main, x, y, text, snapshot.theme.text);
        }

        char_offset += char_count + 1;
    }
}
