use std::cell::Cell;
use std::collections::{HashMap, VecDeque};

use crate::types::FontConfig;

/// One shaped glyph produced by rustybuzz text shaping.
#[derive(Debug, Clone)]
pub(crate) struct ShapedGlyph {
    /// Glyph ID in the font (same ID space as `fontdue::Font::rasterize_indexed`).
    pub glyph_id: u16,
    /// The first source character at this cluster position (for regular char-cache lookup).
    pub source_char: char,
    /// Terminal column where this glyph starts (0-based within the line).
    pub col: usize,
    /// Number of terminal columns spanned (1 = normal, 2+ = ligature).
    pub span_cols: usize,
    /// Character index in the full multi-line text string (for fg_color/style lookups).
    pub full_char_idx: usize,
    /// Sub-pixel x offset from the cell origin, in pixels.
    pub x_offset_px: f32,
    /// Sub-pixel y offset from the baseline, in pixels.
    pub y_offset_px: f32,
}

/// Shape `text` (the whole terminal buffer, `\n`-separated) into per-line glyph sequences.
/// Returns `None` if `font_data` is absent or the face cannot be parsed.
/// Callers should fall back to character-by-character rendering on `None`.
pub(crate) fn shape_terminal_text(
    font_data: &[u8],
    text: &str,
    font_size: f32,
) -> Option<Vec<Vec<ShapedGlyph>>> {
    let face = rustybuzz::Face::from_slice(font_data, 0)?;
    let units_per_em = face.units_per_em() as f32;
    let px_per_unit = font_size / units_per_em;

    let mut result = Vec::new();
    let mut full_char_offset = 0usize;

    for line in text.split('\n') {
        let shaped_line = shape_line(&face, line, full_char_offset, px_per_unit);
        // +1 accounts for the '\n' separator (the last split segment has no trailing '\n',
        // but we still increment to keep full_char_offset consistent with chars()).
        full_char_offset += line.chars().count() + 1;
        result.push(shaped_line);
    }

    Some(result)
}

fn shape_line(
    face: &rustybuzz::Face<'_>,
    line: &str,
    full_char_offset: usize,
    px_per_unit: f32,
) -> Vec<ShapedGlyph> {
    if line.is_empty() {
        return Vec::new();
    }

    // Build byte_offset → (col, source_char) map.
    let byte_to_info: HashMap<u32, (usize, char)> = line
        .char_indices()
        .enumerate()
        .map(|(char_i, (byte_off, ch))| (byte_off as u32, (char_i, ch)))
        .collect();

    let line_char_count = line.chars().count();

    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(line);
    let shaped = rustybuzz::shape(face, &[], buf);

    let infos = shaped.glyph_infos();
    let positions = shaped.glyph_positions();

    let mut result = Vec::with_capacity(infos.len());

    for (i, (info, pos)) in infos.iter().zip(positions.iter()).enumerate() {
        let cluster = info.cluster;
        let &(col, source_char) = match byte_to_info.get(&cluster) {
            Some(v) => v,
            None => continue,
        };

        let span_cols = if i + 1 < infos.len() {
            let next_cluster = infos[i + 1].cluster;
            let next_col = byte_to_info
                .get(&next_cluster)
                .map(|&(c, _)| c)
                .unwrap_or(col + 1);
            (next_col.saturating_sub(col)).max(1)
        } else {
            (line_char_count.saturating_sub(col)).max(1)
        };

        result.push(ShapedGlyph {
            glyph_id: info.glyph_id.min(u16::MAX as u32) as u16,
            source_char,
            col,
            span_cols,
            full_char_idx: full_char_offset + col,
            x_offset_px: pos.x_offset as f32 * px_per_unit,
            y_offset_px: pos.y_offset as f32 * px_per_unit,
        });
    }

    result
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub ch: char,
    pub style: u32,
}

#[derive(Debug, Clone)]
pub struct GlyphEntry {
    pub uv: [f32; 4],
    pub advance: f32,
}

