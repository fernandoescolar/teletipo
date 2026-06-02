use std::sync::Arc;

// ── Style bit flags ───────────────────────────────────────────────────────────

pub(crate) const STYLE_BOLD: u8 = 0b001;
pub(crate) const STYLE_ITALIC: u8 = 0b010;
pub(crate) const STYLE_STRIKE: u8 = 0b100;

// ── Layout constants ──────────────────────────────────────────────────────────

pub(crate) const SEPARATOR_PX: f32 = 2.0;
pub(crate) const TAB_H_MULT: f32 = 1.0;
pub(crate) const PALETTE_MAX_VISIBLE: usize = 10;
pub(crate) const SETTINGS_MAX_VISIBLE_SEARCH: usize = 8;

// ── Atlas constant ────────────────────────────────────────────────────────────

/// Side length (in texels) of the per-painter glyph atlas texture.
pub(crate) const ATLAS_TEX_SIZE: u32 = 1024;

/// Side length of the RGBA color-emoji atlas texture.
/// Larger than the grayscale atlas since emoji bitmaps are 20–160 px each.
pub(crate) const COLOR_ATLAS_TEX_SIZE: u32 = 2048;

// ── Per-frame layout geometry ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameLayout {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) tab_bar_h: f32,
    pub(crate) terminal_h: f32,
    pub(crate) editor_top: f32,
    pub(crate) terminal_text_top: f32,
    pub(crate) terminal_text_bottom: f32,
    pub(crate) padding_h: f32,
    pub(crate) padding_v: f32,
    pub(crate) cell_w_px: f32,
    pub(crate) cell_h_px: f32,
}

// ── Atlas glyph descriptor ────────────────────────────────────────────────────

/// A glyph that has been packed into the GPU atlas texture.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AtlasGlyph {
    /// Normalised UV coordinates inside the atlas.
    pub(crate) u0: f32,
    pub(crate) v0: f32,
    pub(crate) u1: f32,
    pub(crate) v1: f32,
    /// Fontdue left bearing.
    pub(crate) xmin: f32,
    /// Fontdue bottom bearing (positive = above baseline).
    pub(crate) ymin: f32,
    /// Rasterised glyph width in pixels.
    pub(crate) source_gw: f32,
    /// Rasterised glyph height in pixels.
    pub(crate) source_gh: f32,
    /// Horizontal advance width reported by fontdue.
    pub(crate) advance_width: f32,
}

// ── Font / shaping types ──────────────────────────────────────────────────────

/// A color emoji glyph packed into the RGBA color atlas texture.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ColorAtlasEntry {
    /// Normalised UV coordinates inside the color atlas.
    pub(crate) u0: f32,
    pub(crate) v0: f32,
    pub(crate) u1: f32,
    pub(crate) v1: f32,
    /// Original image dimensions (pixels) at the cached ppem.
    pub(crate) w_px: u32,
    pub(crate) h_px: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct GlyphBitmap {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) xmin: f32,
    pub(crate) ymin: f32,
    pub(crate) advance_width: f32,
    pub(crate) alpha: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct FontSource {
    pub(crate) bytes: Box<[u8]>,
    pub(crate) face_index: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ShapedGlyph {
    pub(crate) glyph_id: u16,
    pub(crate) source_char: char,
    pub(crate) col: usize,
    pub(crate) span_cols: usize,
    pub(crate) full_char_idx: usize,
    pub(crate) x_offset_px: f32,
    pub(crate) y_offset_px: f32,
}

pub(crate) type ShapedLines = Vec<Vec<ShapedGlyph>>;
pub(crate) type ShapedTerminalCache = (u64, u64, Arc<ShapedLines>);

// ── Colour helpers ────────────────────────────────────────────────────────────

pub(crate) fn clamp_color(mut c: [f32; 4], d: f32) -> [f32; 4] {
    c[0] = (c[0] + d).clamp(0.0, 1.0);
    c[1] = (c[1] + d).clamp(0.0, 1.0);
    c[2] = (c[2] + d).clamp(0.0, 1.0);
    c
}

pub(crate) fn mix_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}
