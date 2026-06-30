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
            self.row_h = self.row_h.max(height);
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

    // /// Get current allocation position.
    // pub fn alloc_position(&self) -> (u32, u32) {
    //     (self.alloc_x, self.alloc_y)
    // }

    // /// Check if atlas is full.
    // pub fn is_full(&self) -> bool {
    //     self.alloc_y + self.row_h + 1 >= self.max_size
    // }

    // /// Get number of cached entries.
    // pub fn entry_count(&self) -> usize {
    //     self.char_cache.len()
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_row_height_tracks_tallest_image_in_row() {
        let mut atlas = ColorAtlas::new(10);

        assert_eq!(atlas.allocate(2, 2), Some((0, 0)));
        assert_eq!(atlas.allocate(2, 6), Some((3, 0)));
        assert_eq!(atlas.allocate(10, 1), Some((0, 7)));
    }
}
