/// Editor text selection: highlighted background for selected text.
///
/// Renders semi-transparent rectangles over selected text in the editor pane.

use crate::{RenderContext, Scene, SceneLayer};

/// Render editor text selection background.
/// Called from GlPainter to emit selection geometry into the Scene.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let Some((a, b)) = ctx.snapshot.editor_selection else {
        return;
    };

    let layout = ctx.layout;
    let snapshot = ctx.snapshot;

    // Normalize selection range
    let (start, end) = if a <= b { (a, b) } else { (b, a) };
    if end <= start {
        return;
    }

    let selection_color = [0.35, 0.50, 0.80, 0.35]; // Blue, semi-transparent

    // Iterate through visible editor text lines
    let mut char_idx = 0usize;
    for (line_idx, line) in snapshot.editor_text.lines().enumerate() {
        // Skip scrolled-out lines
        if line_idx < snapshot.editor_scroll_offset {
            char_idx = char_idx.saturating_add(line.chars().count() + 1);
            continue;
        }

        // Position relative to visible area
        let visible_row = line_idx - snapshot.editor_scroll_offset;
        let row_start_idx = char_idx;
        let row_end_idx = char_idx + line.chars().count();

        // Skip if selection ends before this row
        if end < row_start_idx {
            break;
        }

        // Render selection for this row if it overlaps
        if start <= row_end_idx {
            // Calculate selection range within this line
            let from = start.saturating_sub(row_start_idx);
            let to = end.min(row_end_idx).saturating_sub(row_start_idx);

            if to > from {
                let horizontal_scroll = snapshot.editor_horizontal_scroll_offset as f32;
                let y = layout.editor_top + layout.padding_v + visible_row as f32 * layout.cell_h_px;
                let x0 = layout.padding_h + (from as f32 - horizontal_scroll) * layout.cell_w_px;
                let x1 = layout.padding_h + (to as f32 - horizontal_scroll) * layout.cell_w_px;

                scene.rect_to_layer(
                    SceneLayer::Main,
                    x0,
                    y,
                    x1 - x0,
                    layout.cell_h_px,
                    selection_color,
                );
            }
        }

        char_idx = row_end_idx.saturating_add(1);
    }
}
