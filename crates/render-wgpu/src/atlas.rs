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

/// Tries to load raw font bytes from the configured path or OS fallbacks.
pub(crate) fn load_font_bytes(config: &FontConfig) -> Option<Vec<u8>> {
    if let Some(ref path) = config.font_path {
        match std::fs::read(path) {
            Ok(bytes) => return Some(bytes),
            Err(e) => eprintln!("render-wgpu: cannot load font '{path}': {e}, trying fallback"),
        }
    }

    let home = std::env::var("HOME").unwrap_or_default();

    let owned_candidates: Vec<String> = if cfg!(target_os = "macos") {
        vec![
            format!("{home}/Library/Fonts/HackNerdFontMono-Regular.ttf"),
            format!("{home}/Library/Fonts/HackNerdFont-Regular.ttf"),
            "/Library/Fonts/HackNerdFontMono-Regular.ttf".into(),
            "/Library/Fonts/HackNerdFont-Regular.ttf".into(),
            "/System/Library/Fonts/Monaco.ttf".into(),
            "/Library/Fonts/Courier New.ttf".into(),
            "/System/Library/Fonts/Supplemental/Courier New.ttf".into(),
        ]
    } else if cfg!(target_os = "linux") {
        vec![
            "/usr/share/fonts/truetype/hack/Hack-Regular.ttf".into(),
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf".into(),
            "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf".into(),
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf".into(),
            "/usr/share/fonts/noto/NotoMono-Regular.ttf".into(),
        ]
    } else {
        vec![]
    };

    for path in &owned_candidates {
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    eprintln!("render-wgpu: no system font found — text will not be rendered");
    None
}

pub(crate) const TEXT_ATLAS_SIZE: u32 = 1024;

/// Rasterizes one glyph into the atlas and returns its cached descriptor.
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
