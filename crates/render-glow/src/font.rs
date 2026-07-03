use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use fontdb::{Family, Query, Weight};

use crate::emoji::ColorEmojiRasterizer;
use crate::types::{
    FontPathSource, FontSource, GlyphBitmap, STYLE_BOLD, STYLE_ITALIC, ShapedGlyph,
};

// ── CPU font rasterizer ───────────────────────────────────────────────────────

pub(crate) struct CpuFontRasterizer {
    pub(crate) font_size_px: f32,
    pub(crate) primary_font: Option<fontdue::Font>,
    pub(crate) primary_font_source: Option<FontSource>,
    /// Ordered list of Unicode symbol fallback font paths (lazy-loaded).
    /// Uses specific fonts (Apple Symbols, ZapfDingbats, Arial Unicode MS, …)
    /// rather than the generic SansSerif family, which on macOS resolves to
    /// San Francisco and returns stub box glyphs for many Unicode ranges.
    unicode_fallback_paths: Vec<FontPathSource>,
    /// Cached loaded fallback fonts (lazily populated from unicode_fallback_paths).
    unicode_fallback_fonts_cache: RefCell<Vec<Option<fontdue::Font>>>,
    /// Outline emoji font path (lazy-loaded, e.g. Noto Emoji).
    emoji_font_path: Option<FontPathSource>,
    /// Cached loaded emoji font (lazily populated from emoji_font_path).
    emoji_font_cache: RefCell<Option<Option<fontdue::Font>>>,
    /// File path and face index for color emoji font (lazy-opened).
    color_emoji_source: Option<(PathBuf, u32)>,
    /// Color emoji rasterizer backed by SBIX/CBDT bitmap strikes.
    /// Created on-demand the first time a color emoji is actually needed.
    pub(crate) color_rasterizer: Option<ColorEmojiRasterizer>,
    glyph_cache: HashMap<(char, u8), GlyphBitmap>,
    glyph_cache_order: VecDeque<(char, u8)>,
    shaped_glyph_cache: HashMap<(u16, u8), GlyphBitmap>,
    shaped_glyph_cache_order: VecDeque<(u16, u8)>,
}

const GLYPH_CACHE_LIMIT: usize = 4096;
const SHAPED_GLYPH_CACHE_LIMIT: usize = 4096;

impl CpuFontRasterizer {
    pub(crate) fn new(family: Option<String>, font_size_px: f32) -> Self {
        let (
            primary_font,
            primary_font_source,
            unicode_fallback_paths,
            emoji_font_path,
            emoji_source,
        ) = load_fonts_for_family(family.as_deref());
        let num_fallbacks = unicode_fallback_paths.len();
        Self {
            font_size_px,
            primary_font,
            primary_font_source,
            unicode_fallback_paths,
            unicode_fallback_fonts_cache: RefCell::new(vec![None; num_fallbacks]),
            emoji_font_path,
            emoji_font_cache: RefCell::new(None),
            color_emoji_source: emoji_source,
            color_rasterizer: None,
            glyph_cache: HashMap::new(),
            glyph_cache_order: VecDeque::new(),
            shaped_glyph_cache: HashMap::new(),
            shaped_glyph_cache_order: VecDeque::new(),
        }
    }

    pub(crate) fn set_font_size(&mut self, font_size_px: f32) {
        if (self.font_size_px - font_size_px).abs() < 0.5 {
            return;
        }
        self.font_size_px = font_size_px;
        self.glyph_cache.clear();
        self.glyph_cache_order.clear();
        self.shaped_glyph_cache.clear();
        self.shaped_glyph_cache_order.clear();
        if let Some(cr) = self.color_rasterizer.as_mut() {
            cr.clear_cache();
        }
    }

