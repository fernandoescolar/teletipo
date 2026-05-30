//! Glyph rasterisation and atlas-cache management for `GpuState`.
//!
//! Holds the per-glyph cache lookup methods (`ensure_glyph`,
//! `ensure_shaped_glyph`) and the font-rescale routine that invalidates them.
//! Split out of `pipeline.rs` to keep the renderer constructor focused on
//! resource wiring.

use crate::atlas::{load_emoji_font_bytes, pack_emoji_glyph, pack_glyph};
use crate::pipeline::GpuState;

impl<'a> GpuState<'a> {
    /// Lazily load the colour-emoji font on first need.  Emoji fonts are
    /// huge (Apple Color Emoji is ~180 MB) and most sessions never render an
    /// emoji, so we defer the allocation until it's actually required.
    /// `emoji_load_attempted` ensures we only try once per renderer lifetime.
    fn ensure_emoji_font_loaded(&mut self) {
        if self.emoji_load_attempted {
            return;
        }
        self.emoji_load_attempted = true;
        // One-time scan on first emoji usage so Linux can discover
        // Noto Color Emoji via fontdb.
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let bytes = load_emoji_font_bytes(Some(&db)).or_else(|| load_emoji_font_bytes(None));
        match bytes {
            Some(bytes) => {
                self.emoji_font_bytes = Some(bytes.into_boxed_slice());
            }
            None => {
                tracing::warn!("no colour emoji font found; emoji will not be rendered");
            }
        }
    }

    /// Recompute glyph metrics for a new font size and invalidate every cached
    /// rasterisation. Cheap no-op if the size change is below half a pixel.
    pub(crate) fn rescale_font(&mut self, base_font_size: f32, scale_factor: f64) {
        let new_size = base_font_size * scale_factor as f32;
        if (new_size - self.font_size).abs() < 0.5 {
            return;
        }
        self.font_size = new_size;
        self.cell_h_px = new_size * 1.2;
        if let Some(ref f) = self.font {
            self.cell_w_px = f.metrics('M', new_size).advance_width;
        }
        self.glyph_cache.clear();
        self.bold_glyph_cache.clear();
        self.shaped_glyph_cache.clear();
        self.atlas_alloc_x = 0;
        self.atlas_alloc_y = 0;
        self.atlas_row_h = 0;
    }

    /// Ensure a ligature glyph (identified by its TTF glyph ID) is rasterized and
    /// stored in the shaped glyph cache.  No-op if already cached.
    pub(crate) fn ensure_shaped_glyph(&mut self, glyph_id: u16, bold: bool) {
        let key = (glyph_id, bold);
        if self.shaped_glyph_cache.contains_key(&key) {
            return;
        }
        let font = if bold {
            self.bold_font.as_ref().or(self.font.as_ref())
        } else {
            self.font.as_ref()
        };
        let Some(font) = font else { return };
        let (metrics, bitmap) = font.rasterize_indexed(glyph_id, self.font_size);
        let cached = pack_glyph(
            &self.queue,
            &self.atlas_texture,
            &mut self.atlas_alloc_x,
            &mut self.atlas_alloc_y,
            &mut self.atlas_row_h,
            &metrics,
            &bitmap,
            self.cell_h_px,
        );
        self.shaped_glyph_cache.insert(key, cached);
    }

    /// Rasterise `ch` into the atlas and cache it. Falls back to the Unicode
    /// symbol font, then to the colour-emoji font for characters the primary
    /// font lacks. Cheap no-op if already cached.
    pub(crate) fn ensure_glyph(&mut self, ch: char) {
        if self.glyph_cache.contains_key(&ch) {
            return;
        }
        let font = match self.font.take() {
            Some(f) => f,
            None => return,
        };
        let font_size = self.font_size;
        let cell_h_px = self.cell_h_px;

        // Check whether the character is actually in the primary font's cmap.
        // fontdue::Font::rasterize returns the .notdef (tofu box) glyph for missing
        // characters, and that glyph has non-zero metrics — so we cannot rely on
        // `metrics.width > 0` alone to detect "font has this char".  Instead we
        // call lookup_glyph_index which returns 0 when the char is absent.
        let not_in_primary = ch > '\u{7E}' && font.lookup_glyph_index(ch) == 0;
        let (metrics, bitmap) = font.rasterize(ch, font_size);

        // Try Unicode symbol fallback (e.g. Apple Symbols) for non-ASCII not in primary.
        let (final_metrics, final_bitmap, found) = if not_in_primary {
            if let Some(ref fb) = self.unicode_fallback_font {
                if fb.lookup_glyph_index(ch) != 0 {
                    let (m, b) = fb.rasterize(ch, font_size);
                    if m.width > 0 && m.height > 0 {
                        (m, b, true)
                    } else {
                        (metrics, bitmap, false)
                    }
                } else {
                    (metrics, bitmap, false)
                }
            } else {
                (metrics, bitmap, false)
            }
        } else {
            (metrics, bitmap, true)
        };

        // Last resort: colour emoji font (SBIX/CBDT) for chars not found above.
        if !found {
            self.ensure_emoji_font_loaded();
            let target_px = self.cell_h_px as u32;
            let emoji_result = if let Some(bytes) = self.emoji_font_bytes.as_deref() {
                pack_emoji_glyph(
                    &self.queue,
                    &self.atlas_texture,
                    &mut self.atlas_alloc_x,
                    &mut self.atlas_alloc_y,
                    &mut self.atlas_row_h,
                    bytes,
                    ch,
                    target_px,
                )
            } else {
                None
            };
            if let Some(cached) = emoji_result {
                self.glyph_cache.insert(ch, cached);
                self.font = Some(font);
                return;
            }
        }

        let cached = pack_glyph(
            &self.queue,
            &self.atlas_texture,
            &mut self.atlas_alloc_x,
            &mut self.atlas_alloc_y,
            &mut self.atlas_row_h,
            &final_metrics,
            &final_bitmap,
            cell_h_px,
        );
        self.glyph_cache.insert(ch, cached);
        self.font = Some(font);
    }
}
