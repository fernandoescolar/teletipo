/// Scrollbars: visual indicators of scroll position.
///
/// Renders scrollbar tracks and thumbs for:
/// - Terminal vertical scrollbar (right edge)
/// - Editor vertical scrollbar (right edge)
/// - Editor horizontal scrollbar (bottom)
use crate::{RenderContext, SCROLLBAR_W_PX, Scene, SceneLayer};

/// Render all visible scrollbars based on scroll state.
/// Called from GlPainter to emit scrollbar geometry into the Scene.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    let layout = ctx.layout;
    let snapshot = ctx.snapshot;

    // Scrollbar color: lighter version of separator
    let [r, g, b, _] = snapshot.theme.separator_focused;
    let thumb_color = [r, g, b, 0.85];
    let track_color = snapshot.theme.separator;

    let sb_w = SCROLLBAR_W_PX;
    let sb_left = layout.width - sb_w;

    // === Terminal vertical scrollbar ===
    // Show if there's scrollback history
    if snapshot.scrollback_lines > 0 {
        let track_top = layout.tab_bar_h;
        let track_bottom = layout.terminal_h;
        let track_h = track_bottom - track_top;

        if track_h > 0.0 && layout.cell_h_px > 0.0 {
            // Draw track background
            scene.rect_to_layer(
                SceneLayer::Main,
                sb_left,
                track_top,
                sb_w,
                track_h,
                track_color,
            );

            // Calculate and draw thumb
            let visible_rows = (track_h / layout.cell_h_px).floor();
            let total_rows = visible_rows + snapshot.scrollback_lines as f32;
            let thumb_h = (visible_rows / total_rows).clamp(0.05, 1.0) * track_h;
            let scroll_pos =
                (snapshot.scroll_offset as f32 / snapshot.scrollback_lines as f32).clamp(0.0, 1.0);
            let thumb_top = track_top + (1.0 - scroll_pos) * (track_h - thumb_h);

            scene.rect_to_layer(
                SceneLayer::Main,
                sb_left,
                thumb_top,
                sb_w,
                thumb_h,
                thumb_color,
            );
        }
    }

    // === Editor vertical scrollbar ===
    // Show if editor has more content than visible area
    let editor_h = layout.height - layout.editor_top;
    let visible_rows = ((editor_h - layout.padding_v) / layout.cell_h_px)
        .floor()
        .max(1.0);

    if snapshot.editor_line_count as f32 > visible_rows {
        // Draw track background
        scene.rect_to_layer(
            SceneLayer::Main,
            sb_left,
            layout.editor_top,
            sb_w,
            editor_h,
            track_color,
        );

        // Calculate and draw thumb
        let thumb_h =
            (visible_rows / snapshot.editor_line_count as f32).clamp(0.05, 1.0) * editor_h;
        let max_scroll = snapshot.editor_line_count as f32 - visible_rows;
        let scroll_pos = (snapshot.editor_scroll_offset as f32 / max_scroll).clamp(0.0, 1.0);
        let thumb_top = layout.editor_top + scroll_pos * (editor_h - thumb_h);

        scene.rect_to_layer(
            SceneLayer::Main,
            sb_left,
            thumb_top,
            sb_w,
            thumb_h,
            thumb_color,
        );
    }

    // === Editor horizontal scrollbar ===
    // Show if editor has longer lines than visible area
    let max_cols = snapshot
        .editor_text
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    let track_left = layout.padding_h;
    let track_right = sb_left - layout.padding_h;
    let track_w = track_right - track_left;
    let visible_cols = (track_w / layout.cell_w_px).floor().max(1.0);

    if max_cols as f32 > visible_cols && track_w > 0.0 {
        let track_top = layout.height - SCROLLBAR_W_PX;

        // Draw track background
        scene.rect_to_layer(
            SceneLayer::Main,
            track_left,
            track_top,
            track_w,
            SCROLLBAR_W_PX,
            track_color,
        );

        // Calculate and draw thumb
        let thumb_w = (visible_cols / max_cols as f32).clamp(0.05, 1.0) * track_w;
        let max_scroll = max_cols as f32 - visible_cols;
        let scroll_pos =
            (snapshot.editor_horizontal_scroll_offset as f32 / max_scroll).clamp(0.0, 1.0);
        let thumb_left = track_left + scroll_pos * (track_w - thumb_w);

        scene.rect_to_layer(
            SceneLayer::Main,
            thumb_left,
            track_top,
            thumb_w,
            SCROLLBAR_W_PX,
            thumb_color,
        );
    }
}