    /// Try to rasterise `ch` as a color RGBA bitmap from the emoji font's
    /// bitmap-strike tables (SBIX / CBDT).  Returns `None` when no strike is
    /// available — the caller should fall back to the grayscale outline path.
    pub(crate) fn color_rasterize(&mut self, ch: char) -> Option<image::RgbaImage> {
        if self.color_rasterizer.is_none()
            && let Some((path, face_index)) = self.color_emoji_source.as_ref()
        {
            self.color_rasterizer = ColorEmojiRasterizer::new(path, *face_index);
        }
        self.color_rasterizer
            .as_mut()?
            .rasterize(ch, self.font_size_px)
    }

    pub(crate) fn cell_metrics(&self) -> (f32, f32) {
        let size = self.font_size_px.max(1.0);
        if let Some(font) = self.primary_font.as_ref() {
            let w = font.metrics('M', size).advance_width.max(1.0);
            return (w, (size * 1.2).max(1.0));
        }
        ((size * 0.62).max(1.0), (size * 1.30).max(1.0))
    }

    pub(crate) fn glyph(&mut self, ch: char, style: u8) -> Option<GlyphBitmap> {
        // Whitespace characters must always render as blank. Some fonts carry
        // visible glyphs for U+00A0 and other Unicode spaces (editor-style
        // "show invisible characters" markers) that must not appear in a terminal.
        if ch.is_whitespace() {
            return None;
        }

        let style_key = style & (STYLE_BOLD | STYLE_ITALIC);
        if let Some(g) = self.glyph_cache.get(&(ch, style_key)) {
            return Some(g.clone());
        }

        let font_size = self.font_size_px.max(1.0);
        let (metrics, bitmap) = self.rasterize_char(ch, font_size)?;
        if metrics.width == 0 || metrics.height == 0 {
            return None;
        }

        let glyph = GlyphBitmap {
            width: metrics.width,
            height: metrics.height,
            xmin: metrics.xmin as f32,
            ymin: metrics.ymin as f32,
            advance_width: metrics.advance_width,
            alpha: bitmap,
        };
        self.insert_glyph_cache((ch, style_key), glyph.clone());
        Some(glyph)
    }

    fn rasterize_char(&self, ch: char, font_size_px: f32) -> Option<(fontdue::Metrics, Vec<u8>)> {
        if let Some(font) = self.primary_font.as_ref()
            && font.lookup_glyph_index(ch) != 0
        {
            return Some(font.rasterize(ch, font_size_px));
        }

        // Check fallback fonts (loaded lazily one-by-one).
        for i in 0..self.unicode_fallback_paths.len() {
            self.ensure_fallback_font_loaded(i);
            let fallback_cache = self.unicode_fallback_fonts_cache.borrow();
            if let Some(font) = fallback_cache.get(i).and_then(Option::as_ref)
                && font.lookup_glyph_index(ch) != 0
            {
                return Some(font.rasterize(ch, font_size_px));
            }
        }

        // Check emoji font
        self.ensure_emoji_font_loaded();
        let emoji_cache = self.emoji_font_cache.borrow();
        if let Some(Some(font)) = emoji_cache.as_ref()
            && font.lookup_glyph_index(ch) != 0
        {
            return Some(font.rasterize(ch, font_size_px));
        }

        None
    }

    fn ensure_fallback_font_loaded(&self, index: usize) {
        let mut cache = self.unicode_fallback_fonts_cache.borrow_mut();
        let Some(font_opt) = cache.get_mut(index) else {
            return;
        };
        if font_opt.is_none()
            && let Some(path_source) = self.unicode_fallback_paths.get(index)
            && let Ok(data) = std::fs::read(&path_source.path)
            && let Ok(font) = fontdue::Font::from_bytes(
                &data[..],
                fontdue::FontSettings {
                    collection_index: path_source.face_index,
                    ..fontdue::FontSettings::default()
                },
            )
        {
            *font_opt = Some(font);
        }
    }

