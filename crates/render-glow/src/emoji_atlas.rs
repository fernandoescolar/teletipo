/// Color emoji atlas: RGBA emoji bitmaps (SBIX / CBDT strikes).
///
/// Manages allocation of a single GL_RGBA texture for color emoji bitmaps.
/// Tracks entries by character for efficient lookup and reuse.
/// Works with the types::ColorAtlasEntry from the painter.

use std::collections::HashMap;

/// Re-export the painter's ColorAtlasEntry type for convenience.
pub use crate::types::ColorAtlasEntry;

/// Color emoji atlas allocator.
/// Tracks a linear allocation region in RGBA texture space.
/// Uses the painter's types::ColorAtlasEntry for compatibility.
pub struct ColorAtlas {
    /// Max texture width/height in pixels.
    pub max_size: u32,
    /// Current allocation position X.
    alloc_x: u32,
    /// Current allocation position Y.
    alloc_y: u32,
    /// Height of current row.
    row_h: u32,
    /// Cache: char -> ColorAtlasEntry
    char_cache: HashMap<char, ColorAtlasEntry>,
    /// Stats for debugging
    pub stats: ColorAtlasStats,
}

/// Statistics for color atlas behavior.
#[derive(Debug, Clone, Copy, Default)]
pub struct ColorAtlasStats {
    pub entries: usize,
    pub uploads_this_frame: usize,
    pub resets: usize,
}

impl ColorAtlas {
    /// Create a new color emoji atlas with given texture size.
    pub fn new(max_size: u32) -> Self {
        ColorAtlas {
            max_size,
            alloc_x: 0,
            alloc_y: 0,
            row_h: 0,
            char_cache: HashMap::new(),
            stats: ColorAtlasStats::default(),
        }
    }

    /// Clear the atlas (called at start of each frame typically).
    pub fn clear(&mut self) {
        self.alloc_x = 0;
        self.alloc_y = 0;
        self.row_h = 0;
        self.char_cache.clear();
        self.stats.resets += 1;
    }

    /// Check if a character is already in the atlas.
    pub fn lookup(&self, ch: char) -> Option<ColorAtlasEntry> {
        self.char_cache.get(&ch).copied()
    }

    /// Allocate space for a new emoji.
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

    /// Insert a character entry into the atlas.
    pub fn insert(&mut self, ch: char, entry: ColorAtlasEntry) {
        self.char_cache.insert(ch, entry);
        self.stats.entries = self.char_cache.len();
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
        self.char_cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_atlas_new() {
        let atlas = ColorAtlas::new(2048);
        assert_eq!(atlas.max_size, 2048);
        assert_eq!(atlas.alloc_x, 0);
        assert_eq!(atlas.alloc_y, 0);
        assert_eq!(atlas.entry_count(), 0);
    }

    #[test]
    fn test_color_atlas_allocate_single() {
        let mut atlas = ColorAtlas::new(2048);
        let (x, y) = atlas.allocate(32, 32).unwrap();

        assert_eq!(x, 0);
        assert_eq!(y, 0);
        assert_eq!(atlas.alloc_x, 33); // 32 + 1 padding
    }

    #[test]
    fn test_color_atlas_allocate_multiple_same_row() {
        let mut atlas = ColorAtlas::new(2048);
        let (x1, y1) = atlas.allocate(32, 32).unwrap();
        let (x2, y2) = atlas.allocate(32, 32).unwrap();

        assert_eq!(x1, 0);
        assert_eq!(y1, 0);
        assert_eq!(x2, 33); // After first + padding
        assert_eq!(y2, 0); // Same row
    }

    #[test]
    fn test_color_atlas_allocate_new_row() {
        let mut atlas = ColorAtlas::new(2048);
        // First allocation
        let (x1, y1) = atlas.allocate(100, 32).unwrap();
        assert_eq!(x1, 0);
        assert_eq!(y1, 0);

        // Second allocation that doesn't fit in first row
        let (x2, y2) = atlas.allocate(2000, 32).unwrap();
        assert_eq!(x2, 0); // Back to beginning
        assert_eq!(y2, 33); // 32 + 1 (padding)
    }

    #[test]
    fn test_color_atlas_insert_and_lookup() {
        let mut atlas = ColorAtlas::new(2048);
        let entry = ColorAtlasEntry {
            tex_x: 100,
            tex_y: 200,
            w: 32,
            h: 32,
            advance_x: 32,
            offset_y: 0,
        };

        atlas.insert('😀', entry);
        let retrieved = atlas.lookup('😀');

        assert_eq!(retrieved, Some(entry));
    }

    #[test]
    fn test_color_atlas_lookup_nonexistent() {
        let atlas = ColorAtlas::new(2048);
        assert_eq!(atlas.lookup('😀'), None);
    }

    #[test]
    fn test_color_atlas_clear() {
        let mut atlas = ColorAtlas::new(2048);
        let entry = ColorAtlasEntry {
            tex_x: 0,
            tex_y: 0,
            w: 32,
            h: 32,
            advance_x: 32,
            offset_y: 0,
        };

        atlas.insert('😀', entry);
        assert_eq!(atlas.entry_count(), 1);

        atlas.clear();
        assert_eq!(atlas.entry_count(), 0);
        assert_eq!(atlas.alloc_x, 0);
        assert_eq!(atlas.alloc_y, 0);
    }

    #[test]
    fn test_color_atlas_is_full() {
        let mut atlas = ColorAtlas::new(64); // Very small for testing

        // Allocate most of the space
        let _ = atlas.allocate(64, 32);
        assert!(!atlas.is_full());

        // Next row won't fit
        let _ = atlas.allocate(64, 32); // Fills up
        assert!(atlas.is_full());
    }

    #[test]
    fn test_color_atlas_stats() {
        let mut atlas = ColorAtlas::new(2048);
        let entry = ColorAtlasEntry {
            tex_x: 0,
            tex_y: 0,
            w: 32,
            h: 32,
            advance_x: 32,
            offset_y: 0,
        };

        atlas.insert('😀', entry);
        assert_eq!(atlas.stats.entries, 1);
        assert_eq!(atlas.stats.uploads_this_frame, 1);

        atlas.clear();
        assert_eq!(atlas.stats.resets, 1);
    }
}
