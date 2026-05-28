use std::collections::HashMap;

use crate::types::FontConfig;

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

#[derive(Debug, Default)]
pub struct GlyphAtlas {
    entries: HashMap<GlyphKey, GlyphEntry>,
}

impl GlyphAtlas {
    pub fn get(&self, key: &GlyphKey) -> Option<&GlyphEntry> {
        self.entries.get(key)
    }

    pub fn insert(&mut self, key: GlyphKey, entry: GlyphEntry) {
        self.entries.insert(key, entry);
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
}

/// Tries to load raw font bytes from the configured family or system fallbacks.
pub(crate) fn load_font_bytes(config: &FontConfig) -> Option<Vec<u8>> {
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

    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    if let Some(ref family) = config.font_family {
        if let Some(bytes) = query_bytes_by_family(&db, family) {
            return Some(bytes);
        }
        eprintln!("render-wgpu: cannot load font family '{family}', trying fallback");
    }

    for family in ["Hack", "DejaVu Sans Mono", "Consolas", "Courier New", "Menlo"] {
        if let Some(bytes) = query_bytes_by_family(&db, family) {
            return Some(bytes);
        }
    }

    if let Some(bytes) = query_monospace_bytes(&db) {
        return Some(bytes);
    }

    eprintln!("render-wgpu: no system font found — text will not be rendered");
    None
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
        eprintln!("render-wgpu: glyph atlas full");
        return CachedGlyph::default();
    }
    let dest_x = *alloc_x;
    let dest_y = *alloc_y;
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: atlas_texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x: dest_x, y: dest_y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        bitmap,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(gw),
            rows_per_image: Some(gh),
        },
        wgpu::Extent3d { width: gw, height: gh, depth_or_array_layers: 1 },
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
    CachedGlyph { u0, v0, u1, v1, offset_x_px, offset_y_px, width_px: gw as f32, height_px: gh as f32 }
}