    fn ensure_emoji_font_loaded(&self) {
        let mut cache = self.emoji_font_cache.borrow_mut();
        if cache.is_none() {
            if let Some(path_source) = &self.emoji_font_path {
                if let Ok(data) = std::fs::read(&path_source.path) {
                    if let Ok(font) = fontdue::Font::from_bytes(
                        &data[..],
                        fontdue::FontSettings {
                            collection_index: path_source.face_index,
                            ..fontdue::FontSettings::default()
                        },
                    ) {
                        *cache = Some(Some(font));
                    } else {
                        *cache = Some(None);
                    }
                } else {
                    *cache = Some(None);
                }
            } else {
                *cache = Some(None);
            }
        }
    }

    pub(crate) fn glyph_indexed(&mut self, glyph_id: u16, style: u8) -> Option<GlyphBitmap> {
        let style_key = style & STYLE_BOLD;
        if let Some(g) = self.shaped_glyph_cache.get(&(glyph_id, style_key)) {
            return Some(g.clone());
        }

        let font = self.primary_font.as_ref()?;
        let (metrics, bitmap) = font.rasterize_indexed(glyph_id, self.font_size_px.max(1.0));
        if metrics.width == 0 || metrics.height == 0 {
            return None;
        }

        let glyph = GlyphBitmap {
            width: metrics.width,
            height: metrics.height,
            xmin: metrics.xmin as f32,
            ymin: metrics.ymin as f32,
            advance_width: metrics.advance_width,
            alpha: bitmap,
        };
        self.insert_shaped_glyph_cache((glyph_id, style_key), glyph.clone());
        Some(glyph)
    }

    fn insert_glyph_cache(&mut self, key: (char, u8), glyph: GlyphBitmap) {
        if self.glyph_cache.insert(key, glyph).is_none() {
            self.glyph_cache_order.push_back(key);
        }
        while self.glyph_cache.len() > GLYPH_CACHE_LIMIT {
            if let Some(oldest_key) = self.glyph_cache_order.pop_front() {
                self.glyph_cache.remove(&oldest_key);
            } else {
                break;
            }
        }
    }

    fn insert_shaped_glyph_cache(&mut self, key: (u16, u8), glyph: GlyphBitmap) {
        if self.shaped_glyph_cache.insert(key, glyph).is_none() {
            self.shaped_glyph_cache_order.push_back(key);
        }
        while self.shaped_glyph_cache.len() > SHAPED_GLYPH_CACHE_LIMIT {
            if let Some(oldest_key) = self.shaped_glyph_cache_order.pop_front() {
                self.shaped_glyph_cache.remove(&oldest_key);
            } else {
                break;
            }
        }
    }

    pub(crate) fn shape_terminal_text(&self, text: &str) -> Option<Vec<Vec<ShapedGlyph>>> {
        let source = self.primary_font_source.as_ref()?;
        let face = rustybuzz::Face::from_slice(source.bytes.as_ref(), source.face_index)?;
        let units_per_em = face.units_per_em() as f32;
        if units_per_em <= 0.0 {
            return None;
        }
        let px_per_unit = self.font_size_px.max(1.0) / units_per_em;

        let mut result = Vec::new();
        let mut full_char_offset = 0usize;

        for line in text.split('\n') {
            result.push(shape_line(&face, line, full_char_offset, px_per_unit));
            full_char_offset = full_char_offset.saturating_add(line.chars().count() + 1);
        }

        Some(result)
    }
}

// ── OpenType shaping ──────────────────────────────────────────────────────────