/// LRU-bounded GPU glyph cache.
///
/// Holds rasterised glyphs keyed by `(font, size, codepoint, …)`; on overflow
/// the least-recently-used entry is evicted, and a sustained high miss rate
/// triggers a repack that drops the coldest fraction of entries.
#[derive(Debug)]
pub struct GlyphAtlas {
    capacity: usize,
    entries: HashMap<GlyphKey, GlyphEntry>,
    lru: VecDeque<GlyphKey>,
    lookups_since_repack: Cell<u64>,
    misses_since_repack: Cell<u64>,
}

impl GlyphAtlas {
    pub const DEFAULT_CAPACITY: usize = 4096;
    const REPACK_LOOKUP_WINDOW: u64 = 256;
    const REPACK_MISS_RATE_THRESHOLD: f64 = 0.35;
    const REPACK_DROP_FRACTION: usize = 4;

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::new(),
            lru: VecDeque::new(),
            lookups_since_repack: Cell::new(0),
            misses_since_repack: Cell::new(0),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &GlyphKey) -> Option<&GlyphEntry> {
        self.lookups_since_repack
            .set(self.lookups_since_repack.get().saturating_add(1));
        match self.entries.get(key) {
            Some(entry) => {
                metrics::counter!("atlas_cache_hits").increment(1);
                Some(entry)
            }
            None => {
                self.misses_since_repack
                    .set(self.misses_since_repack.get().saturating_add(1));
                metrics::counter!("atlas_cache_misses").increment(1);
                None
            }
        }
    }

    pub fn insert(&mut self, key: GlyphKey, entry: GlyphEntry) {
        let is_new = self.entries.insert(key.clone(), entry).is_none();
        self.touch_lru(&key);
        if is_new {
            metrics::counter!("atlas_glyphs").increment(1);
            self.evict_to_capacity();
        }
        self.repack_if_needed();
    }

    fn touch_lru(&mut self, key: &GlyphKey) {
        if let Some(pos) = self.lru.iter().position(|k| k == key) {
            let _ = self.lru.remove(pos);
        }
        self.lru.push_back(key.clone());
    }

    fn evict_to_capacity(&mut self) {
        while self.entries.len() > self.capacity {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if self.entries.remove(&oldest).is_some() {
                metrics::counter!("atlas_evictions").increment(1);
            }
        }
    }

    fn repack_if_needed(&mut self) {
        let lookups = self.lookups_since_repack.get();
        if lookups < Self::REPACK_LOOKUP_WINDOW {
            return;
        }
        let misses = self.misses_since_repack.get();
        let miss_rate = misses as f64 / lookups as f64;
        self.lookups_since_repack.set(0);
        self.misses_since_repack.set(0);
        if miss_rate <= Self::REPACK_MISS_RATE_THRESHOLD || self.entries.len() < 2 {
            return;
        }

        let drop_count = (self.entries.len() / Self::REPACK_DROP_FRACTION).max(1);
        let mut dropped = 0u64;
        for _ in 0..drop_count {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if self.entries.remove(&oldest).is_some() {
                dropped += 1;
            }
        }
        if dropped > 0 {
            metrics::counter!("atlas_repacks").increment(1);
            metrics::counter!("atlas_repack_dropped").increment(dropped);
        }
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::{GlyphAtlas, GlyphEntry, GlyphKey};

    #[test]
    fn glyph_atlas_roundtrip() {
        let mut atlas = GlyphAtlas::default();
        let key = GlyphKey { ch: 'a', style: 1 };
        let entry = GlyphEntry {
            uv: [0.1, 0.2, 0.3, 0.4],
            advance: 1.5,
        };

        atlas.insert(key.clone(), entry.clone());

        let stored = atlas.get(&key).expect("glyph entry stored");
        assert_eq!(stored.uv, entry.uv);
        assert_eq!(stored.advance, entry.advance);
    }

    #[test]
    fn atlas_evicts_oldest_when_capacity_reached() {
        let mut atlas = GlyphAtlas::with_capacity(2);
        atlas.insert(
            GlyphKey { ch: 'a', style: 0 },
            GlyphEntry {
                uv: [0.0, 0.0, 0.1, 0.1],
                advance: 1.0,
            },
        );
        atlas.insert(
            GlyphKey { ch: 'b', style: 0 },
            GlyphEntry {
                uv: [0.1, 0.0, 0.2, 0.1],
                advance: 1.0,
            },
        );
        atlas.insert(
            GlyphKey { ch: 'c', style: 0 },
            GlyphEntry {
                uv: [0.2, 0.0, 0.3, 0.1],
                advance: 1.0,
            },
        );

        assert_eq!(atlas.len(), 2);
        assert!(atlas.get(&GlyphKey { ch: 'a', style: 0 }).is_none());
        assert!(atlas.get(&GlyphKey { ch: 'b', style: 0 }).is_some());
        assert!(atlas.get(&GlyphKey { ch: 'c', style: 0 }).is_some());
    }

    #[test]
    fn atlas_repack_drops_cold_entries_after_sustained_miss_rate() {
        let mut atlas = GlyphAtlas::with_capacity(8);
        for ch in ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'] {
            atlas.insert(
                GlyphKey { ch, style: 0 },
                GlyphEntry {
                    uv: [0.0, 0.0, 0.0, 0.0],
                    advance: 1.0,
                },
            );
        }

        for i in 0..GlyphAtlas::REPACK_LOOKUP_WINDOW {
            let hit = i % 4 == 0;
            let key = if hit {
                GlyphKey { ch: 'h', style: 0 }
            } else {
                GlyphKey {
                    ch: (b'i' + (i % 8) as u8) as char,
                    style: 0,
                }
            };
            let _ = atlas.get(&key);
        }

        let before = atlas.len();
        atlas.insert(
            GlyphKey { ch: 'z', style: 0 },
            GlyphEntry {
                uv: [0.9, 0.9, 1.0, 1.0],
                advance: 1.0,
            },
        );

        assert!(atlas.len() < before + 1);
    }
}

