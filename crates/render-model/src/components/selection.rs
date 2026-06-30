/// Editor text selection: highlighted background for selected text.
///
/// Renders semi-transparent rectangles over selected text in the editor pane.
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

fn byte_to_visual_col(line: &str, byte_off: usize) -> usize {
    let clamped = byte_off.min(line.len());
    let mut col = 0usize;
    for (i, ch) in line.char_indices() {
        if i >= clamped {
            break;
        }
        col += char_col_width(ch);
    }
    col
}

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

    // Iterate through visible editor text lines using byte offsets, because
    // editor_selection is stored as a byte range.
    let mut row_start_byte = 0usize;
    for (line_idx, line) in snapshot.editor_text.split('\n').enumerate() {
        let row_end_byte = row_start_byte + line.len();

        // Skip scrolled-out lines
        if line_idx < snapshot.editor_scroll_offset {
            row_start_byte = row_end_byte.saturating_add(1);
            continue;
        }

        // Position relative to visible area
        let visible_row = line_idx - snapshot.editor_scroll_offset;

        // Skip if selection ends before this row
        if end <= row_start_byte {
            break;
        }

        // Render selection for this row if it overlaps
        if start < row_end_byte && end > row_start_byte {
            // Calculate selection range within this line (byte offsets).
            let from_byte = start.max(row_start_byte) - row_start_byte;
            let to_byte = end.min(row_end_byte) - row_start_byte;

            // Convert to visual columns so wide glyphs (emoji/icons) are
            // highlighted with the same geometry used for caret/text layout.
            let from = byte_to_visual_col(line, from_byte);
            let to = byte_to_visual_col(line, to_byte);

            if to > from {
                let horizontal_scroll = snapshot.editor_horizontal_scroll_offset as f32;
                let y =
                    layout.editor_top + layout.padding_v + visible_row as f32 * layout.cell_h_px;
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

        row_start_byte = row_end_byte.saturating_add(1);
    }
}
