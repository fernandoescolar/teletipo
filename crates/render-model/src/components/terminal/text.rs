/// Terminal text rendering: emits text commands for all visible terminal lines.
/// Uses per-character colors to support ANSI color codes.
use crate::{RenderContext, Scene, SceneLayer};

/// Render terminal text with per-character colors.
pub fn render_text(ctx: &RenderContext, scene: &mut Scene) {
    let layout = ctx.layout;
    let snapshot = ctx.snapshot;

    let max_y = layout.terminal_text_bottom;

    for (row, render_row) in snapshot.terminal_rows.iter().enumerate() {
        let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;
        if y >= max_y {
            break;
        }

        let x = layout.padding_h;
        let mut text = String::with_capacity(render_row.cells.len());
        let mut colors = Vec::with_capacity(render_row.cells.len());

        for cell in &render_row.cells {
            text.push(cell.ch);
            colors.push(
                cell.fg
                    .map(|c| [c[0], c[1], c[2], 1.0])
                    .unwrap_or(snapshot.theme.text),
            );
        }

        // Emit TextCommand with per-character colors
        if !colors.is_empty() && colors.len() == render_row.cells.len() {
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
    }
}
