/// Monochrome glyph atlas: rasterized character bitmaps for text rendering.
///
/// Manages allocation of a single GL_R8 texture for glyph bitmaps.
/// Tracks entries by (char, style_mask) for efficient lookup and reuse.
/// Works with the types::AtlasGlyph from the painter.

use std::collections::HashMap;

/// Re-export the painter's AtlasGlyph type for convenience.
pub use crate::types::AtlasGlyph;

/// Monochrome glyph atlas allocator.
/// Tracks a linear allocation region in texture space.
/// Uses the painter's types::AtlasGlyph for compatibility.
pub struct GlyphAtlas {
    /// Max texture width/height in pixels.
    pub max_size: u32,
    /// Current allocation position X.
    alloc_x: u32,
    /// Current allocation position Y.
    alloc_y: u32,
    /// Height of current row.
    row_h: u32,
    /// Cache: (char, style_mask) -> AtlasGlyph
    char_cache: HashMap<(char, u8), AtlasGlyph>,
    /// Cache: (glyph_id, style_mask) -> AtlasGlyph (for shaped glyphs)
    glyph_id_cache: HashMap<(u16, u8), AtlasGlyph>,
    /// Stats for debugging
    pub stats: AtlasStats,
}

/// Statistics for atlas behavior.
#[derive(Debug, Clone, Copy, Default)]
pub struct AtlasStats {
    pub entries: usize,
    pub uploads_this_frame: usize,
    pub resets: usize,
}

impl GlyphAtlas {
    /// Create a new glyph atlas with given texture size.
    pub fn new(max_size: u32) -> Self {
        GlyphAtlas {
            max_size,
            alloc_x: 0,
            alloc_y: 0,
            row_h: 0,
            char_cache: HashMap::new(),
            glyph_id_cache: HashMap::new(),
            stats: AtlasStats::default(),
        }
    }

    /// Clear the atlas (called at start of each frame typically).
    pub fn clear(&mut self) {
        self.alloc_x = 0;
        self.alloc_y = 0;
        self.row_h = 0;
        self.char_cache.clear();
        self.glyph_id_cache.clear();
        self.stats.resets += 1;
    }

    /// Check if a character+style is already in the atlas.
    pub fn lookup_char(&self, ch: char, style: u8) -> Option<AtlasGlyph> {
        self.char_cache.get(&(ch, style)).copied()
    }

    /// Check if a glyph_id+style is already in the atlas.
    pub fn lookup_glyph_id(&self, glyph_id: u16, style: u8) -> Option<AtlasGlyph> {
        self.glyph_id_cache.get(&(glyph_id, style)).copied()
    }

    /// Allocate space for a new glyph.
    /// Returns None if the atlas is full.
    pub fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 {
            return None;
        }

        // Check if it fits on the current row
        if self.alloc_x + width <= self.max_size {
            if self.row_h == 0 {
                self.row_h = height;
            }
            let x = self.alloc_x;
            let y = self.alloc_y;
            self.alloc_x += width + 1; // +1 for padding
            return Some((x, y));
        }

        // Start a new row
        self.alloc_x = 0;
        self.alloc_y += self.row_h + 1; // +1 for padding
        self.row_h = height;

        if self.alloc_y + height > self.max_size {
            // Atlas is full
            return None;
        }

        let x = self.alloc_x;
        let y = self.alloc_y;
        self.alloc_x = width + 1; // +1 for padding

