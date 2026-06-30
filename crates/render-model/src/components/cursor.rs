/// Cursor rendering: text insertion point indicator.
///
/// Renders the cursor as:
/// - Block: full cell (default for terminal)
/// - Underline: thin line at cell bottom
/// - Vertical bar: thin line at cell left

use crate::{RenderContext, Scene, SceneLayer};

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
            col += 1;
        }

        current_offset += ch.len_utf8();
    }

    (row, col)
}

/// Render the text cursor (insertion point).
/// Called from GlPainter to emit cursor geometry into the Scene.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    // Don't render cursor when blinking is off
    if !ctx.snapshot.cursor_blink_on {
        return;
    }

    let color = ctx.snapshot.theme.cursor;
    let layout = ctx.layout;
    let snapshot = ctx.snapshot;

    // === Editor cursor ===
    // Render as full block if editor is focused and not in fullscreen terminal mode
    if snapshot.editor_focused && !snapshot.terminal_fullscreen && !snapshot.editor_disabled {
        let (row, col) = offset_to_row_col(&snapshot.editor_text, snapshot.editor_cursor_offset);
        let visible_row = row.saturating_sub(snapshot.editor_scroll_offset);
        let x = layout.padding_h
            + (col as f32 - snapshot.editor_horizontal_scroll_offset as f32) * layout.cell_w_px;
        let y = layout.editor_top + layout.padding_v + visible_row as f32 * layout.cell_h_px;

        scene.rect_to_layer(SceneLayer::Main, x, y, layout.cell_w_px, layout.cell_h_px, color);
        return;
    }

    // === Terminal cursor ===
    // Render based on cursor shape: block, underline, or vertical bar
    let row = snapshot.terminal_cursor_row;
    let col = snapshot.terminal_cursor_col;
    let x = layout.padding_h + col as f32 * layout.cell_w_px;
    let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;

    if y < layout.terminal_text_bottom {
        match snapshot.cursor_shape {
            // Underline cursor (shapes 3, 4)
            3 | 4 => {
                let h = (layout.cell_h_px * 0.12).max(2.0);
                scene.rect_to_layer(
                    SceneLayer::Main,
                    x,
                    y + layout.cell_h_px - h,
                    layout.cell_w_px,
                    h,
                    color,
                );
            }
            // Vertical bar cursor (shapes 5, 6)
            5 | 6 => {
                let w = (layout.cell_w_px * 0.12).max(2.0);
                scene.rect_to_layer(SceneLayer::Main, x, y, w, layout.cell_h_px, color);
            }
            // Block cursor (default)
            _ => {
                scene.rect_to_layer(
                    SceneLayer::Main,
                    x,
                    y,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    color,
                );
            }
        }
    }
}