/// One rasterized glyph packed into the atlas texture.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CachedGlyph {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub offset_x_px: f32,
    pub offset_y_px: f32,
    pub width_px: f32,
    pub height_px: f32,
    /// `true` for colour glyphs (emoji): the atlas stores full RGBA pixels;
    /// the shader renders them directly instead of tinting with the fg colour.
    pub is_color: bool,
}

/// Monospace font families tried in order when the user's configured family is
/// unavailable or unspecified.
const MONOSPACE_FONT_FAMILIES: &[&str] = &[
    "Hack",
    "DejaVu Sans Mono",
    "Consolas",
    "Courier New",
    "Menlo",
];

/// Builds a `fontdb::Database` populated with the system fonts.
///
/// Scanning the system font directories is expensive (it touches hundreds of
/// files on macOS) and the resulting database also keeps file metadata in
/// memory.  Callers should build this **once** at startup, reuse it for every
/// font query, and drop it before entering the render loop so the backing
/// mmaps/handles can be released.
pub(crate) fn load_system_font_database() -> fontdb::Database {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    db
}

/// Tries to load raw font bytes from the configured family or system fallbacks.
pub(crate) fn load_font_bytes(db: &fontdb::Database, config: &FontConfig) -> Option<Vec<u8>> {
    fn query_bytes_by_family(db: &fontdb::Database, family: &str) -> Option<Vec<u8>> {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..fontdb::Query::default()
        };
        let id = db.query(&query)?;
        db.with_face_data(id, |data, _| data.to_vec())
    }

    fn query_monospace_bytes(db: &fontdb::Database) -> Option<Vec<u8>> {
        let query = fontdb::Query {
            families: &[fontdb::Family::Monospace],
            ..fontdb::Query::default()
        };
        let id = db.query(&query)?;
        db.with_face_data(id, |data, _| data.to_vec())
    }

    if let Some(ref family) = config.font_family {
        if let Some(bytes) = query_bytes_by_family(db, family) {
            return Some(bytes);
        }
        tracing::warn!(family = %family, "cannot load font family, trying fallback");
    }

    for &family in MONOSPACE_FONT_FAMILIES {
        if let Some(bytes) = query_bytes_by_family(db, family) {
            return Some(bytes);
        }
    }

    if let Some(bytes) = query_monospace_bytes(db) {
        return Some(bytes);
    }

    tracing::error!("no system font found — text will not be rendered");
    None
}

