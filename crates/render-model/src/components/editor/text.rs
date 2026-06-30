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
    let row_offset = snapshot.editor_scroll_offset;

    for (line_idx, line) in lines.iter().copied().enumerate() {
        // Skip scrolled-out lines
        if line_idx < row_offset {
            continue;
        }

        let row = line_idx - row_offset;
        let y = layout.editor_top + layout.padding_v + row as f32 * layout.cell_h_px;
        if y + layout.cell_h_px > max_y {
            break;
        }

        let text = line.to_string();

        // Emit the line with horizontal scroll offset applied
        scene.text_to_layer(
            SceneLayer::Main,
            layout.padding_h - snapshot.editor_horizontal_scroll_offset as f32 * layout.cell_w_px,
            y,
            text,
            snapshot.theme.text,
        );
    }
}
