//! Color conversion utilities.

use gpui::{Background, Hsla, Rgba, rgba};

/// Convert RGBA color array to GPUI Background.
pub fn background_color(color: [f32; 4], alpha_multiplier: f32) -> Background {
    rgba_hex(color, alpha_multiplier).into()
}

/// Convert RGBA color array to GPUI Hsla (text color).
pub fn text_color(color: [f32; 4], alpha_multiplier: f32) -> Hsla {
    rgba_hex(color, alpha_multiplier).into()
}

/// Convert normalized RGBA array [0.0-1.0] to GPUI Rgba with alpha multiplier.
pub(crate) fn rgba_hex(color: [f32; 4], alpha_multiplier: f32) -> Rgba {
    rgba(
        ((color[0].clamp(0.0, 1.0) * 255.0).round() as u32) << 24
            | ((color[1].clamp(0.0, 1.0) * 255.0).round() as u32) << 16
            | ((color[2].clamp(0.0, 1.0) * 255.0).round() as u32) << 8
            | ((color[3].clamp(0.0, 1.0) * alpha_multiplier.clamp(0.0, 1.0) * 255.0).round()
                as u32),
    )
}