/// Tries to load the **bold** variant of the configured font family (or system fallbacks).
/// Falls back gracefully so callers can treat `None` as "use regular font for bold".
pub(crate) fn load_bold_font_bytes(db: &fontdb::Database, config: &FontConfig) -> Option<Vec<u8>> {
    fn query_bold_bytes(db: &fontdb::Database, family: &str) -> Option<Vec<u8>> {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            weight: fontdb::Weight::BOLD,
            ..fontdb::Query::default()
        };
        let id = db.query(&query)?;
        db.with_face_data(id, |data, _| data.to_vec())
    }

    fn query_bold_monospace_bytes(db: &fontdb::Database) -> Option<Vec<u8>> {
        let query = fontdb::Query {
            families: &[fontdb::Family::Monospace],
            weight: fontdb::Weight::BOLD,
            ..fontdb::Query::default()
        };
        let id = db.query(&query)?;
        db.with_face_data(id, |data, _| data.to_vec())
    }

    if let Some(ref family) = config.font_family
        && let Some(bytes) = query_bold_bytes(db, family)
    {
        return Some(bytes);
    }

    for &family in MONOSPACE_FONT_FAMILIES {
        if let Some(bytes) = query_bold_bytes(db, family) {
            return Some(bytes);
        }
    }

    query_bold_monospace_bytes(db)
}

/// Load all available Unicode fallback fonts in priority order.
///
/// Multiple fonts are needed because no single system font covers every
/// Unicode block a terminal might render.  For example, on macOS:
///   - "Apple Symbols" covers box-drawing, math, and misc symbols
///   - "Apple Braille" covers Braille Patterns (U+2800–U+28FF), used by
///     CLI spinners (including Claude Code's ⠸⠴⠦ spinner)
///   - "Zapf Dingbats" covers Dingbats (U+2700–U+27BF) including ❯ (U+276F)
///   - "Menlo" covers a broad range including many chars missing from symbol fonts
///   - "Arial Unicode MS" covers a very wide range as a catch-all
pub(crate) fn load_unicode_fallback_fonts(db: &fontdb::Database) -> Vec<fontdue::Font> {
    fn query_bytes(db: &fontdb::Database, family: &str) -> Option<Vec<u8>> {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..fontdb::Query::default()
        };
        let id = db.query(&query)?;
        db.with_face_data(id, |data, _| data.to_vec())
    }

    fn load_from_path(path: &str) -> Option<fontdue::Font> {
        let bytes = std::fs::read(path).ok()?;
        fontdue::Font::from_bytes(bytes.as_slice(), fontdue::FontSettings::default()).ok()
    }

    let families: &[&str] = &[
        "Apple Symbols",    // macOS – broad symbol coverage (math, misc symbols)
        "Apple Braille",    // macOS – Braille Patterns (U+2800–U+28FF)
        "Zapf Dingbats",    // macOS – Dingbats block (U+2700–U+27BF), includes ❯
        "Menlo",            // macOS – broad monospace coverage including many symbols
        "Arial Unicode MS", // macOS/Windows – very wide Unicode range
        "Segoe UI Symbol",  // Windows – broad symbol coverage
        "Noto Sans",        // Linux (if installed)
        "DejaVu Sans",      // Linux fallback
        "FreeSans",         // another Linux option
    ];

    let mut loaded_families = std::collections::HashSet::new();
    let mut fonts: Vec<fontdue::Font> = families
        .iter()
        .filter_map(|family| {
            let bytes = query_bytes(db, family)?;
            let font =
                fontdue::Font::from_bytes(bytes.as_slice(), fontdue::FontSettings::default())
                    .ok()?;
            loaded_families.insert(*family);
            Some(font)
        })
        .collect();

    // macOS direct-path fallbacks for fonts that fontdb might miss.
    // These are essential because some macOS font locations are not scanned by fontdb,
    // and Apple's font naming conventions can vary across OS versions.
    #[cfg(target_os = "macos")]
    {
        // ZapfDingbats covers Dingbats U+2700–U+27BF, including ❯ (U+276F)
        // which is widely used in shell prompts (fish, starship, oh-my-zsh).
        if !loaded_families.contains("Zapf Dingbats")
            && let Some(font) = load_from_path("/System/Library/Fonts/ZapfDingbats.ttf")
        {
            fonts.push(font);
        }
        // Menlo is always present on macOS and covers many Unicode ranges including
        // Dingbats, box drawing, and misc symbols that the dedicated symbol fonts miss.
        if !loaded_families.contains("Menlo")
            && let Some(font) = load_from_path("/System/Library/Fonts/Menlo.ttc")
        {
            fonts.push(font);
        }
    }

    fonts
}

