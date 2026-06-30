/// Toast notifications: transient, temporary UI messages.
///
/// Rendered in the Toast layer at bottom-right corner.
/// Emits geometry and text commands to Scene.
use crate::{RenderContext, Scene, SceneLayer, ToastKind};

/// Render all active toasts based on snapshot state.
/// Called from GlPainter to emit toast notifications into the Scene.
pub fn render(ctx: &RenderContext, scene: &mut Scene) {
    if ctx.snapshot.toast_stack.is_empty() {
        return;
    }

    let layout = ctx.layout;
    let margin = layout.cell_h_px * 0.35;

    for (rev_idx, toast) in ctx.snapshot.toast_stack.iter().rev().enumerate() {
        let max_chars = toast.text.chars().count().max(4) as f32;
        let h = layout.cell_h_px * 1.5;
        let pad_h = layout.cell_w_px * 1.2;
        let w = (max_chars * layout.cell_w_px + pad_h * 2.0).min(layout.width * 0.45);
        let bottom = layout.height - margin - rev_idx as f32 * (h + margin);
        let top = bottom - h;
        let right = layout.width - margin;
        let left = right - w;

        // Determine colors based on toast kind
        let (bg, border, text_color) = match toast.kind {
            ToastKind::Info => (
                [0.12, 0.15, 0.25, 0.93],
                [0.35, 0.50, 0.90, 1.0],
                [0.92, 0.94, 0.98, 1.0],
            ),
            ToastKind::Success => (
                [0.08, 0.20, 0.10, 0.93],
                [0.25, 0.78, 0.35, 1.0],
                [0.90, 1.00, 0.90, 1.0],
            ),
            ToastKind::Warn => (
                [0.22, 0.18, 0.05, 0.93],
                [0.90, 0.72, 0.20, 1.0],
                [1.00, 0.97, 0.85, 1.0],
            ),
            ToastKind::Error => (
                [0.22, 0.08, 0.08, 0.93],
                [0.90, 0.30, 0.30, 1.0],
                [1.00, 0.90, 0.90, 1.0],
            ),
        };

        // Border
        scene.rect_to_layer(
            SceneLayer::Toast,
            left - 1.0,
            top - 1.0,
            w + 2.0,
            h + 2.0,
            border,
        );

        // Background
        scene.rect_to_layer(SceneLayer::Toast, left, top, w, h, bg);

        // Text (emitted as TextCommand for rendering via the text system)
        let text = toast.text.clone();
        scene.text_to_layer(
            SceneLayer::Toast,
            left + pad_h,
            top + (h - layout.cell_h_px) * 0.5,
            text,
            text_color,
        );
    }
}
