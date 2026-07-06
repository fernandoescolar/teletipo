//! Geometric utilities for bounds and rectangles.

use gpui::{Bounds, Pixels, point, px, size};

/// Convert render-model Rect to GPUI Bounds.
pub(crate) fn rect(rect: render_model::Rect) -> Bounds<Pixels> {
    Bounds {
        origin: point(px(rect.x), px(rect.y)),
        size: size(px(rect.w.max(0.0)), px(rect.h.max(0.0))),
    }
}
