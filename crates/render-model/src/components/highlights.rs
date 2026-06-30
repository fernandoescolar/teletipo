/// Terminal highlights: search results, current match, selection, links.
///
/// Renders semi-transparent rectangles over terminal cells to highlight:
/// - Search query matches (blue)
/// - Current match (orange/yellow)
/// - Text selection (blue)
/// - Detected links (underline)
use crate::{RenderContext, Scene, SceneLayer};

/// Normalize a rectangular selection defined by two corners.
/// Returns (start_row, start_col, end_row, end_col) with start <= end.
fn normalize_rect_selection(
    r0: usize,
    c0: usize,
    r1: usize,
    c1: usize,
) -> (usize, usize, usize, usize) {
    if r0 < r1 || (r0 == r1 && c0 <= c1) {
        (r0, c0, r1, c1)
    } else {
        (r1, c1, r0, c0)
    }
}

/// Render all terminal highlights based on snapshot state.
/// Called from GlPainter to emit highlight geometry into the Scene.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let layout = ctx.layout;
    let snapshot = ctx.snapshot;

    // Color palette for different highlight types
    let search_hl_color = [0.40, 0.55, 0.85, 0.35]; // Blue, semi-transparent
    let current_hl_color = [0.85, 0.65, 0.20, 0.45]; // Orange, semi-transparent
    let selection_color = [0.35, 0.50, 0.80, 0.35]; // Blue (same as search)
    let link_color = [0.25, 0.70, 1.00, 0.90]; // Bright blue for underlines

    // === Search highlights ===
    // Render rectangles for all search query matches
    for (row, start, len) in &snapshot.search_highlights {
        if *len == 0 {
            continue;
        }

        let y = layout.terminal_text_top + *row as f32 * layout.cell_h_px;

        // Skip if outside visible terminal area
        if y < layout.terminal_text_top || y + layout.cell_h_px > layout.terminal_text_bottom {
            continue;
        }

        let x0 = layout.padding_h + *start as f32 * layout.cell_w_px;
        let x1 = x0 + *len as f32 * layout.cell_w_px;

        scene.rect_to_layer(
            SceneLayer::Main,
            x0,
            y,
            x1 - x0,
            layout.cell_h_px,
            search_hl_color,
        );
    }

    // === Current search highlight ===
    // Render with distinct color (orange) to show which match is active
    if let Some((row, start, len)) = snapshot.search_current_highlight
        && len > 0
    {
        let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;

        // Skip if outside visible terminal area
        if y >= layout.terminal_text_top && y + layout.cell_h_px <= layout.terminal_text_bottom {
            let x0 = layout.padding_h + start as f32 * layout.cell_w_px;
            let x1 = x0 + len as f32 * layout.cell_w_px;

            scene.rect_to_layer(
                SceneLayer::Main,
                x0,
                y,
                x1 - x0,
                layout.cell_h_px,
                current_hl_color,
            );
        }
    }

    // === Terminal text selection ===
    // Render rectangles for selected text region
    if let Some((r0, c0, r1, c1)) = snapshot.selection {
        let (sr, sc, er, ec) = normalize_rect_selection(r0, c0, r1, c1);

        for row in sr..=er {
            // Determine column range for this row (may be partial)
            let from = if row == sr { sc } else { 0 };
            let to = if row == er {
                ec
            } else {
                (layout.width / layout.cell_w_px) as usize
            };

            if to <= from {
                continue;
            }

            let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;

            // Skip if outside visible terminal area
            if y < layout.terminal_text_top || y + layout.cell_h_px > layout.terminal_text_bottom {
                continue;
            }

            let x0 = layout.padding_h + from as f32 * layout.cell_w_px;
            let x1 = layout.padding_h + to as f32 * layout.cell_w_px;

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

    // === Link underlines ===
    // Render thin underlines for detected terminal links
    for link in &snapshot.terminal_links {
        let y = layout.terminal_text_top + link.row as f32 * layout.cell_h_px;

        // Skip if outside visible terminal area
        if y < layout.terminal_text_top || y + layout.cell_h_px > layout.terminal_text_bottom {
            continue;
        }

        let x0 = layout.padding_h + link.col_start as f32 * layout.cell_w_px;
        let x1 = layout.padding_h + link.col_end as f32 * layout.cell_w_px;

        // Underline at bottom of cell
        let underline_thickness = (layout.cell_h_px * 0.08).max(1.0);
        let underline_y = y + layout.cell_h_px - (layout.cell_h_px * 0.10).max(1.0);

        scene.rect_to_layer(
            SceneLayer::Main,
            x0,
            underline_y,
            x1 - x0,
            underline_thickness,
            link_color,
        );
    }
}