pub(crate) fn shape_line(
    face: &rustybuzz::Face<'_>,
    line: &str,
    full_char_offset: usize,
    px_per_unit: f32,
) -> Vec<ShapedGlyph> {
    if line.is_empty() {
        return Vec::new();
    }

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
        let Some(&(col, source_char)) = byte_to_info.get(&cluster) else {
            continue;
        };

        let span_cols_from_clusters = if i + 1 < infos.len() {
            let next_cluster = infos[i + 1].cluster;
            let next_col = byte_to_info
                .get(&next_cluster)
                .map(|&(c, _)| c)
                .unwrap_or(col + 1);
            (next_col.saturating_sub(col)).max(1)
        } else {
            (line_char_count.saturating_sub(col)).max(1)
        };
        // Wide characters (emoji, CJK) always occupy at least 2 columns.
        // The terminal grid stores a '\0' placeholder after wide chars; that
        // placeholder makes the cluster-distance calculation return 1 instead
        // of 2, giving a 1-column slot that causes left-overflow in rendering.
        let span_cols = span_cols_from_clusters.max(crate::util::char_col_width(source_char));

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

// ── Font loading helpers ──────────────────────────────────────────────────────

/// Return type of [`load_fonts_for_family`].
type LoadedFonts = (
    Option<fontdue::Font>,  // primary
    Option<FontSource>,     // primary source
    Vec<FontPathSource>,    // Unicode symbol fallback paths (ordered, lazy-loaded)
    Option<FontPathSource>, // outline emoji path (Noto Emoji, for fontdue, lazy-loaded)
    Option<(PathBuf, u32)>, // color emoji: (file path, face index)
);

pub(crate) fn load_fonts_for_family(family: Option<&str>) -> LoadedFonts {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let requested_family = family
        .map(str::trim)
        .filter(|name| !name.is_empty() && *name != "(default)")
        .map(ToOwned::to_owned);

    let resolved_family = requested_family
        .as_deref()
        .and_then(|name| resolve_family_name(&db, name).or_else(|| Some(name.to_owned())));

    if let Some(ref requested) = requested_family
        && let Some(ref resolved) = resolved_family
        && requested != resolved
    {
        tracing::info!(requested = %requested, resolved = %resolved, "resolved configured font family name");
    }

    let primary_source = resolved_family
        .as_deref()
        .and_then(|name| {
            load_font_source_from_query(
                &db,
                Query {
                    families: &[Family::Name(name)],
                    weight: Weight::NORMAL,
                    ..Query::default()
                },
            )
        })
        .or_else(|| {
            load_font_source_from_named_families(
                &db,
                &[
                    "Hack",
                    "DejaVu Sans Mono",
                    "Consolas",
                    "Courier New",
                    "Menlo",
                ],
            )
        })
        .or_else(|| {
            load_font_source_from_query(
                &db,
                Query {
                    families: &[Family::Monospace],
                    weight: Weight::NORMAL,
                    ..Query::default()
                },
            )
        });
    let primary = primary_source.as_ref().and_then(|source| {
        fontdue::Font::from_bytes(
            source.bytes.as_ref(),
            fontdue::FontSettings {
                collection_index: source.face_index,
                ..fontdue::FontSettings::default()
            },
        )
        .ok()
    });

    let fallback = load_unicode_fallback_fonts(&db);

    // Get path to monochrome outline emoji font for lazy loading.
    // Only fonts whose glyph outlines fontdue can rasterize (glyf/CFF).
    // Apple Color Emoji uses SBIX bitmaps which fontdue cannot render, so it
    // is intentionally excluded — its glyf fallback entries are empty stubs
    // that produce misleading outlined-square bitmaps.
    let emoji_families = &[
        "Noto Emoji",
        "Noto Color Emoji",
        "Segoe UI Emoji",
        "Twitter Color Emoji",
        "EmojiOne Mozilla",
    ];
    let emoji = find_emoji_font_path_for_families(&db, emoji_families);

    // Locate the color emoji font by file path only — the file will be
    // memory-mapped on demand so we never copy the full 100+ MB into the heap.
    let color_emoji_source = find_emoji_font_path(
        &db,
        &[
            "Apple Color Emoji", // macOS – SBIX PNG strikes (~183 MB, mmapped)
            "Noto Color Emoji",  // Linux/Android – CBDT PNG strikes
            "Segoe UI Emoji",    // Windows – COLR (glyph_raster_image → None)
            "Twitter Color Emoji",
            "EmojiOne Mozilla",
        ],
    );

    if emoji.is_some() || color_emoji_source.is_some() {
        tracing::debug!(
            outline_emoji = emoji.is_some(),
            color_emoji = color_emoji_source.is_some(),
            "emoji fonts loaded"
        );
    }

    if primary.is_none() {
        tracing::warn!(requested = ?requested_family, "failed to load requested font family; using fallback rendering");
    }

    (primary, primary_source, fallback, emoji, color_emoji_source)
}

/// Load an ordered list of Unicode symbol fallback font paths.
/// Specific fonts with known good Unicode coverage are preferred over generic
/// family queries (SansSerif on macOS resolves to San Francisco, which has stub
/// glyph outlines for many Unicode ranges that fontdue cannot render).
fn load_unicode_fallback_fonts(db: &fontdb::Database) -> Vec<FontPathSource> {
    const FAMILIES: &[&str] = &[
        "Apple Symbols",
        "Apple Braille",
        "Zapf Dingbats",
        "Menlo",
        "Arial Unicode MS",
        "Segoe UI Symbol",
        "Noto Sans",
        "DejaVu Sans",
        "FreeSans",
    ];

    let mut loaded_families = std::collections::HashSet::new();
    let mut fonts: Vec<FontPathSource> = FAMILIES
        .iter()
        .filter_map(|family| {
            let path_source = find_font_path(
                db,
                Query {
                    families: &[Family::Name(family)],
                    weight: Weight::NORMAL,
                    ..Query::default()
                },
            )?;
            loaded_families.insert(*family);
            Some(path_source)
        })
        .collect();

    // Direct-path fallbacks for fonts that fontdb might miss due to non-standard
    // scan paths or naming differences. Only loaded if fontdb didn't already find
    // the family by name.
    fn path_exists(path: &str) -> Option<FontPathSource> {
        let pb = PathBuf::from(path);
        if pb.is_file() {
            Some(FontPathSource {
                path: pb,
                face_index: 0,
            })
        } else {
            None
        }
    }

    #[cfg(target_os = "macos")]
    {
        if !loaded_families.contains("Zapf Dingbats")
            && let Some(source) = path_exists("/System/Library/Fonts/ZapfDingbats.ttf")
        {
            fonts.push(source);
        }
        if !loaded_families.contains("Menlo")
            && let Some(source) = path_exists("/System/Library/Fonts/Menlo.ttc")
        {
            fonts.push(source);
        }
    }

    #[cfg(target_os = "windows")]
    {
        let candidates = [
            ("Segoe UI Symbol", r"C:\Windows\Fonts\seguisym.ttf"),
            ("Arial Unicode MS", r"C:\Windows\Fonts\ARIALUNI.TTF"),
            ("Arial", r"C:\Windows\Fonts\arial.ttf"),
        ];
        for (family, path) in candidates.iter() {
            if !loaded_families.contains(family)
                && let Some(source) = path_exists(path)
            {
                fonts.push(source);
                loaded_families.insert(family);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            (
                "DejaVu Sans",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ),
            (
                "Noto Sans",
                "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            ),
            ("Noto Sans", "/usr/share/fonts/noto/NotoSans-Regular.ttf"),
            (
                "Liberation Sans",
                "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            ),
            (
                "Liberation Sans",
                "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
            ),
            (
                "FreeSans",
                "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
            ),
            ("FreeSans", "/usr/share/fonts/freefont/FreeSans.ttf"),
        ];
        for (family, path) in candidates.iter() {
            if !loaded_families.contains(family)
                && let Some(source) = path_exists(path)
            {
                fonts.push(source);
                loaded_families.insert(family);
            }
        }
    }

    fonts
}

/// Find the file path and face index for a font matching the given query.
fn find_font_path(db: &fontdb::Database, query: Query<'_>) -> Option<FontPathSource> {
    let id = db.query(&query)?;
    let face_info = db.faces().find(|fi| fi.id == id)?;
    let path = match &face_info.source {
        fontdb::Source::File(p) => p.to_path_buf(),
        fontdb::Source::SharedFile(p, _) => p.to_path_buf(),
        fontdb::Source::Binary(_) => return None,
    };
    Some(FontPathSource {
        path,
        face_index: face_info.index,
    })
}

fn resolve_family_name(db: &fontdb::Database, name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut case_insensitive_match: Option<String> = None;
    for face in db.faces() {
        for (family_name, _) in &face.families {
            let candidate = family_name.trim();
            if candidate.is_empty() {
                continue;
            }
            if candidate == trimmed {
                return Some(candidate.to_owned());
            }
            if case_insensitive_match.is_none() && candidate.eq_ignore_ascii_case(trimmed) {
                case_insensitive_match = Some(candidate.to_owned());
            }
        }
    }
    case_insensitive_match
}

fn load_font_source_from_named_families(
    db: &fontdb::Database,
    families: &[&str],
) -> Option<FontSource> {
    for family in families {
        if let Some(source) = load_font_source_from_query(
            db,
            Query {
                families: &[Family::Name(family)],
                weight: Weight::NORMAL,
                ..Query::default()
            },
        ) {
            return Some(source);
        }
    }
    None
}

/// Return the on-disk path and face index for the first matching emoji font.
/// Returns `None` if the font is not found or is not file-backed.
fn find_emoji_font_path_for_families(
    db: &fontdb::Database,
    families: &[&str],
) -> Option<FontPathSource> {
    families.iter().find_map(|family| {
        find_font_path(
            db,
            Query {
                families: &[Family::Name(family)],
                weight: Weight::NORMAL,
                ..Query::default()
            },
        )
    })
}

/// Return the on-disk path and face index for the first matching color emoji
/// font.  Returns `None` if the font is not found or is not file-backed
/// (i.e. loaded from in-memory binary data).
fn find_emoji_font_path(db: &fontdb::Database, families: &[&str]) -> Option<(PathBuf, u32)> {
    for family in families {
        let id = db.query(&Query {
            families: &[Family::Name(family)],
            weight: Weight::NORMAL,
            ..Query::default()
        });
        let Some(id) = id else {
            continue;
        };
        let face_info = db.faces().find(|fi| fi.id == id)?;
        let path = match &face_info.source {
            fontdb::Source::File(p) => p.to_path_buf(),
            fontdb::Source::SharedFile(p, _) => p.to_path_buf(),
            fontdb::Source::Binary(_) => continue,
        };
        #[cfg(target_os = "linux")]
        tracing::info!(
            family,
            path = %path.display(),
            face_index = face_info.index,
            "emoji font: resolved from fontdb family query"
        );
        return Some((path, face_info.index));
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
            "/usr/share/fonts/noto/NotoColorEmoji.ttf",
            "/usr/local/share/fonts/NotoColorEmoji.ttf",
        ];
        for path in candidates {
            let pb = PathBuf::from(path);
            if pb.is_file() {
                tracing::info!(
                    path = %pb.display(),
                    "emoji font: resolved from direct Linux fallback path"
                );
                return Some((pb, 0));
            }
        }
        tracing::warn!(
            "emoji font: no usable color emoji font found via family query or Linux fallback paths"
        );
    }

    None
}

fn load_font_source_from_query(db: &fontdb::Database, query: Query<'_>) -> Option<FontSource> {
    let id = db.query(&query)?;
    let mut loaded: Option<FontSource> = None;
    let _ = db.with_face_data(id, |data, face_index| {
        loaded = Some(FontSource {
            bytes: data.to_vec().into_boxed_slice(),
            face_index,
        });
    });
    loaded
}