pub(crate) const TEXT_ATLAS_SIZE: u32 = 1024;

/// Rasterizes one glyph into the atlas and returns its cached descriptor.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pack_glyph(
    queue: &wgpu::Queue,
    atlas_texture: &wgpu::Texture,
    alloc_x: &mut u32,
    alloc_y: &mut u32,
    row_h: &mut u32,
    metrics: &fontdue::Metrics,
    bitmap: &[u8],
    cell_h_px: f32,
) -> CachedGlyph {
    let gw = metrics.width as u32;
    let gh = metrics.height as u32;
    if gw == 0 || gh == 0 || bitmap.is_empty() {
        return CachedGlyph::default();
    }
    if *alloc_x + gw + 1 > TEXT_ATLAS_SIZE {
        *alloc_y += *row_h + 1;
        *alloc_x = 0;
        *row_h = 0;
    }
    if *alloc_y + gh + 1 > TEXT_ATLAS_SIZE {
        tracing::warn!("glyph atlas full");
        return CachedGlyph::default();
    }
    let dest_x = *alloc_x;
    let dest_y = *alloc_y;
    // Convert coverage (1 byte/px) to RGBA8 (white with coverage in alpha).
    let rgba: Vec<u8> = bitmap
        .iter()
        .flat_map(|&cov| [255u8, 255, 255, cov])
        .collect();
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: atlas_texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: dest_x,
                y: dest_y,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(gw * 4),
            rows_per_image: Some(gh),
        },
        wgpu::Extent3d {
            width: gw,
            height: gh,
            depth_or_array_layers: 1,
        },
    );
    *row_h = (*row_h).max(gh);
    *alloc_x += gw + 1;
    let af = TEXT_ATLAS_SIZE as f32;
    let u0 = dest_x as f32 / af;
    let v0 = dest_y as f32 / af;
    let u1 = (dest_x + gw) as f32 / af;
    let v1 = (dest_y + gh) as f32 / af;
    let baseline_y = cell_h_px * 0.80;
    let glyph_ascent = metrics.height as f32 + metrics.ymin as f32;
    let offset_y_px = (baseline_y - glyph_ascent).max(-2.0);
    let offset_x_px = metrics.xmin as f32;
    CachedGlyph {
        u0,
        v0,
        u1,
        v1,
        offset_x_px,
        offset_y_px,
        width_px: gw as f32,
        height_px: gh as f32,
        is_color: false,
    }
}

/// Writes a pre-decoded RGBA8 image (e.g. a colour emoji) into the atlas.
/// `rgba_pixels` must be exactly `glyph_w * glyph_h * 4` bytes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pack_color_glyph(
    queue: &wgpu::Queue,
    atlas_texture: &wgpu::Texture,
    alloc_x: &mut u32,
    alloc_y: &mut u32,
    row_h: &mut u32,
    rgba_pixels: &[u8],
    glyph_w: u32,
    glyph_h: u32,
) -> CachedGlyph {
    if glyph_w == 0 || glyph_h == 0 || rgba_pixels.is_empty() {
        return CachedGlyph::default();
    }
    if *alloc_x + glyph_w + 1 > TEXT_ATLAS_SIZE {
        *alloc_y += *row_h + 1;
        *alloc_x = 0;
        *row_h = 0;
    }
    if *alloc_y + glyph_h + 1 > TEXT_ATLAS_SIZE {
        tracing::warn!("glyph atlas full (colour glyph)");
        return CachedGlyph::default();
    }
    let dest_x = *alloc_x;
    let dest_y = *alloc_y;
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: atlas_texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: dest_x,
                y: dest_y,
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        rgba_pixels,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(glyph_w * 4),
            rows_per_image: Some(glyph_h),
        },
        wgpu::Extent3d {
            width: glyph_w,
            height: glyph_h,
            depth_or_array_layers: 1,
        },
    );
    *row_h = (*row_h).max(glyph_h);
    *alloc_x += glyph_w + 1;
    let af = TEXT_ATLAS_SIZE as f32;
    let u0 = dest_x as f32 / af;
    let v0 = dest_y as f32 / af;
    let u1 = (dest_x + glyph_w) as f32 / af;
    let v1 = (dest_y + glyph_h) as f32 / af;
    CachedGlyph {
        u0,
        v0,
        u1,
        v1,
        offset_x_px: 0.0,
        offset_y_px: 0.0,
        width_px: glyph_w as f32,
        height_px: glyph_h as f32,
        is_color: true,
    }
}

