/// Editor suggestion: faint auto-complete preview text.
///
/// Renders a dimmed suggestion string at the cursor position to preview
/// what text would be inserted if the user accepts the suggestion.
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

/// Convert text offset to (row, col) in the text.
fn offset_to_row_col(text: &str, offset: usize) -> (usize, usize) {
    let mut row = 0;
    let mut col = 0;
    let mut current_offset = 0;

    for ch in text.chars() {
        if current_offset >= offset {
            break;
        }

        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += char_col_width(ch);
        }

        current_offset += ch.len_utf8();
    }

    (row, col)
}

/// Render editor suggestion text (faint auto-complete preview).
/// Called from GlPainter to emit suggestion text into the Scene.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    if ctx.snapshot.editor_suggestion.is_empty() {
        return;
    }

    let layout = ctx.layout;
    let snapshot = ctx.snapshot;

    // Get cursor position
    let (row, col) = offset_to_row_col(&snapshot.editor_text, snapshot.editor_cursor_offset);
    let visible_row = row.saturating_sub(snapshot.editor_scroll_offset);
    let y = layout.editor_top + layout.padding_v + visible_row as f32 * layout.cell_h_px;
    let base_x = layout.padding_h
        + (col as f32 - snapshot.editor_horizontal_scroll_offset as f32) * layout.cell_w_px;

    // Suggestion color: theme text color with reduced alpha (faint)
    let [r, g, b, _] = snapshot.theme.text;
    let suggestion_color = [r, g, b, 0.45];

    // Emit suggestion as a single TextCommand
    // (Will be rendered character by character with monospace layout)
    let mut visible_suggestion = String::new();
    let mut used_cols = 0usize;
    for ch in snapshot.editor_suggestion.chars() {
        let cw = char_col_width(ch);
        let x = base_x + used_cols as f32 * layout.cell_w_px;
        // Stop if suggestion would extend past right edge
        if x + cw as f32 * layout.cell_w_px > layout.width - layout.padding_h {
            break;
        }
        visible_suggestion.push(ch);
        used_cols += cw;
    }

    if !visible_suggestion.is_empty() {
        scene.text_to_layer(
            SceneLayer::Floating,
            base_x,
            y,
            visible_suggestion,
            suggestion_color,
        );
    }
}
