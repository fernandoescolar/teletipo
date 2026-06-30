/// Scroll indicator: visual indicator showing scroll position.
///
/// Rendered in the Overlay layer to show user they can scroll.
/// Emits geometry only; text rendering is deferred to the old painter path.
use crate::{Color, RenderContext, Scene, SceneLayer};

/// Render scroll indicator based on snapshot state.
/// Called from GlPainter to emit scroll position feedback into the Scene.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    if ctx.snapshot.scroll_offset == 0 {
        return;
    }

    let layout = ctx.layout;
    let h = layout.cell_h_px * 1.4;
    let w = layout.cell_w_px * 14.0;
    let margin = layout.cell_h_px * 0.5;
    let cx = layout.width * 0.5;
    let bottom = layout.terminal_h - margin;
    let top = bottom - h;
    let left = cx - w * 0.5;
    let right = cx + w * 0.5;

    // Outer border (lighter)
    let border_color: Color = [0.40, 0.70, 1.00, 0.80];
    scene.rect_to_layer(
        SceneLayer::Overlay,
        left - 1.0,
        top - 1.0,
        right - left + 2.0,
        h + 2.0,
        border_color,
    );

    // Main background
    let bg_color: Color = [0.08, 0.10, 0.18, 0.88];
    scene.rect_to_layer(SceneLayer::Overlay, left, top, w, h, bg_color);

    // TODO: Text rendering ("↑ N lines")
}