/// Tries to load the bytes of a colour emoji font from the system.
/// On macOS this is Apple Color Emoji; on Linux Noto Color Emoji; on Windows Segoe UI Emoji.
///
/// Emoji fonts are **large** (Apple Color Emoji is ~180 MB) so callers should
/// only invoke this lazily, the first time an emoji glyph actually needs to be
/// rasterised.  Pass `None` to skip the (slow) system font scan and only try
/// the platform-specific direct path fallback.
pub(crate) fn load_emoji_font_bytes(db: Option<&fontdb::Database>) -> Option<Vec<u8>> {
    fn query_bytes(db: &fontdb::Database, family: &str) -> Option<Vec<u8>> {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(family)],
            ..fontdb::Query::default()
        };
        let id = db.query(&query)?;
        db.with_face_data(id, |data, _| data.to_vec())
    }

    if let Some(db) = db {
        for family in ["Apple Color Emoji", "Noto Color Emoji", "Segoe UI Emoji"] {
            if let Some(bytes) = query_bytes(db, family) {
                return Some(bytes);
            }
        }
    }

    // macOS direct-path fallback: fast (single `read`) and reliable even when
    // the caller doesn't have a `fontdb::Database` handy.
    #[cfg(target_os = "macos")]
    {
        let paths = [
            "/System/Library/Fonts/Apple Color Emoji.ttc",
            "/System/Library/Fonts/Apple Color Emoji.ttf",
        ];
        for path in &paths {
            if let Ok(bytes) = std::fs::read(path) {
                return Some(bytes);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let paths = [
            "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
            "/usr/share/fonts/noto/NotoColorEmoji.ttf",
            "/usr/local/share/fonts/NotoColorEmoji.ttf",
            "/usr/share/fonts/google-noto-color-emoji/NotoColorEmoji.ttf",
        ];
        for path in &paths {
            if let Ok(bytes) = std::fs::read(path) {
                return Some(bytes);
            }
        }
    }

    None
}

/// Extracts an emoji glyph from a colour font via the SBIX/CBDT raster image tables,
/// decodes the embedded PNG/JPEG, scales it to `target_px × target_px`, and writes
/// it into the atlas as RGBA8.  Returns `None` if the glyph cannot be found or decoded.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pack_emoji_glyph(
    queue: &wgpu::Queue,
    atlas_texture: &wgpu::Texture,
    alloc_x: &mut u32,
    alloc_y: &mut u32,
    row_h: &mut u32,
    emoji_font_bytes: &[u8],
    ch: char,
    target_px: u32,
) -> Option<CachedGlyph> {
    let face = ttf_parser::Face::parse(emoji_font_bytes, 0).ok()?;
    let glyph_id = face.glyph_index(ch)?;
    // Use u16::MAX to request the largest available strike; the API returns the
    // strike whose ppem is closest to (and not below) target_px.
    let raster = face.glyph_raster_image(glyph_id, target_px.min(u16::MAX as u32) as u16)?;
    let img = image::load_from_memory(raster.data).ok()?;
    // Scale to target_px (emoji are square; resize preserves aspect ratio).
    let resized = img.resize(target_px, target_px, image::imageops::FilterType::Triangle);
    let rgba8 = resized.to_rgba8();
    let (w, h) = rgba8.dimensions();
    Some(pack_color_glyph(
        queue,
        atlas_texture,
        alloc_x,
        alloc_y,
        row_h,
        rgba8.as_raw(),
        w,
        h,
    ))
}
