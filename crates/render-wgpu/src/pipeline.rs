use std::collections::HashMap;

use anyhow::Result;
use wgpu::SurfaceError;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::atlas::{
    CachedGlyph, load_bold_font_bytes, load_font_bytes, load_system_font_database,
    load_unicode_fallback_font_bytes, pack_glyph, shape_terminal_text,
};
use crate::geometry::{
    TEXT_VERTEX_BUF_CAPACITY, VERTEX_BUF_CAPACITY, add_text_verts, add_text_verts_shaped,
    build_command_palette_bg_verts, build_panel_vertices, build_scroll_indicator_bg_verts,
    build_settings_overlay_bg_verts, build_suggestion_dropdown_bg_verts, build_toast_bg_verts,
    floats_as_bytes,
};
use crate::shell_highlight::highlight_shell;
use crate::types::{ColorTheme, RenderConfig, RenderSnapshot};

pub(crate) struct GpuState<'a> {
    pub(crate) surface: wgpu::Surface<'a>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) size: PhysicalSize<u32>,
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    text_pipeline: wgpu::RenderPipeline,
    text_vertex_buf: wgpu::Buffer,
    pub(crate) atlas_texture: wgpu::Texture,
    atlas_bind_group: wgpu::BindGroup,
    pub(crate) font: Option<fontdue::Font>,
    pub(crate) font_size: f32,
    pub(crate) cell_w_px: f32,
    pub(crate) cell_h_px: f32,
    pub(crate) glyph_cache: HashMap<char, CachedGlyph>,
    pub(crate) bold_font: Option<fontdue::Font>,
    pub(crate) bold_glyph_cache: HashMap<char, CachedGlyph>,
    pub(crate) atlas_alloc_x: u32,
    pub(crate) atlas_alloc_y: u32,
    pub(crate) atlas_row_h: u32,
    theme: ColorTheme,
    /// Raw font bytes kept for rustybuzz shaping. `None` when no font was found.
    font_data: Option<Box<[u8]>>,
    /// Cache of glyphs rasterized by TTF glyph ID (used for ligature glyphs).
    /// Keyed by `(glyph_id, is_bold)`.
    pub(crate) shaped_glyph_cache: HashMap<(u16, bool), CachedGlyph>,
    /// Fallback font for non-ASCII characters not covered by the primary monospace font.
    pub(crate) unicode_fallback_font: Option<fontdue::Font>,
    /// Raw bytes of a colour emoji font (Apple Color Emoji / Noto Color Emoji / Segoe UI Emoji).
    /// Used to extract SBIX/CBDT raster images for characters fontdue cannot render.
    ///
    /// Lazily populated on first emoji rasterisation (emoji fonts are large —
    /// Apple Color Emoji is ~180 MB — so we avoid paying that cost upfront).
    pub(crate) emoji_font_bytes: Option<Box<[u8]>>,
    /// Set once we've attempted to load the emoji font, so we don't retry on
    /// every miss when no emoji font is available on the system.
    pub(crate) emoji_load_attempted: bool,
    /// The `terminal_screen_version` value from the last successfully rendered
    /// snapshot.  When the incoming snapshot carries the same version and the
    /// terminal cursor / scroll position haven't changed, the terminal text
    /// vertex buffers are re-used from the previous frame instead of being
    /// re-uploaded, saving GPU bandwidth on idle frames.
    last_terminal_version: u64,
    /// Terminal cursor position at the time of the last terminal vertex upload.
    last_cursor_pos: (usize, usize),
    /// Terminal scroll offset at the time of the last terminal vertex upload.
    last_scroll_offset: usize,
    /// Cached terminal text vertices from the last upload. Re-used when
    /// `last_terminal_version` matches the current snapshot version.
    terminal_verts_cache: Vec<f32>,
}

struct FontInitResources {
    font: Option<fontdue::Font>,
    font_data: Option<Box<[u8]>>,
    font_size: f32,
    cell_w_px: f32,
    cell_h_px: f32,
    glyph_cache: HashMap<char, CachedGlyph>,
    bold_font: Option<fontdue::Font>,
    bold_glyph_cache: HashMap<char, CachedGlyph>,
    atlas_alloc_x: u32,
    atlas_alloc_y: u32,
    atlas_row_h: u32,
    unicode_fallback_font: Option<fontdue::Font>,
}

struct AsciiRasterContext<'a> {
    font_size: f32,
    queue: &'a wgpu::Queue,
    atlas_texture: &'a wgpu::Texture,
    atlas_alloc_x: &'a mut u32,
    atlas_alloc_y: &'a mut u32,
    atlas_row_h: &'a mut u32,
    cell_h_px: f32,
}

fn rasterize_ascii_glyphs(
    font: Option<&fontdue::Font>,
    ctx: &mut AsciiRasterContext<'_>,
) -> HashMap<char, CachedGlyph> {
    let mut glyph_cache = HashMap::new();
    if let Some(font) = font {
        for ch in ' '..='~' {
            let (metrics, bitmap) = font.rasterize(ch, ctx.font_size);
            let cached = pack_glyph(
                ctx.queue,
                ctx.atlas_texture,
                ctx.atlas_alloc_x,
                ctx.atlas_alloc_y,
                ctx.atlas_row_h,
                &metrics,
                &bitmap,
                ctx.cell_h_px,
            );
            glyph_cache.insert(ch, cached);
        }
    }
    glyph_cache
}

fn init_font_resources(
    queue: &wgpu::Queue,
    atlas_texture: &wgpu::Texture,
    render_config: &RenderConfig,
) -> FontInitResources {
    // The system-font database is expensive to build (scans hundreds of
    // files on macOS and keeps mmap handles open), so we build it once,
    // share it across lookups, then drop it before returning.
    let font_db = load_system_font_database();
    let font_size = render_config.font.font_size;
    let font_bytes = load_font_bytes(&font_db, &render_config.font);
    let font = font_bytes.as_ref().and_then(|bytes| {
        fontdue::Font::from_bytes(bytes.as_slice(), fontdue::FontSettings::default()).ok()
    });
    let font_data = font_bytes.map(Vec::into_boxed_slice);
    let (cell_w_px, cell_h_px) = font
        .as_ref()
        .map(|f| (f.metrics('M', font_size).advance_width, font_size * 1.2))
        .unwrap_or((font_size * 0.6, font_size * 1.2));

    let mut atlas_alloc_x = 0u32;
    let mut atlas_alloc_y = 0u32;
    let mut atlas_row_h = 0u32;
    let mut raster_ctx = AsciiRasterContext {
        font_size,
        queue,
        atlas_texture,
        atlas_alloc_x: &mut atlas_alloc_x,
        atlas_alloc_y: &mut atlas_alloc_y,
        atlas_row_h: &mut atlas_row_h,
        cell_h_px,
    };
    let glyph_cache = rasterize_ascii_glyphs(font.as_ref(), &mut raster_ctx);

    let unicode_fallback_font = load_unicode_fallback_font_bytes(&font_db).and_then(|bytes| {
        fontdue::Font::from_bytes(bytes.as_slice(), fontdue::FontSettings::default()).ok()
    });

    let bold_font_bytes = load_bold_font_bytes(&font_db, &render_config.font);
    let bold_font = bold_font_bytes.as_ref().and_then(|bytes| {
        fontdue::Font::from_bytes(bytes.as_slice(), fontdue::FontSettings::default()).ok()
    });
    let bold_glyph_cache = rasterize_ascii_glyphs(bold_font.as_ref(), &mut raster_ctx);

    drop(font_db);

    FontInitResources {
        font,
        font_data,
        font_size,
        cell_w_px,
        cell_h_px,
        glyph_cache,
        bold_font,
        bold_glyph_cache,
        atlas_alloc_x,
        atlas_alloc_y,
        atlas_row_h,
        unicode_fallback_font,
    }
}

