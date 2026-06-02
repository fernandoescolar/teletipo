use std::collections::HashSet;
use std::path::Path;

use image::RgbaImage;
use memmap2::Mmap;

/// Rasterises color emoji from bitmap-strike font tables.
///
/// Supported formats (via `ttf_parser`):
/// - **SBIX** – Apple Color Emoji (macOS)
/// - **CBDT / CBLC** – Noto Color Emoji (Linux/Android)
/// - **PNG-embedded SBIX strikes** – Windows / other vendors
///
/// Falls back gracefully to `None` when no bitmap strike exists for the
/// requested character (e.g. regular text glyphs, outline-only emoji fonts).
///
/// # Memory notes
///
/// The font file is memory-mapped rather than copied into the heap.
/// On macOS, Apple Color Emoji.ttc is ~183 MB; with mmap the OS pages in only
/// the PNG data for glyphs that are actually rendered, keeping resident memory
/// proportional to the number of distinct emoji used.
///
/// Decoded `RgbaImage` bitmaps are *not* cached here — they are uploaded to
/// the GPU atlas immediately after decoding and then dropped.  A `HashSet` of
/// `(char, ppem)` pairs is kept only to avoid repeatedly re-attempting failed
/// decodes for characters that have no bitmap strike.
pub(crate) struct ColorEmojiRasterizer {
    mmap: Mmap,
    face_index: u32,
    /// Characters known to have no bitmap at the current ppem.
    /// Prevents re-attempting expensive decode failures every frame.
    missing: HashSet<(char, u16)>,
}

impl ColorEmojiRasterizer {
    /// Open the font at `path` and set up a memory-mapped view.
    /// Returns `None` if the file cannot be opened or mapped.
    pub(crate) fn new(path: &Path, face_index: u32) -> Option<Self> {
        let file = std::fs::File::open(path).ok()?;
        // SAFETY: we treat the mapping as read-only and never modify the file
        // while the process is running.  The font file is a system resource
        // that is not expected to change.
        let mmap = unsafe { Mmap::map(&file) }.ok()?;
        Some(Self {
            mmap,
            face_index,
            missing: HashSet::new(),
        })
    }

    /// Decode and return a color bitmap for `ch` at `size_px`, or `None`.
    ///
    /// On success the caller should upload the image to the GPU atlas and
    /// then drop it — this function does not cache decoded images.
    pub(crate) fn rasterize(&mut self, ch: char, size_px: f32) -> Option<RgbaImage> {
        let ppem = size_px.round() as u16;
        if self.missing.contains(&(ch, ppem)) {
            return None;
        }
        let result = decode(&self.mmap, self.face_index, ch, ppem);
        if result.is_none() {
            self.missing.insert((ch, ppem));
        }
        result
    }

    /// Wipe the missing-char set (call when font size changes).
    pub(crate) fn clear_cache(&mut self) {
        self.missing.clear();
    }
}

// ── Internal decoding helper ──────────────────────────────────────────────────

fn decode(data: &[u8], face_index: u32, ch: char, ppem: u16) -> Option<RgbaImage> {
    let face = ttf_parser::Face::parse(data, face_index).ok()?;

    // Map Unicode scalar to glyph id inside this face.
    let glyph_id = face.glyph_index(ch)?;

    // Ask ttf-parser for the nearest bitmap strike ≥ ppem.
    // This covers both SBIX (Apple Color Emoji) and CBDT/CBLC (Noto Color Emoji).
    let raster = face.glyph_raster_image(glyph_id, ppem)?;

    // The data is almost always a self-contained PNG (occasionally JPEG).
    let img = image::load_from_memory(raster.data).ok()?.into_rgba8();

    if img.width() == 0 || img.height() == 0 {
        return None;
    }
    Some(img)
}