        Some((x, y))
    }

    /// Insert a char+style entry into the atlas.
    pub fn insert_char(&mut self, ch: char, style: u8, glyph: AtlasGlyph) {
        self.char_cache.insert((ch, style), glyph);
        self.stats.entries = self.char_cache.len() + self.glyph_id_cache.len();
        self.stats.uploads_this_frame += 1;
    }

    /// Insert a glyph_id+style entry into the atlas.
    pub fn insert_glyph_id(&mut self, glyph_id: u16, style: u8, glyph: AtlasGlyph) {
        self.glyph_id_cache.insert((glyph_id, style), glyph);
        self.stats.entries = self.char_cache.len() + self.glyph_id_cache.len();
        self.stats.uploads_this_frame += 1;
    }

    /// Get current allocation position.
    pub fn alloc_position(&self) -> (u32, u32) {
        (self.alloc_x, self.alloc_y)
    }

    /// Check if atlas is full.
    pub fn is_full(&self) -> bool {
        self.alloc_y + self.row_h + 1 >= self.max_size
    }

    /// Get number of cached entries.
    pub fn entry_count(&self) -> usize {
        self.char_cache.len() + self.glyph_id_cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glyph_atlas_new() {
        let atlas = GlyphAtlas::new(1024);
        assert_eq!(atlas.max_size, 1024);
        assert_eq!(atlas.alloc_x, 0);
        assert_eq!(atlas.alloc_y, 0);
        assert_eq!(atlas.entry_count(), 0);
    }

    #[test]
    fn test_glyph_atlas_allocate_single() {
        let mut atlas = GlyphAtlas::new(1024);
        let (x, y) = atlas.allocate(10, 20).unwrap();

        assert_eq!(x, 0);
        assert_eq!(y, 0);
        assert_eq!(atlas.alloc_x, 11); // 10 + 1 padding
    }

    #[test]
    fn test_glyph_atlas_allocate_multiple_same_row() {
        let mut atlas = GlyphAtlas::new(1024);
        let (x1, y1) = atlas.allocate(10, 20).unwrap();
        let (x2, y2) = atlas.allocate(10, 20).unwrap();

        assert_eq!(x1, 0);
        assert_eq!(y1, 0);
        assert_eq!(x2, 11); // After first glyph + padding
        assert_eq!(y2, 0); // Same row
    }

    #[test]
    fn test_glyph_atlas_allocate_new_row() {
        let mut atlas = GlyphAtlas::new(1024);
        // First allocation
        let (x1, y1) = atlas.allocate(100, 20).unwrap();
        assert_eq!(x1, 0);
        assert_eq!(y1, 0);

        // Fill the rest of the row past the end of texture
        // This won't fit, so next allocation should start new row
        let (x2, y2) = atlas.allocate(1000, 20).unwrap();
        assert_eq!(x2, 0); // Back to beginning
        assert_eq!(y2, 21); // 20 (row_h) + 1 (padding)
    }

    #[test]
    fn test_glyph_atlas_insert_and_lookup_char() {
        let mut atlas = GlyphAtlas::new(1024);
        let glyph = AtlasGlyph {
            tex_x: 10,
            tex_y: 20,
            w: 8,
            h: 16,
            advance_x: 8,
            offset_y: 0,
        };

        atlas.insert_char('A', 0, glyph);
        let retrieved = atlas.lookup_char('A', 0);

        assert_eq!(retrieved, Some(glyph));
    }

    #[test]
    fn test_glyph_atlas_lookup_nonexistent() {
        let atlas = GlyphAtlas::new(1024);
        assert_eq!(atlas.lookup_char('A', 0), None);
    }

    #[test]
    fn test_glyph_atlas_clear() {
        let mut atlas = GlyphAtlas::new(1024);
        let glyph = AtlasGlyph {
            tex_x: 0,
            tex_y: 0,
            w: 8,
            h: 16,
            advance_x: 8,
            offset_y: 0,
        };

        atlas.insert_char('A', 0, glyph);
        assert_eq!(atlas.entry_count(), 1);

        atlas.clear();
        assert_eq!(atlas.entry_count(), 0);
        assert_eq!(atlas.alloc_x, 0);
        assert_eq!(atlas.alloc_y, 0);
    }

    #[test]
    fn test_glyph_atlas_stats() {
        let mut atlas = GlyphAtlas::new(1024);
        let glyph = AtlasGlyph {
            tex_x: 0,
            tex_y: 0,
            w: 8,
            h: 16,
            advance_x: 8,
            offset_y: 0,
        };

        atlas.insert_char('A', 0, glyph);
        assert_eq!(atlas.stats.entries, 1);
        assert_eq!(atlas.stats.uploads_this_frame, 1);

        atlas.clear();
        assert_eq!(atlas.stats.resets, 1);
    }

    #[test]
    fn test_glyph_atlas_allocate_with_zero_size() {
        let mut atlas = GlyphAtlas::new(1024);
        assert_eq!(atlas.allocate(0, 20), None);
        assert_eq!(atlas.allocate(10, 0), None);
    }
}