impl<'a> GpuState<'a> {
    pub(crate) async fn new(window: &'a Window, render_config: &RenderConfig) -> Result<Self> {
        let crate::surface::SurfaceInit {
            surface,
            device,
            queue,
            config,
            size,
        } = crate::surface::init_surface(window, render_config).await?;
        let format = config.format;

        // Background (flat-color) pipeline
        let crate::surface::BgPipeline {
            pipeline,
            vertex_buf,
        } = crate::surface::build_bg_pipeline(&device, format);

        // Glyph atlas texture + bind group
        let crate::surface::AtlasResources {
            texture: atlas_texture,
            bind_group_layout: atlas_bgl,
            bind_group: atlas_bind_group,
        } = crate::surface::build_atlas_resources(&device);

        // Text (atlas-sampled) pipeline
        let crate::surface::TextPipeline {
            pipeline: text_pipeline,
            vertex_buf: text_vertex_buf,
        } = crate::surface::build_text_pipeline(&device, format, &atlas_bgl);
        let FontInitResources {
            font,
            font_data,
            font_size,
            cell_w_px,
            cell_h_px,
            glyph_cache,
            bold_font,
            bold_glyph_cache,
            atlas_alloc_x,
            atlas_alloc_y,
            atlas_row_h,
            unicode_fallback_font,
        } = init_font_resources(&queue, &atlas_texture, render_config);

        // Colour emoji font is loaded lazily on first use — see
        // `glyph_raster::ensure_emoji_font_loaded`.

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            vertex_buf,
            text_pipeline,
            text_vertex_buf,
            atlas_texture,
            atlas_bind_group,
            font,
            font_size,
            cell_w_px,
            cell_h_px,
            glyph_cache,
            bold_font,
            bold_glyph_cache,
            atlas_alloc_x,
            atlas_alloc_y,
            atlas_row_h,
            theme: render_config.theme.clone(),
            font_data,
            shaped_glyph_cache: HashMap::new(),
            unicode_fallback_font,
            emoji_font_bytes: None,
            emoji_load_attempted: false,
            last_terminal_version: 0,
            last_cursor_pos: (usize::MAX, usize::MAX),
            last_scroll_offset: usize::MAX,
            terminal_verts_cache: Vec::new(),
        })
    }

    pub(crate) fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    pub(crate) fn invalidate_terminal_text_cache(&mut self) {
        self.terminal_verts_cache.clear();
        self.last_terminal_version = 0;
        self.last_cursor_pos = (usize::MAX, usize::MAX);
        self.last_scroll_offset = usize::MAX;
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // GPU render loop; tracked as follow-up to T8/T16
    pub(crate) fn render(&mut self, snapshot: &RenderSnapshot) -> Result<()> {
        // Sync theme from snapshot so live config changes are picked up immediately.
        self.theme = snapshot.theme.clone();
        let surface_texture = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(SurfaceError::Lost) => {
                self.resize(self.size);
                return Ok(());
            }
            Err(SurfaceError::OutOfMemory) => {
                return Err(anyhow::anyhow!("wgpu surface out of memory"));
            }
            Err(SurfaceError::Outdated | SurfaceError::Timeout) => return Ok(()),
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("teletipo-command-encoder"),
            });

        // Compute tab-bar height (in pixels) — shown whenever there are tabs.
        let tab_bar_h = if !snapshot.tab_labels.is_empty() {
            self.cell_h_px
        } else {
            0.0
        };
        let tab_bar_h_u32 = tab_bar_h.round() as u32;
        let available_h = self.size.height as f32 - tab_bar_h;

        let pad_h = snapshot.padding_h as f32;
        let pad_v = snapshot.padding_v as f32;
        let terminal_text = snapshot.terminal_text_from_rows();
        let (terminal_fg_colors, _terminal_bg_colors, terminal_styles) =
            snapshot.terminal_flatten_fg_bg_style();

        // Bottom-align terminal content within the padded area: pad_v is reserved at
        // both the top and bottom of the terminal pane so the last visible row ends
        // pad_v pixels above the separator.
        let terminal_rows = snapshot.terminal_rows_len() as f32;
        let term_pane_h_px = snapshot.split_ratio * available_h;
        let effective_term_h = (term_pane_h_px - 2.0 * pad_v).max(0.0);
        let content_h_px = (terminal_rows * self.cell_h_px).min(effective_term_h);
        let term_top_offset_px = tab_bar_h + (effective_term_h - content_h_px).max(0.0);

        let panel_verts = build_panel_vertices(
            self.size,
            snapshot,
            term_top_offset_px,
            self.cell_w_px,
            self.cell_h_px,
            pad_h,
            pad_v,
        );
        let panel_vertex_count = (panel_verts.len() / 6) as u32;
        let dropdown_bg_verts = build_suggestion_dropdown_bg_verts(
            self.size,
            snapshot,
            self.cell_w_px,
            self.cell_h_px,
            pad_h,
        );
        let dropdown_bg_start = panel_vertex_count;
        let dropdown_bg_count = (dropdown_bg_verts.len() / 6) as u32;
        let overlay_bg_verts =
            build_settings_overlay_bg_verts(self.size, snapshot, self.cell_w_px, self.cell_h_px);
        let overlay_bg_start = dropdown_bg_start + dropdown_bg_count;
        let overlay_bg_count = (overlay_bg_verts.len() / 6) as u32;
        let scroll_indicator_bg_verts = build_scroll_indicator_bg_verts(
            self.size,
            snapshot,
            self.cell_w_px,
            self.cell_h_px,
            tab_bar_h,
        );
        let scroll_indicator_bg_start = overlay_bg_start + overlay_bg_count;
        let scroll_indicator_bg_count = (scroll_indicator_bg_verts.len() / 6) as u32;
        let command_palette_bg_verts = build_command_palette_bg_verts(
            self.size,
            snapshot,
            self.cell_w_px,
            self.cell_h_px,
            tab_bar_h,
        );
        let command_palette_bg_start = scroll_indicator_bg_start + scroll_indicator_bg_count;
        let command_palette_bg_count = (command_palette_bg_verts.len() / 6) as u32;
        let toast_bg_verts =
            build_toast_bg_verts(self.size, snapshot, self.cell_w_px, self.cell_h_px);
        let toast_bg_start = command_palette_bg_start + command_palette_bg_count;
        let toast_bg_count = (toast_bg_verts.len() / 6) as u32;
        {
            // Upload main bg + dropdown bg + settings overlay bg + scroll indicator + command palette + toast bg, in draw order.
            let mut all_bg = panel_verts;
            all_bg.extend_from_slice(&dropdown_bg_verts);
            all_bg.extend_from_slice(&overlay_bg_verts);
            all_bg.extend_from_slice(&scroll_indicator_bg_verts);
            all_bg.extend_from_slice(&command_palette_bg_verts);
            all_bg.extend_from_slice(&toast_bg_verts);
            if !all_bg.is_empty() {
                let bytes = floats_as_bytes(&all_bg);
                let cap = VERTEX_BUF_CAPACITY as usize;
                self.queue
                    .write_buffer(&self.vertex_buf, 0, &bytes[..bytes.len().min(cap)]);
            }
        }

        self.ensure_glyph('\u{276f}');
        self.ensure_glyph('\u{d7}'); // × close-button character
        for ch in terminal_text
            .chars()
            .chain(snapshot.editor_text.chars())
            .chain(snapshot.editor_suggestion.chars())
        {
            if ch != '\n' && ch != '\r' && ch != '\t' && ch != ' ' {
                self.ensure_glyph(ch);
            }
        }
        if let Some(ref overlay_text) = snapshot.resize_overlay {
            for ch in overlay_text.chars() {
                if ch != ' ' {
                    self.ensure_glyph(ch);
                }
            }
        }
        // Pre-cache all tab label characters plus × and + used in the tab bar.
        for label in &snapshot.tab_labels {
            for ch in label.chars() {
                if ch != ' ' {
                    self.ensure_glyph(ch);
                }
            }
        }
        // Context menu item text characters (all ASCII, already cached by the ' '..='~'
        // loop in `new`, but ensure_glyph is idempotent so this is safe).
        if let Some(ref menu) = snapshot.context_menu {
            let _ = menu; // characters are ASCII — already in cache
        }
        // Pre-cache suggestion dropdown characters.
        if let Some(ref dd) = snapshot.suggestion_dropdown {
            for item in &dd.items {
                for ch in item.chars() {
                    if ch != ' ' {
                        self.ensure_glyph(ch);
                    }
                }
            }
        }
        if let Some(ref panel) = snapshot.search_panel {
            for ch in panel.query.chars() {
                if ch != ' ' {
                    self.ensure_glyph(ch);
                }
            }
            for ch in [
                'F', 'i', 'n', 'd', ':', '/', '↑', '↓', '×', 'R', 'C', 'c', '[', ']',
            ] {
                self.ensure_glyph(ch);
            }
        }
        // Pre-cache scroll indicator glyphs (↑ and digit characters).
        if snapshot.scroll_offset > 0 {
            for ch in ['↑', 'l', 'i', 'n', 'e', 's'] {
                self.ensure_glyph(ch);
            }
        }
        // Pre-cache command palette characters.
        if let Some(ref cp) = snapshot.command_palette {
            self.ensure_glyph('>');
            self.ensure_glyph('_');
            for ch in cp
                .query
                .chars()
                .chain(cp.items.iter().flat_map(|s| s.chars()))
            {
                if ch != ' ' {
                    self.ensure_glyph(ch);
                }
            }
        }
        for toast in &snapshot.toast_stack {
            for ch in toast.text.chars() {
                if ch != ' ' {
                    self.ensure_glyph(ch);
                }
            }
        }

        // Pixel coordinates for the terminal and editor pane boundaries.
        let term_top_px = term_top_offset_px;
        let edit_top_px = (tab_bar_h + snapshot.split_ratio * available_h + 2.0).round();
        let split_y_px = (tab_bar_h + snapshot.split_ratio * available_h)
            .round()
            .clamp(1.0, self.size.height.saturating_sub(1) as f32) as u32;

        let mut text_verts: Vec<f32> = Vec::new();

        // Check whether the terminal text vertices can be reused from the
        // previous frame.  They are valid when the screen content, cursor
        // position, and scroll offset are all unchanged.
        let terminal_cache_valid = snapshot.terminal_screen_version != 0
            && snapshot.terminal_screen_version == self.last_terminal_version
            && (snapshot.terminal_cursor_row, snapshot.terminal_cursor_col) == self.last_cursor_pos
            && snapshot.scroll_offset == self.last_scroll_offset;

        // Shape terminal text for ligature rendering when font data is available.
        // Falls back to character-by-character rendering when shaping is unavailable.
        let shaped_terminal = if terminal_cache_valid {
            None // no need to reshape when the cache is valid
        } else {
            self.font_data
                .as_deref()
                .and_then(|fd| shape_terminal_text(fd, &terminal_text, self.font_size))
        };

        // Pre-rasterize any ligature glyphs (span_cols > 1) into the shaped glyph cache.
        if let Some(ref shaped) = shaped_terminal {
            for shaped_line in shaped {
                for sg in shaped_line {
                    if sg.span_cols > 1 {
                        let is_bold = terminal_styles.get(sg.full_char_idx).copied().unwrap_or(0)
                            & 0b001
                            != 0;
                        self.ensure_shaped_glyph(sg.glyph_id, is_bold);
                    }
                }
            }
        }

        if terminal_cache_valid {
            // Reuse the cached terminal vertex data from the previous frame.
            text_verts.extend_from_slice(&self.terminal_verts_cache);
        } else {
            let terminal_vert_start = text_verts.len();
            if let Some(ref shaped) = shaped_terminal {
                add_text_verts_shaped(
                    term_top_px + pad_v,
                    pad_h,
                    self.theme.text,
                    &terminal_fg_colors,
                    &terminal_styles,
                    shaped,
                    &self.shaped_glyph_cache,
                    &self.glyph_cache,
                    Some(&self.bold_glyph_cache),
                    self.cell_w_px,
                    self.cell_h_px,
                    self.size,
                    &mut text_verts,
                    0,
                );
            } else {
                add_text_verts(
                    &terminal_text,
                    term_top_px + pad_v,
                    pad_h,
                    self.theme.text,
                    &terminal_fg_colors,
                    &terminal_styles,
                    &self.glyph_cache,
                    Some(&self.bold_glyph_cache),
                    self.cell_w_px,
                    self.cell_h_px,
                    self.size,
                    &mut text_verts,
                    0,
                );
            }

            // Update the terminal vertex cache.
            self.terminal_verts_cache.clear();
            self.terminal_verts_cache
                .extend_from_slice(&text_verts[terminal_vert_start..]);
            self.last_terminal_version = snapshot.terminal_screen_version;
            self.last_cursor_pos = (snapshot.terminal_cursor_row, snapshot.terminal_cursor_col);
            self.last_scroll_offset = snapshot.scroll_offset;
        }

        let terminal_vert_count = (text_verts.len() / 8) as u32;

        let editor_skip = snapshot.editor_scroll_offset;
        let prefix_color = [0.40, 0.80, 1.00, 1.0_f32];
        if editor_skip == 0 && snapshot.editor_horizontal_scroll_offset == 0 {
            add_text_verts(
                "\u{276f} ",
                edit_top_px + pad_v,
                pad_h,
                prefix_color,
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );
        }
        let editor_hl = highlight_shell(&snapshot.editor_text);
        let mut padded_hl: Vec<Option<[f32; 3]>> = vec![None, None];
        padded_hl.extend(editor_hl);
        // Append ghost-text suggestion in dim grey when present.
        let padded_editor = if snapshot.editor_suggestion.is_empty() {
            format!("  {}", snapshot.editor_text)
        } else {
            let ghost_color: [f32; 3] = [0.50, 0.50, 0.50];
            for _ in snapshot.editor_suggestion.chars() {
                padded_hl.push(Some(ghost_color));
            }
            format!("  {}{}", snapshot.editor_text, snapshot.editor_suggestion)
        };
        add_text_verts(
            &padded_editor,
            edit_top_px + pad_v,
            pad_h - snapshot.editor_horizontal_scroll_offset as f32 * self.cell_w_px,
            self.theme.text,
            &padded_hl,
            &[],
            &self.glyph_cache,
            None,
            self.cell_w_px,
            self.cell_h_px,
            self.size,
            &mut text_verts,
            editor_skip,
        );

        // Tab label text — rendered inside the tab bar region at the very top.
        // The rightmost (2 × cell_w) pixels are reserved for the "+" button.
        let tab_text_vert_start = (text_verts.len() / 8) as u32;
        if !snapshot.tab_labels.is_empty() {
            let n = snapshot.tab_labels.len();
            let add_btn_w = self.cell_w_px * 2.0;
            let tab_area_w = self.size.width as f32 - add_btn_w;
            let tab_w_px = tab_area_w / n as f32;
            let th = &snapshot.theme;
            for (i, label) in snapshot.tab_labels.iter().enumerate() {
                let tab_x0 = i as f32 * tab_w_px;
                let tab_x1 = (i + 1) as f32 * tab_w_px;
                let text_color = if i == snapshot.active_tab {
                    th.text
                } else {
                    let [r, g, b, _] = th.text;
                    [r * 0.65, g * 0.65, b * 0.65, 1.0]
                };
                // Label — left-padded, leaving room for the × button on the right.
                add_text_verts(
                    label,
                    0.0,
                    tab_x0 + self.cell_w_px * 0.4,
                    text_color,
                    &[],
                    &[],
                    &self.glyph_cache,
                    None,
                    self.cell_w_px,
                    self.cell_h_px,
                    self.size,
                    &mut text_verts,
                    0,
                );
                // × close button at the right edge of the tab.
                let close_x = tab_x1 - self.cell_w_px * 1.3;
                let close_color = {
                    let [r, g, b] = th.ansi_palette[9];
                    [r * 0.80, g * 0.65 + 0.20, b * 0.65 + 0.20, 0.85_f32]
                };
                add_text_verts(
                    "\u{d7}",
                    0.0,
                    close_x,
                    close_color,
                    &[],
                    &[],
                    &self.glyph_cache,
                    None,
                    self.cell_w_px,
                    self.cell_h_px,
                    self.size,
                    &mut text_verts,
                    0,
                );
            }
            // "+" button text on the far right.
            let add_x = self.size.width as f32 - add_btn_w + self.cell_w_px * 0.5;
            add_text_verts(
                "+",
                0.0,
                add_x,
                {
                    let [r, g, b] = th.ansi_palette[10];
                    [r, g, b, 0.95_f32]
                },
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );
        }

        // Pre-cache settings overlay characters.
        if let Some(ref overlay) = snapshot.settings_overlay {
            for item in &overlay.items {
                for ch in item.key.chars().chain(item.value.chars()) {
                    if ch != ' ' {
                        self.ensure_glyph(ch);
                    }
                }
            }
            if let Some(ref buf) = overlay.editing {
                for ch in buf.chars() {
                    if ch != ' ' {
                        self.ensure_glyph(ch);
                    }
                }
            }
            // Pre-cache search buffer and match list characters.
            if let Some(ref sbuf) = overlay.search_buf {
                for ch in sbuf.chars() {
                    if ch != ' ' {
                        self.ensure_glyph(ch);
                    }
                }
            }
            for m in &overlay.search_matches {
                for ch in m.chars() {
                    if ch != ' ' {
                        self.ensure_glyph(ch);
                    }
                }
            }
            // Fixed UI characters used in settings overlay rendering:
            // ← → (arrows), ↑ ↓ (footer nav), ▶ (dropdown marker), ▌ (cursor hint).
            for ch in [
                '\u{2190}', '\u{2192}', '\u{2191}', '\u{2193}', '\u{25b6}', '\u{258e}',
            ] {
                self.ensure_glyph(ch);
            }
        }

        // Context menu item text — drawn with no scissor so it floats above everything.
        let context_text_vert_start = (text_verts.len() / 8) as u32;
        if let Some(ref menu) = snapshot.context_menu {
            if menu.items.is_empty() {
                // nothing to draw
            } else {
                let menu_item_h = self.cell_h_px * 1.15;
                let max_chars = menu
                    .items
                    .iter()
                    .map(|s| s.chars().count())
                    .max()
                    .unwrap_or(8);
                let menu_w = self.cell_w_px * (max_chars.max(8) as f32 + 2.0);
                let menu_h = menu_item_h * menu.items.len() as f32;
                let mx = menu.x_px.min(self.size.width as f32 - menu_w).max(0.0);
                let my = menu.y_px.min(self.size.height as f32 - menu_h).max(0.0);
                for (i, item) in menu.items.iter().enumerate() {
                    let text_color = if menu.hovered_item == Some(i) {
                        [1.0_f32, 1.0, 1.0, 1.0]
                    } else {
                        [0.78_f32, 0.82, 0.87, 1.0]
                    };
                    // Vertically centre the text within each item row.
                    let y_item = my + i as f32 * menu_item_h + (menu_item_h - self.cell_h_px) * 0.5;
                    add_text_verts(
                        item,
                        y_item,
                        mx + self.cell_w_px * 0.5,
                        text_color,
                        &[],
                        &[],
                        &self.glyph_cache,
                        None,
                        self.cell_w_px,
                        self.cell_h_px,
                        self.size,
                        &mut text_verts,
                        0,
                    );
                }
            }
        }

        if let Some(ref panel) = snapshot.search_panel {
            // Panel layout (40 cells wide):
            //  [Find: ____query_____  [R]  [Cc]  NNN/MMM  ↑  ↓  × ]
            //   0    6.6            20   23    27         34  36  38  40
            let panel_w = self.cell_w_px * 40.0;
            let panel_h = self.cell_h_px * 1.6;
            let panel_x = (self.size.width as f32 - pad_h - panel_w).max(0.0);
            let panel_y = tab_bar_h + pad_v;
            let button_w = self.cell_w_px * 2.0;
            let text_y = panel_y + (panel_h - self.cell_h_px) * 0.5;

            // ── "Find: " label ────────────────────────────────────────────────
            add_text_verts(
                "Find: ",
                text_y,
                panel_x + self.cell_w_px * 0.6,
                [0.55, 0.62, 0.78, 1.0],
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );

            // ── Query text (viewport-aware, up to 13 visible chars) ───────────
            const QUERY_TEXT_X: f32 = 6.6;
            const VISIBLE: usize = 13;

            let query_chars: Vec<char> = panel.query.chars().collect();
            let cursor_char = panel.cursor_char.min(query_chars.len());
            let view_start = cursor_char
                .saturating_sub(VISIBLE - 1)
                .min(query_chars.len().saturating_sub(VISIBLE));
            let view_end = (view_start + VISIBLE).min(query_chars.len());
            let display_query: String = query_chars[view_start..view_end].iter().collect();

            // Show an ellipsis at the left when the query is scrolled.
            let (query_display_text, query_x_offset) = if view_start > 0 {
                (
                    format!(
                        "…{}",
                        &display_query[display_query
                            .char_indices()
                            .nth(1)
                            .map(|(i, _)| i)
                            .unwrap_or(0)..]
                    ),
                    0.0f32,
                )
            } else {
                (display_query, 0.0f32)
            };

            add_text_verts(
                &query_display_text,
                text_y,
                panel_x + self.cell_w_px * (QUERY_TEXT_X + query_x_offset),
                [0.88, 0.92, 0.98, 1.0],
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );

            // ── Regex / case-sensitive flag indicators ─────────────────────────
            // [R] at cell 20.0, [Cc] at cell 23.0
            let dim_color = [0.45_f32, 0.45, 0.55, 1.0];
            let bright_color = [0.90_f32, 0.92, 1.0, 1.0];
            let regex_color = if panel.regex_mode {
                bright_color
            } else {
                dim_color
            };
            let case_color = if panel.case_sensitive {
                bright_color
            } else {
                dim_color
            };
            add_text_verts(
                "[R]",
                text_y,
                panel_x + self.cell_w_px * 20.0,
                regex_color,
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );
            add_text_verts(
                "[Cc]",
                text_y,
                panel_x + self.cell_w_px * 23.0,
                case_color,
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );

            // ── Match count or error message ─────────────────────────────────
            // Count at cell 27.0, format "N/M" (compact, no spaces).
            let count_or_err = if let Some(ref err) = panel.error {
                // Truncate regex error to fit the available space (~7 chars).
                let s: String = err.chars().take(7).collect();
                s
            } else {
                format!("{}/{}", panel.current_match, panel.match_count)
            };
            let count_color = if panel.error.is_some() {
                [1.0_f32, 0.45, 0.45, 1.0]
            } else {
                [0.72_f32, 0.80, 0.92, 1.0]
            };
            add_text_verts(
                &count_or_err,
                text_y,
                panel_x + self.cell_w_px * 27.0,
                count_color,
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );

            // ── Navigation / close button glyphs ─────────────────────────────
            for (i, glyph) in ['↑', '↓', '×'].iter().enumerate() {
                let bx = panel_x + panel_w - button_w * (3 - i) as f32;
                add_text_verts(
                    &glyph.to_string(),
                    text_y,
                    bx + self.cell_w_px * 0.55,
                    [0.92, 0.96, 1.0, 1.0],
                    &[],
                    &[],
                    &self.glyph_cache,
                    None,
                    self.cell_w_px,
                    self.cell_h_px,
                    self.size,
                    &mut text_verts,
                    0,
                );
            }
        }

        // Settings overlay text — rendered last, no scissor.
        let dropdown_text_vert_start = (text_verts.len() / 8) as u32;
        if let Some(ref dd) = snapshot.suggestion_dropdown {
            let th = &snapshot.theme;
            let n_visible = dd.items.len().saturating_sub(dd.scroll_offset).min(8);
            let visible_end = dd.scroll_offset + n_visible;
            let visible_selected = dd.selected.saturating_sub(dd.scroll_offset);
            let row_h = self.cell_h_px * 1.2;
            let panel_h = n_visible as f32 * row_h;
            let edit_top_px = (tab_bar_h + snapshot.split_ratio * available_h + 2.0).round();
            let panel_y_top_px = edit_top_px - panel_h;
            for (i, item) in dd.items[dd.scroll_offset..visible_end].iter().enumerate() {
                let row_y = panel_y_top_px + i as f32 * row_h + (row_h - self.cell_h_px) * 0.5;
                let color = if i == visible_selected {
                    th.text
                } else {
                    let [r, g, b, _] = th.text;
                    [r * 0.72, g * 0.72, b * 0.72, 0.9]
                };
                add_text_verts(
                    item,
                    row_y,
                    pad_h + self.cell_w_px,
                    color,
                    &[],
                    &[],
                    &self.glyph_cache,
                    None,
                    self.cell_w_px,
                    self.cell_h_px,
                    self.size,
                    &mut text_verts,
                    0,
                );
            }
        }
        let settings_text_vert_start = (text_verts.len() / 8) as u32;
        if let Some(ref overlay) = snapshot.settings_overlay
            && self.size.width > 0
            && self.size.height > 0
            && self.cell_w_px > 0.0
            && self.cell_h_px > 0.0
        {
            let th = &snapshot.theme;
            let win_w = self.size.width as f32;
            let win_h = self.size.height as f32;
            let title_h = self.cell_h_px * 2.2;
            let row_h = self.cell_h_px * 1.7;
            let footer_h = self.cell_h_px * 1.9;
            let edit_h = if overlay.editing.is_some() {
                self.cell_h_px * 1.8
            } else {
                0.0
            };
            let n_items = overlay.items.len() as f32;
            let panel_h = title_h + n_items * row_h + edit_h + footer_h;
            let panel_w = (self.cell_w_px * 72.0)
                .min(win_w * 0.92)
                .max(self.cell_w_px * 40.0);
            let panel_x0 = (win_w - panel_w) / 2.0;
            let panel_y0 = (win_h - panel_h) / 2.0;

            // Title
            let title_text = if overlay.just_saved {
                "  SETTINGS  \u{2713} Saved"
            } else {
                "  SETTINGS  (Cmd+,)"
            };
            let title_y = panel_y0 + (title_h - self.cell_h_px) / 2.0;
            add_text_verts(
                title_text,
                title_y,
                panel_x0 + self.cell_w_px,
                th.text,
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );

            // Rows
            let key_col = panel_x0 + self.cell_w_px * 1.5;
            let val_col = panel_x0 + panel_w * 0.50;

            // Pre-compute the flat (non-header) item index of the focused row.
            // This lets us skip rendering text for rows that are physically covered
            // by the search dropdown, which otherwise bleeds through the opaque BG
            // (all text is accumulated in one draw call, so order matters).
            let pre_focused_flat = {
                let mut ec = 0usize;
                let mut fi = 0usize;
                for (idx, itm) in overlay.items.iter().enumerate() {
                    if !itm.is_header {
                        if ec == overlay.cursor {
                            fi = idx;
                            break;
                        }
                        ec += 1;
                    }
                }
                fi
            };
            // Flat indices [pre_focused_flat+1 .. pre_focused_flat+n_visible] are
            // covered by the dropdown and must not have their text rendered.
            const SEARCH_MAX_VISIBLE: usize = 8;
            let search_cover_end = if overlay.search_buf.is_some() {
                let n_vis = overlay
                    .search_matches
                    .len()
                    .saturating_sub(overlay.search_scroll_offset)
                    .min(SEARCH_MAX_VISIBLE);
                pre_focused_flat + n_vis
            } else {
                0
            };

            let mut editable_idx = 0usize;
            let mut focused_flat_idx = 0usize;
            for (i, item) in overlay.items.iter().enumerate() {
                let row_y = panel_y0 + title_h + i as f32 * row_h + (row_h - self.cell_h_px) / 2.0;
                if item.is_header {
                    // Skip section headers covered by the search dropdown.
                    if overlay.search_buf.is_some() && i > pre_focused_flat && i <= search_cover_end
                    {
                        continue;
                    }
                    add_text_verts(
                        &item.key,
                        row_y,
                        key_col,
                        th.separator_focused,
                        &[],
                        &[],
                        &self.glyph_cache,
                        None,
                        self.cell_w_px,
                        self.cell_h_px,
                        self.size,
                        &mut text_verts,
                        0,
                    );
                } else {
                    let is_focused = editable_idx == overlay.cursor;
                    if is_focused {
                        focused_flat_idx = i;
                    }
                    // Increment before the potential early-continue so the cursor
                    // mapping stays correct even for visually-skipped rows.
                    editable_idx += 1;
                    // Skip rows hidden under the search dropdown.
                    if overlay.search_buf.is_some() && i > pre_focused_flat && i <= search_cover_end
                    {
                        continue;
                    }
                    let (key_color, val_color) = if is_focused {
                        (th.text, th.cursor)
                    } else {
                        (
                            {
                                let [r, g, b, _] = th.text;
                                [r * 0.85, g * 0.85, b * 0.85, 1.0_f32]
                            },
                            {
                                let [r, g, b, _] = th.cursor;
                                [r * 0.75, g * 0.85, b * 0.85, 0.85_f32]
                            },
                        )
                    };
                    add_text_verts(
                        &item.key,
                        row_y,
                        key_col,
                        key_color,
                        &[],
                        &[],
                        &self.glyph_cache,
                        None,
                        self.cell_w_px,
                        self.cell_h_px,
                        self.size,
                        &mut text_verts,
                        0,
                    );

                    // Build the display value string for the right column.
                    // Priority: active search > searchable hint > arrows (numeric) > freetext hint > plain value.
                    let search_val_buf: Option<String> =
                        if item.is_searchable && is_focused && overlay.search_buf.is_some() {
                            // Show the live search buffer with a "/" prompt and block cursor.
                            let sbuf = overlay.search_buf.as_deref().unwrap_or("");
                            Some(format!("/ {}\u{258e}", sbuf))
                        } else {
                            None
                        };
                    // Searchable fields (theme, font family): show "value /" to signal
                    // that Enter opens a live search. No arrows — it's a picker, not an incrementor.
                    let searchable_hint: Option<String> =
                        if item.is_searchable && search_val_buf.is_none() {
                            Some(format!("{} /", item.value))
                        } else {
                            None
                        };
                    // Numeric selectable fields show ← value → at all times (focused or not).
                    let arrows_buf: Option<String> = if item.is_selectable
                        && !item.is_searchable
                        && !item.is_action
                        && search_val_buf.is_none()
                        && !(is_focused && overlay.editing.is_some())
                    {
                        Some(format!("\u{2190} {} \u{2192}", item.value))
                    } else {
                        None
                    };
                    // Free-text fields get a dim cursor hint when focused and not yet editing.
                    let freetext_hint: Option<String> = if !item.is_selectable
                        && !item.is_searchable
                        && is_focused
                        && overlay.editing.is_none()
                    {
                        Some(format!("{}\u{258e}", item.value))
                    } else {
                        None
                    };
                    let display_val: &str = if let Some(ref s) = search_val_buf {
                        s.as_str()
                    } else if let Some(ref s) = searchable_hint {
                        s.as_str()
                    } else if let Some(ref s) = arrows_buf {
                        s.as_str()
                    } else if let Some(ref s) = freetext_hint {
                        s.as_str()
                    } else if is_focused {
                        if let Some(ref buf) = overlay.editing {
                            buf.as_str()
                        } else {
                            &item.value
                        }
                    } else {
                        &item.value
                    };
                    add_text_verts(
                        display_val,
                        row_y,
                        val_col,
                        val_color,
                        &[],
                        &[],
                        &self.glyph_cache,
                        None,
                        self.cell_w_px,
                        self.cell_h_px,
                        self.size,
                        &mut text_verts,
                        0,
                    );
                }
            }

            // Footer help text — hidden when the search dropdown is open to avoid
            // it bleeding through the dropdown (footer y falls inside the dropdown area
            // when the focused item is in the lower half of the panel).
            if overlay.search_buf.is_none() {
                let footer_y = panel_y0
                    + title_h
                    + n_items * row_h
                    + edit_h
                    + (footer_h - self.cell_h_px) / 2.0;
                let footer_text = if overlay.editing.is_some() {
                    "  Enter: confirm   Esc: cancel"
                } else {
                    "  \u{2191}\u{2193} navigate   \u{2190}\u{2192} change   Enter: edit/search   Esc: close & save"
                };
                add_text_verts(
                    footer_text,
                    footer_y,
                    panel_x0,
                    {
                        let [r, g, b, _] = th.text;
                        [r * 0.55, g * 0.55, b * 0.55, 0.90_f32]
                    },
                    &[],
                    &[],
                    &self.glyph_cache,
                    None,
                    self.cell_w_px,
                    self.cell_h_px,
                    self.size,
                    &mut text_verts,
                    0,
                );
            }

            // Search dropdown text — rendered on top of the dropdown background.
            if overlay.search_buf.is_some() {
                const SEARCH_MAX_VISIBLE: usize = 8;
                let n_visible = overlay
                    .search_matches
                    .len()
                    .saturating_sub(overlay.search_scroll_offset)
                    .min(SEARCH_MAX_VISIBLE);
                let visible_end = overlay.search_scroll_offset + n_visible;
                let vis_sel = overlay
                    .search_selected
                    .saturating_sub(overlay.search_scroll_offset);
                let drop_top_px = panel_y0 + title_h + (focused_flat_idx + 1) as f32 * row_h;
                for (i, match_str) in overlay.search_matches
                    [overlay.search_scroll_offset..visible_end]
                    .iter()
                    .enumerate()
                {
                    let item_y = drop_top_px + i as f32 * row_h + (row_h - self.cell_h_px) / 2.0;
                    let is_sel = i == vis_sel;
                    let color = if is_sel {
                        th.text
                    } else {
                        let [r, g, b, _] = th.text;
                        // Dim non-selected items significantly for clear contrast with selected.
                        [r * 0.60, g * 0.60, b * 0.60, 1.0]
                    };
                    // ▶ marker on the selected row; matching-width indent on others.
                    let labeled = if is_sel {
                        format!("\u{25b6} {}", match_str)
                    } else {
                        format!("  {}", match_str)
                    };
                    add_text_verts(
                        &labeled,
                        item_y,
                        key_col,
                        color,
                        &[],
                        &[],
                        &self.glyph_cache,
                        None,
                        self.cell_w_px,
                        self.cell_h_px,
                        self.size,
                        &mut text_verts,
                        0,
                    );
                }
                // "no results" hint when the query matched nothing.
                if overlay.search_matches.is_empty() {
                    let item_y = drop_top_px + (row_h - self.cell_h_px) / 2.0;
                    add_text_verts(
                        "(no results)",
                        item_y,
                        key_col,
                        {
                            let [r, g, b, _] = th.text;
                            [r * 0.45, g * 0.45, b * 0.45, 0.70]
                        },
                        &[],
                        &[],
                        &self.glyph_cache,
                        None,
                        self.cell_w_px,
                        self.cell_h_px,
                        self.size,
                        &mut text_verts,
                        0,
                    );
                }
            }
        }

        // Scroll-up indicator text — a small centred pill label when scrollback
        // is active.  Rendered after settings overlay text but before toasts.
        let scroll_indicator_text_vert_start = (text_verts.len() / 8) as u32;
        if snapshot.scroll_offset > 0
            && self.size.width > 0
            && self.size.height > 0
            && self.cell_w_px > 0.0
            && self.cell_h_px > 0.0
        {
            let label = format!("↑  {} lines  ↑", snapshot.scroll_offset);
            let pill_h_px = self.cell_h_px * 1.4;
            let margin_px = self.cell_h_px * 0.5;
            let term_bottom_px =
                tab_bar_h + snapshot.split_ratio * (self.size.height as f32 - tab_bar_h);
            let bottom_px = term_bottom_px - margin_px;
            let top_px = bottom_px - pill_h_px;
            let text_y = top_px + (pill_h_px - self.cell_h_px) / 2.0;
            let n_chars = label.chars().count() as f32;
            let text_w_px = n_chars * self.cell_w_px;
            let text_x = (self.size.width as f32 - text_w_px) / 2.0;
            add_text_verts(
                &label,
                text_y,
                text_x,
                [0.60, 0.85, 1.00, 1.0],
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );
        }

        // Command palette text — rendered after scroll indicator, before toasts.
        let command_palette_text_vert_start = (text_verts.len() / 8) as u32;
        if let Some(ref cp) = snapshot.command_palette
            && self.size.width > 0
            && self.size.height > 0
            && self.cell_w_px > 0.0
            && self.cell_h_px > 0.0
        {
            let palette_w_px = self.cell_w_px * 50.0;
            let header_h_px = self.cell_h_px * 2.2;
            let item_h_px = self.cell_h_px * 1.4;
            let cx = self.size.width as f32 / 2.0;
            let y0_px = tab_bar_h + self.size.height as f32 * 0.08;
            let left_px = cx - palette_w_px / 2.0;
            let pad_px = self.cell_w_px * 1.5;

            // Header: "> query_text" with a blinking cursor marker
            let query_display = if cp.query.is_empty() {
                "  Search commands…".to_owned()
            } else {
                format!("  > {}", cp.query)
            };
            let header_text_y = y0_px + (header_h_px - self.cell_h_px) / 2.0;
            add_text_verts(
                &query_display,
                header_text_y,
                left_px + pad_px,
                [0.88, 0.92, 1.00, 1.0],
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );

            // Item rows
            let n_visible = cp.items.len().saturating_sub(cp.scroll_offset).min(10);
            let visible_end = cp.scroll_offset + n_visible;
            for (i, item) in cp.items[cp.scroll_offset..visible_end].iter().enumerate() {
                let row_top_px = y0_px + header_h_px + i as f32 * item_h_px;
                let text_y = row_top_px + (item_h_px - self.cell_h_px) / 2.0;
                let abs_idx = cp.scroll_offset + i;
                let color = if abs_idx == cp.selected {
                    [1.00, 1.00, 1.00, 1.0]
                } else {
                    [0.65, 0.72, 0.85, 1.0]
                };
                add_text_verts(
                    item,
                    text_y,
                    left_px + pad_px,
                    color,
                    &[],
                    &[],
                    &self.glyph_cache,
                    None,
                    self.cell_w_px,
                    self.cell_h_px,
                    self.size,
                    &mut text_verts,
                    0,
                );
            }
        }

        // Toast text — rendered after settings overlay so toasts appear on top.
        let toast_text_vert_start = (text_verts.len() / 8) as u32;
        if !snapshot.toast_stack.is_empty()
            && self.size.height > 0
            && self.cell_w_px > 0.0
            && self.cell_h_px > 0.0
        {
            use crate::types::ToastKind;
            let toast_h_px = self.cell_h_px * 1.5;
            let toast_margin_px = self.cell_h_px * 0.35;
            let toast_pad_h_px = self.cell_w_px * 1.2;
            let win_h_px = self.size.height as f32;
            let win_w_px = self.size.width as f32;

            for (rev_idx, toast) in snapshot.toast_stack.iter().rev().enumerate() {
                let max_chars = toast.text.chars().count().max(4) as f32;
                let toast_w_px =
                    (max_chars * self.cell_w_px + toast_pad_h_px * 2.0).min(win_w_px * 0.45);
                let bottom_edge_px =
                    win_h_px - toast_margin_px - rev_idx as f32 * (toast_h_px + toast_margin_px);
                let text_y = bottom_edge_px - toast_h_px + (toast_h_px - self.cell_h_px) * 0.5;
                let right_edge_px = win_w_px - toast_margin_px;
                let text_x = right_edge_px - toast_w_px + toast_pad_h_px;

                let text_color: [f32; 4] = match toast.kind {
                    ToastKind::Info => [0.88, 0.92, 1.00, 1.0],
                    ToastKind::Success => [0.40, 1.00, 0.50, 1.0],
                    ToastKind::Warn => [1.00, 0.85, 0.30, 1.0],
                    ToastKind::Error => [1.00, 0.40, 0.40, 1.0],
                };
                add_text_verts(
                    &toast.text,
                    text_y,
                    text_x,
                    text_color,
                    &[],
                    &[],
                    &self.glyph_cache,
                    None,
                    self.cell_w_px,
                    self.cell_h_px,
                    self.size,
                    &mut text_verts,
                    0,
                );
            }
        }

        // Update banner text is rendered last so it stays in front of the
        // rest of the UI and remains visible as a top-of-window panel.
        if let Some(ref overlay_text) = snapshot.resize_overlay
            && self.size.width > 0
            && self.size.height > 0
            && self.cell_w_px > 0.0
            && self.cell_h_px > 0.0
        {
            let n_chars = overlay_text.chars().count() as f32;
            let text_w_px = n_chars * self.cell_w_px;
            let tab_bar_h = if !snapshot.tab_labels.is_empty() {
                self.cell_h_px
            } else {
                0.0
            };
            let x_start = (self.size.width as f32 - text_w_px) / 2.0;
            let y_start = tab_bar_h + self.cell_h_px;
            add_text_verts(
                overlay_text,
                y_start,
                x_start,
                [1.0, 1.0, 1.0, 1.0],
                &[],
                &[],
                &self.glyph_cache,
                None,
                self.cell_w_px,
                self.cell_h_px,
                self.size,
                &mut text_verts,
                0,
            );
        }

        let total_vert_count = (text_verts.len() / 8) as u32;
        if !text_verts.is_empty() {
            let bytes = floats_as_bytes(&text_verts);
            let cap = TEXT_VERTEX_BUF_CAPACITY as usize;
            self.queue
                .write_buffer(&self.text_vertex_buf, 0, &bytes[..bytes.len().min(cap)]);
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("teletipo-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: (self.theme.terminal_bg[0] as f64
                                + if snapshot.bell_active { 0.12 } else { 0.0 })
                            .min(1.0),
                            g: self.theme.terminal_bg[1] as f64,
                            b: self.theme.terminal_bg[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            if panel_vertex_count > 0 {
                pass.draw(0..panel_vertex_count, 0..1);
            }
            if total_vert_count > 0 {
                let cap_verts = (TEXT_VERTEX_BUF_CAPACITY / 32) as u32;
                pass.set_pipeline(&self.text_pipeline);
                pass.set_bind_group(0, &self.atlas_bind_group, &[]);
                pass.set_vertex_buffer(0, self.text_vertex_buf.slice(..));

                // Terminal scissor: from bottom of tab bar to separator.
                let term_end = terminal_vert_count.min(cap_verts);
                if term_end > 0 {
                    let term_scissor_h = split_y_px.saturating_sub(tab_bar_h_u32);
                    if term_scissor_h > 0 {
                        pass.set_scissor_rect(0, tab_bar_h_u32, self.size.width, term_scissor_h);
                        pass.draw(0..term_end, 0..1);
                    }
                }

                // Editor scissor: from separator to bottom of window minus bottom padding.
                let total_capped = total_vert_count.min(cap_verts);
                let editor_start = terminal_vert_count.min(total_capped);
                if total_capped > editor_start {
                    let editor_pane_h = self
                        .size
                        .height
                        .saturating_sub(split_y_px)
                        .saturating_sub(snapshot.padding_v);
                    if editor_pane_h > 0 {
                        pass.set_scissor_rect(0, split_y_px, self.size.width, editor_pane_h);
                        pass.draw(editor_start..tab_text_vert_start.min(total_capped), 0..1);
                    }
                }

                // Tab label + × + "+" text: scissored to the tab bar strip.
                let tab_end = context_text_vert_start.min(total_capped);
                if tab_bar_h_u32 > 0 && tab_end > tab_text_vert_start {
                    pass.set_scissor_rect(0, 0, self.size.width, tab_bar_h_u32);
                    pass.draw(tab_text_vert_start.min(tab_end)..tab_end, 0..1);
                }

                // Context menu text: no scissor clipping (menu floats above all panes).
                if total_capped > context_text_vert_start {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.draw(
                        context_text_vert_start.min(total_capped)
                            ..dropdown_text_vert_start.min(total_capped),
                        0..1,
                    );
                }

                // Suggestion dropdown background — drawn after main panel bg so it
                // sits on top of the terminal/editor backgrounds.
                if dropdown_bg_count > 0 {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.set_pipeline(&self.pipeline);
                    pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                    pass.draw(
                        dropdown_bg_start..dropdown_bg_start + dropdown_bg_count,
                        0..1,
                    );
                    // Restore text pipeline for dropdown text below.
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(0, &self.atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.text_vertex_buf.slice(..));
                }

                // Suggestion dropdown text: no scissor.
                if total_capped > dropdown_text_vert_start {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.draw(
                        dropdown_text_vert_start.min(total_capped)
                            ..settings_text_vert_start.min(total_capped),
                        0..1,
                    );
                }

                // Settings overlay: draw its background (full-screen dim + panel) AFTER all
                // terminal/editor text so the panel covers the content cleanly.
                if overlay_bg_count > 0 {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.set_pipeline(&self.pipeline);
                    pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                    pass.draw(overlay_bg_start..overlay_bg_start + overlay_bg_count, 0..1);
                    // Restore text pipeline for settings text below.
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(0, &self.atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.text_vertex_buf.slice(..));
                }

                // Settings overlay text: on top of the panel background.
                if scroll_indicator_text_vert_start > settings_text_vert_start {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.draw(
                        settings_text_vert_start.min(total_capped)
                            ..scroll_indicator_text_vert_start.min(total_capped),
                        0..1,
                    );
                }

                // Scroll-up indicator background pill.
                if scroll_indicator_bg_count > 0 {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.set_pipeline(&self.pipeline);
                    pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                    pass.draw(
                        scroll_indicator_bg_start
                            ..scroll_indicator_bg_start + scroll_indicator_bg_count,
                        0..1,
                    );
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(0, &self.atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.text_vertex_buf.slice(..));
                }

                // Scroll-up indicator text.
                if command_palette_text_vert_start > scroll_indicator_text_vert_start {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.draw(
                        scroll_indicator_text_vert_start.min(total_capped)
                            ..command_palette_text_vert_start.min(total_capped),
                        0..1,
                    );
                }

                // Command palette background.
                if command_palette_bg_count > 0 {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.set_pipeline(&self.pipeline);
                    pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                    pass.draw(
                        command_palette_bg_start
                            ..command_palette_bg_start + command_palette_bg_count,
                        0..1,
                    );
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(0, &self.atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.text_vertex_buf.slice(..));
                }

                // Command palette text.
                if toast_text_vert_start > command_palette_text_vert_start {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.draw(
                        command_palette_text_vert_start.min(total_capped)
                            ..toast_text_vert_start.min(total_capped),
                        0..1,
                    );
                }

                // Toast backgrounds — drawn last so they appear on top of settings overlay.
                if toast_bg_count > 0 {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.set_pipeline(&self.pipeline);
                    pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                    pass.draw(toast_bg_start..toast_bg_start + toast_bg_count, 0..1);
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(0, &self.atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.text_vertex_buf.slice(..));
                }

                // Toast text — on top of toast backgrounds.
                if total_capped > toast_text_vert_start {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.draw(toast_text_vert_start.min(total_capped)..total_capped, 0..1);
                }

                pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        Ok(())
    }
}
