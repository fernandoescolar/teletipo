#![allow(dead_code, unused_variables)]

use std::mem::size_of;
use std::sync::Arc;

use crate::{ColorTheme, KeybindingsOverlay, RenderSnapshot, SCROLLBAR_W_PX, SettingsOverlay};
use font8x8::UnicodeFonts;
use glow::HasContext;
use render_model::{
    CellMetrics, FrameLayout, Rect, RenderCommand, RenderTarget, Scene, compute_frame_layout,
};
use winit::dpi::PhysicalSize;

use crate::backend::{BatchContainer, GpuState};
use crate::emoji_atlas::ColorAtlas;
use crate::font::CpuFontRasterizer;
use crate::glyph_atlas::{AtlasGlyph, GlyphAtlas};
use crate::shaders::{compile_atlas_program, compile_color_atlas_program, compile_program};
use crate::types::{
    ATLAS_TEX_SIZE, COLOR_ATLAS_TEX_SIZE, ColorAtlasEntry, GlyphBitmap,
    SETTINGS_MAX_VISIBLE_SEARCH, STYLE_BOLD, STYLE_DIM, STYLE_ITALIC, STYLE_STRIKE, ShapedLines,
    ShapedTerminalCache, clamp_color, mix_color,
};
use crate::util::{
    char_col_width, editor_offset_to_row_col, hash_text, is_icon_like, normalize_rect_selection,
};

type Result<T> = anyhow::Result<T>;

fn frosted_backdrop_alpha(opacity: f32) -> f32 {
    // Keep backgrounds translucent but not crystal-clear when opacity is low.
    // This approximates a blur/frosted effect on compositors without real blur.
    let opacity = opacity.clamp(0.0, 1.0);
    0.55 + 0.45 * opacity
}

#[allow(dead_code)]
fn with_backdrop_alpha(mut color: [f32; 4], opacity: f32) -> [f32; 4] {
    color[3] = (color[3] * frosted_backdrop_alpha(opacity)).clamp(0.0, 1.0);
    color
}

/// Convert TextStyle struct to style bits.
fn style_to_bits(style: &render_model::TextStyle) -> u8 {
    let mut bits = 0u8;
    if style.bold {
        bits |= STYLE_BOLD;
    }
    if style.italic {
        bits |= STYLE_ITALIC;
    }
    if style.dim {
        bits |= STYLE_DIM;
    }
    if style.strike {
        bits |= STYLE_STRIKE;
    }
    bits
}

// ── GlPainter struct ──────────────────────────────────────────────────────────

pub(crate) struct GlPainter {
    // ── Backend GPU infrastructure ─────────────────────────────────────────
    gpu_state: GpuState,
    batches: BatchContainer,

    // ── Atlas allocators and caches ────────────────────────────────────────
    glyph_atlas: GlyphAtlas,
    emoji_atlas: ColorAtlas,

    // ── Font rendering state ───────────────────────────────────────────────
    rasterizer: CpuFontRasterizer,
    shaped_terminal_cache: Option<ShapedTerminalCache>,

    // ── Clipping state ─────────────────────────────────────────────────────
    clip_stack: Vec<(i32, i32, i32, i32)>, // (x, y, w, h) in GL coordinates

    // ── GPU context recovery ───────────────────────────────────────────────
    /// Last frame render time. Used to detect long idle periods where macOS
    /// may have evicted the glyph atlas texture from GPU memory.
    last_render_at: Option<std::time::Instant>,
}

struct GlyphCell {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [f32; 4],
    style: u8,
}

struct SettingsPanelGeom {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    title_h: f32,
    row_h: f32,
    edit_h: f32,
    footer_h: f32,
    key_col: f32,
    val_col: f32,
}

struct SettingsPanelColors {
    bg: [f32; 4],
    border: [f32; 4],
    title: [f32; 4],
    section: [f32; 4],
    select: [f32; 4],
    edit: [f32; 4],
}

struct KeybindingsPanelGeom {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    title_h: f32,
    row_h: f32,
    footer_h: f32,
    key_col: f32,
    bind_col: f32,
}

struct KeybindingsPanelColors {
    bg: [f32; 4],
    row_alt: [f32; 4],
    select: [f32; 4],
    record: [f32; 4],
    border: [f32; 4],
    title: [f32; 4],
}

impl GlPainter {
    pub(crate) fn new(
        gl: &glow::Context,
        font_family: Option<String>,
        font_size_px: f32,
    ) -> Result<Self> {
        let program = compile_program(gl)?;
        let vbo = unsafe { gl.create_buffer() }
            .map_err(|err| anyhow::anyhow!("create GL buffer: {err}"))?;
        let vao = unsafe { gl.create_vertex_array() }
            .map_err(|err| anyhow::anyhow!("create GL vertex array: {err}"))?;
        let u_screen = unsafe { gl.get_uniform_location(program, "u_screen") };

        unsafe {
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));

            let stride = (6 * size_of::<f32>()) as i32;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(
                1,
                4,
                glow::FLOAT,
                false,
                stride,
                (2 * size_of::<f32>()) as i32,
            );

            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_vertex_array(None);
        }

        // ── Atlas (textured) pipeline ─────────────────────────────────────
        let atlas_program = compile_atlas_program(gl)?;
        let atlas_vbo = unsafe { gl.create_buffer() }
            .map_err(|err| anyhow::anyhow!("create atlas GL buffer: {err}"))?;
        let atlas_vao = unsafe { gl.create_vertex_array() }
            .map_err(|err| anyhow::anyhow!("create atlas GL vertex array: {err}"))?;
        let atlas_u_screen = unsafe { gl.get_uniform_location(atlas_program, "u_screen") };
        let atlas_u_sampler = unsafe { gl.get_uniform_location(atlas_program, "u_atlas") };

        unsafe {
            gl.bind_vertex_array(Some(atlas_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(atlas_vbo));
            // layout: x(2) y(0) u(2) v(0) r(4) g b a  →  8 floats per vertex
            let atlas_stride = (8 * size_of::<f32>()) as i32;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, atlas_stride, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(
                1,
                2,
                glow::FLOAT,
                false,
                atlas_stride,
                (2 * size_of::<f32>()) as i32,
            );
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(
                2,
                4,
                glow::FLOAT,
                false,
                atlas_stride,
                (4 * size_of::<f32>()) as i32,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_vertex_array(None);
        }

        // Single-channel (GL_RED) 1024×1024 atlas texture
        let atlas_texture = unsafe { gl.create_texture() }
            .map_err(|err| anyhow::anyhow!("create atlas texture: {err}"))?;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(atlas_texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RED as i32,
                ATLAS_TEX_SIZE as i32,
                ATLAS_TEX_SIZE as i32,
                0,
                glow::RED,
                glow::UNSIGNED_BYTE,
                None,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }

        // ── Color-emoji RGBA atlas pipeline ──────────────────────────────
        let color_atlas_program = compile_color_atlas_program(gl)?;
        let color_atlas_vbo = unsafe { gl.create_buffer() }
            .map_err(|err| anyhow::anyhow!("create color atlas GL buffer: {err}"))?;
        let color_atlas_vao = unsafe { gl.create_vertex_array() }
            .map_err(|err| anyhow::anyhow!("create color atlas GL vertex array: {err}"))?;
        let color_atlas_u_screen =
            unsafe { gl.get_uniform_location(color_atlas_program, "u_screen") };
        let color_atlas_u_sampler =
            unsafe { gl.get_uniform_location(color_atlas_program, "u_atlas") };

        unsafe {
            gl.bind_vertex_array(Some(color_atlas_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(color_atlas_vbo));
            let stride = (8 * size_of::<f32>()) as i32;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(
                1,
                2,
                glow::FLOAT,
                false,
                stride,
                (2 * size_of::<f32>()) as i32,
            );
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(
                2,
                4,
                glow::FLOAT,
                false,
                stride,
                (4 * size_of::<f32>()) as i32,
            );
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.bind_vertex_array(None);
        }

        let color_atlas_texture = unsafe { gl.create_texture() }
            .map_err(|err| anyhow::anyhow!("create color atlas texture: {err}"))?;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(color_atlas_texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                COLOR_ATLAS_TEX_SIZE as i32,
                COLOR_ATLAS_TEX_SIZE as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                None,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::LINEAR as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }

        // Create GPU state
        let gpu_state = GpuState::new(
            program,
            vbo,
            vao,
            u_screen,
            atlas_texture,
            atlas_program,
            atlas_vbo,
            atlas_vao,
            atlas_u_screen,
            atlas_u_sampler,
            color_atlas_texture,
            color_atlas_program,
            color_atlas_vbo,
            color_atlas_vao,
            color_atlas_u_screen,
            color_atlas_u_sampler,
        );

        // Create batches and atlases
        let batches = BatchContainer::new();
        let glyph_atlas = GlyphAtlas::new(ATLAS_TEX_SIZE);
        let emoji_atlas = ColorAtlas::new(COLOR_ATLAS_TEX_SIZE);

        Ok(Self {
            gpu_state,
            batches,
            glyph_atlas,
            emoji_atlas,
            rasterizer: CpuFontRasterizer::new(font_family, font_size_px),
            shaped_terminal_cache: None,
            clip_stack: Vec::new(),
            last_render_at: None,
        })
    }

    pub(crate) fn set_font_size(&mut self, font_size_px: f32) {
        let old_size = self.rasterizer.font_size_px;
        self.rasterizer.set_font_size(font_size_px);
        self.shaped_terminal_cache = None;
        // If the size actually changed, glyph bitmaps are now wrong size.
        if (old_size - font_size_px).abs() >= 0.5 {
            self.reset_text_atlas_state();
        }
    }

    fn reset_text_atlas_state(&mut self) {
        self.glyph_atlas.clear();
        self.emoji_atlas.clear();
    }

    /// Force atlas repack/reupload on next frame.
    ///
    /// Useful after display resume/context hiccups where GL textures may lose
    /// their texel contents while CPU-side cache still thinks glyphs exist.
    pub(crate) fn invalidate_text_atlases(&mut self, gl: &glow::Context) {
        self.reset_text_atlas_state();
        self.clear_atlas_textures(gl);
        self.shaped_terminal_cache = None;
    }

    /// Clear both glyph atlases after a DPI/font-size jump so stale texels do
    /// not bleed into newly packed glyphs when linear filtering is enabled.
    pub(crate) fn clear_atlas_textures(&self, gl: &glow::Context) {
        let mono_clear = vec![0_u8; (ATLAS_TEX_SIZE * ATLAS_TEX_SIZE) as usize];
        let color_clear = vec![0_u8; (COLOR_ATLAS_TEX_SIZE * COLOR_ATLAS_TEX_SIZE * 4) as usize];

        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.gpu_state.glyph.texture));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                ATLAS_TEX_SIZE as i32,
                ATLAS_TEX_SIZE as i32,
                glow::RED,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(&mono_clear),
            );

            gl.bind_texture(glow::TEXTURE_2D, Some(self.gpu_state.emoji.texture));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                COLOR_ATLAS_TEX_SIZE as i32,
                COLOR_ATLAS_TEX_SIZE as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(&color_clear),
            );

            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    pub(crate) fn cell_metrics(&self) -> (f32, f32) {
        self.rasterizer.cell_metrics()
    }

    fn shape_terminal_lines_cached(
        &mut self,
        snapshot: &RenderSnapshot,
        terminal_text: &str,
    ) -> Option<Arc<ShapedLines>> {
        let text_hash = hash_text(terminal_text);
        if let Some((cached_version, cached_hash, cached_lines)) =
            self.shaped_terminal_cache.as_ref()
            && *cached_version == snapshot.terminal_screen_version
            && *cached_hash == text_hash
        {
            return Some(Arc::clone(cached_lines));
        }

        let shaped = self.rasterizer.shape_terminal_text(terminal_text)?;
        let shaped = Arc::new(shaped);
        self.shaped_terminal_cache = Some((
            snapshot.terminal_screen_version,
            text_hash,
            Arc::clone(&shaped),
        ));
        Some(shaped)
    }

    pub(crate) fn render(
        &mut self,
        gl: &glow::Context,
        snapshot: &RenderSnapshot,
        size: PhysicalSize<u32>,
        cell_w_px: f32,
        cell_h_px: f32,
    ) {
        let target = RenderTarget::new(size.width as f32, size.height as f32);
        let metrics = CellMetrics::new(cell_w_px, cell_h_px);
        let layout = compute_frame_layout(snapshot, target, metrics);

        // Detect long idle periods (macOS may evict GPU textures during idle).
        // If more than 2 seconds since last render, invalidate atlas to force
        // full re-upload of glyph bitmaps to GPU.
        let now = std::time::Instant::now();
        if let Some(last) = self.last_render_at {
            if now.duration_since(last) > std::time::Duration::from_secs(2) {
                self.invalidate_text_atlases(gl);
            }
        }
        self.last_render_at = Some(now);

        // Clear batch structures
        self.batches.flat.clear();
        self.batches.glyph.clear();
        self.batches.emoji.clear();

        self.warm_atlas(gl, snapshot);

        // Build scene with background, tab bar, terminal background, and editor background
        let mut scene = render_model::build_scene(snapshot, &layout, target, metrics);

        // Add geometry overlays and toast notifications to the scene
        let ctx = render_model::RenderContext::new(snapshot, &layout, target, metrics);
        render_model::overlay::render_resize(&ctx, &mut scene);
        render_model::overlay::render_scroll_indicator(&ctx, &mut scene);
        render_model::components::render_toasts(&ctx, &mut scene);

        // Phase 1: Core components to Scene
        render_model::components::render_highlights(&ctx, &mut scene);
        render_model::components::render_selection(&ctx, &mut scene);
        render_model::components::render_cursor(&ctx, &mut scene);
        render_model::components::render_scrollbar(&ctx, &mut scene);
        render_model::components::render_suggestion(&ctx, &mut scene);

        // Phase 2: Overlay components to Scene
        render_model::components::render_tab_bar(&ctx, &mut scene);
        render_model::components::render_search_panel(&ctx, &mut scene);
        render_model::components::render_command_palette(&ctx, &mut scene);
        render_model::components::render_context_menu(&ctx, &mut scene);
        render_model::components::render_dropdown(&ctx, &mut scene);
        render_model::components::render_settings_overlay(&ctx, &mut scene);
        render_model::components::render_keybindings_overlay(&ctx, &mut scene);

        // Render scene geometry (backgrounds, rectangles, text)
        self.render_scene(gl, &scene, metrics, layout.width, layout.height);

        // Flush main-content passes before drawing overlays so that overlay
        // backgrounds (drawn without blending) completely cover terminal text.
        self.flush_passes(gl, layout.width, layout.height);

        // Toasts, resize overlay, and scroll indicator are now emitted via Scene

        self.flush_passes(gl, layout.width, layout.height);
    }

    /// Push a clipping rectangle (scissor test).
    fn push_clip_rect(&mut self, gl: &glow::Context, rect: &Rect, viewport_height: f32) {
        // Convert from Teletipo coords (origin top-left) to GL coords (origin bottom-left)
        let gl_x = rect.x as i32;
        let gl_y = (viewport_height - rect.y - rect.h) as i32;
        let gl_w = rect.w as i32;
        let gl_h = rect.h as i32;

        // Clamp to valid scissor bounds
        let gl_x = gl_x.max(0);
        let gl_y = gl_y.max(0);
        let gl_w = gl_w.max(0);
        let gl_h = gl_h.max(0);

        self.clip_stack.push((gl_x, gl_y, gl_w, gl_h));

        unsafe {
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(gl_x, gl_y, gl_w, gl_h);
        }
    }

    /// Pop the clipping rectangle.
    fn pop_clip_rect(&mut self, gl: &glow::Context) {
        self.clip_stack.pop();

        if let Some((x, y, w, h)) = self.clip_stack.last() {
            unsafe {
                gl.scissor(*x, *y, *w, *h);
            }
        } else {
            unsafe {
                gl.disable(glow::SCISSOR_TEST);
            }
        }
    }

    /// Render a simple text command (overlay text, no complex shaping).
    /// Assumes monospace rendering and basic style (bold, dim, etc).
    /// Add terminal text backgrounds to the scene.
    /// Emits rectangular backgrounds per cell based on terminal_bg_colors.
    fn add_terminal_text_to_scene(
        &mut self,
        scene: &mut Scene,
        snapshot: &RenderSnapshot,
        layout: &FrameLayout,
        _metrics: CellMetrics,
    ) {
        let terminal_text = snapshot.terminal_text_from_rows();
        let backdrop = frosted_backdrop_alpha(snapshot.opacity);
        let max_x = layout.width - layout.padding_h;
        let max_y = layout.terminal_text_bottom;
        let lines: Vec<&str> = terminal_text.lines().collect();
        let mut line_char_start = 0usize;

        // Emit background colors to scene
        for (row, line) in lines.iter().copied().enumerate() {
            let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;
            if y >= max_y {
                break;
            }
            for (col, _) in line.chars().enumerate() {
                let x = layout.padding_h + col as f32 * layout.cell_w_px;
                if x + layout.cell_w_px > max_x {
                    break;
                }

                let idx = line_char_start + col;
                if let Some(bg) = snapshot.terminal_bg_colors.get(idx).and_then(|c| *c) {
                    scene.rect_to_layer(
                        render_model::SceneLayer::Main,
                        x,
                        y,
                        layout.cell_w_px,
                        layout.cell_h_px,
                        [bg[0], bg[1], bg[2], backdrop],
                    );
                }
            }

            line_char_start = line_char_start.saturating_add(line.chars().count() + 1);
        }
    }

    /// Add editor text backgrounds to the scene.
    /// Similar to terminal but for the editor pane.
    fn add_editor_text_to_scene(
        &mut self,
        scene: &mut Scene,
        snapshot: &RenderSnapshot,
        layout: &FrameLayout,
        metrics: CellMetrics,
    ) {
        if snapshot.editor_text.is_empty() {
            return;
        }

        let backdrop = frosted_backdrop_alpha(snapshot.opacity);
        let lines: Vec<&str> = snapshot.editor_text.lines().collect();
        let max_x = layout.width - layout.padding_h;
        let max_y = layout.height - layout.padding_v;
        let mut line_char_start = 0usize;

        // Emit background colors to scene
        for (row, line) in lines.iter().copied().enumerate() {
            let y = layout.editor_top + row as f32 * layout.cell_h_px;
            if y >= max_y {
                break;
            }
            for (col, _) in line.chars().enumerate() {
                let x = layout.padding_h + col as f32 * layout.cell_w_px;
                if x + layout.cell_w_px > max_x {
                    break;
                }

                let idx = line_char_start + col;
                if let Some(bg) = snapshot.editor_fg_colors.get(idx).and_then(|c| *c) {
                    // Note: using fg_colors as bg is intentional for now (simplified rendering)
                    scene.rect_to_layer(
                        render_model::SceneLayer::Main,
                        x,
                        y,
                        layout.cell_w_px,
                        layout.cell_h_px,
                        [bg[0] * 0.15, bg[1] * 0.15, bg[2] * 0.15, backdrop * 0.3],
                    );
                }
            }

            line_char_start = line_char_start.saturating_add(line.chars().count() + 1);
        }
    }

    /// Render terminal text with font shaping (ligatures, complex scripts).
    /// Uses rustybuzz shaping for correct rendering of complex text.
    fn draw_terminal_text(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let terminal_text = snapshot.terminal_text_from_rows();
        let fallback_fg = [
            snapshot.theme.text[0],
            snapshot.theme.text[1],
            snapshot.theme.text[2],
            1.0,
        ];
        let max_x = layout.width - layout.padding_h;
        let max_y = layout.terminal_text_bottom;
        let lines: Vec<&str> = terminal_text.lines().collect();
        let shaped_lines = self.shape_terminal_lines_cached(snapshot, &terminal_text);
        let mut line_char_start = 0usize;

        for (row, line) in lines.iter().copied().enumerate() {
            let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;
            if y >= max_y {
                break;
            }

            if let Some(shaped) = shaped_lines.as_ref().and_then(|all| all.get(row)) {
                for sg in shaped {
                    let x = layout.padding_h + sg.col as f32 * layout.cell_w_px;
                    let w = layout.cell_w_px * sg.span_cols as f32;
                    if x + w > max_x {
                        continue;
                    }

                    let style = snapshot
                        .terminal_styles
                        .get(sg.full_char_idx)
                        .copied()
                        .unwrap_or(0);
                    let fg = snapshot
                        .terminal_fg_colors
                        .get(sg.full_char_idx)
                        .and_then(|c| *c)
                        .map(|c| [c[0], c[1], c[2], 1.0])
                        .unwrap_or_else(|| {
                            if style & STYLE_DIM != 0 {
                                [
                                    fallback_fg[0] * 0.55,
                                    fallback_fg[1] * 0.55,
                                    fallback_fg[2] * 0.55,
                                    1.0,
                                ]
                            } else {
                                fallback_fg
                            }
                        });

                    if sg.glyph_id == 0 {
                        if !self.push_color_emoji(sg.source_char, x, y, w, layout.cell_h_px) {
                            self.push_glyph_styled(
                                sg.source_char,
                                &GlyphCell {
                                    x,
                                    y,
                                    w,
                                    h: layout.cell_h_px,
                                    color: fg,
                                    style,
                                },
                            );
                        }
                    } else if !self.push_shaped_glyph(
                        sg.source_char,
                        sg.glyph_id,
                        &GlyphCell {
                            x,
                            y,
                            w,
                            h: layout.cell_h_px,
                            color: fg,
                            style,
                        },
                        sg.x_offset_px,
                        sg.y_offset_px,
                    ) {
                        self.push_glyph_styled(
                            sg.source_char,
                            &GlyphCell {
                                x,
                                y,
                                w,
                                h: layout.cell_h_px,
                                color: fg,
                                style,
                            },
                        );
                    }
                }
            } else {
                for (col, ch) in line.chars().enumerate() {
                    let x = layout.padding_h + col as f32 * layout.cell_w_px;
                    if x + layout.cell_w_px > max_x {
                        break;
                    }

                    let idx = line_char_start + col;
                    let style = snapshot.terminal_styles.get(idx).copied().unwrap_or(0);
                    let fg = snapshot
                        .terminal_fg_colors
                        .get(idx)
                        .and_then(|c| *c)
                        .map(|c| [c[0], c[1], c[2], 1.0])
                        .unwrap_or_else(|| {
                            if style & STYLE_DIM != 0 {
                                [
                                    fallback_fg[0] * 0.55,
                                    fallback_fg[1] * 0.55,
                                    fallback_fg[2] * 0.55,
                                    1.0,
                                ]
                            } else {
                                fallback_fg
                            }
                        });

                    self.push_glyph_styled(
                        ch,
                        &GlyphCell {
                            x,
                            y,
                            w: layout.cell_w_px,
                            h: layout.cell_h_px,
                            color: fg,
                            style,
                        },
                    );
                }
            }

            line_char_start = line_char_start.saturating_add(line.chars().count() + 1);
        }
    }

    /// Render a text command with color information.
    /// Uses per-char colors if provided; ignores per-char styles (uses global style).
    /// Glyphs must be preloaded in atlas via warm_atlas.
    fn render_text_simple(
        &mut self,
        _gl: &glow::Context,
        cmd: &render_model::TextCommand,
        metrics: CellMetrics,
    ) {
        let mut x = cmd.x;
        let y = cmd.y;
        let style_bits = style_to_bits(&cmd.style);

        if let Some(char_colors) = &cmd.char_colors {
            for (i, ch) in cmd.text.chars().enumerate() {
                let mut color = char_colors.get(i).copied().unwrap_or(cmd.color);
                if cmd.style.dim {
                    color[3] *= 0.55;
                }
                self.push_glyph_styled(
                    ch,
                    &GlyphCell {
                        x,
                        y,
                        w: metrics.width,
                        h: metrics.height,
                        color,
                        style: style_bits,
                    },
                );
                x += metrics.width;
            }
        } else {
            let mut color = cmd.color;
            if cmd.style.dim {
                color[3] *= 0.55;
            }
            for ch in cmd.text.chars() {
                self.push_glyph_styled(
                    ch,
                    &GlyphCell {
                        x,
                        y,
                        w: metrics.width,
                        h: metrics.height,
                        color,
                        style: style_bits,
                    },
                );
                x += metrics.width;
            }
        }
    }

    /// Render terminal text from Scene with per-character colors and styles.
    /// This is more complete than render_text_simple, handling the full palette.
    #[allow(clippy::too_many_arguments)]
    fn render_text_with_colors(
        &mut self,
        gl: &glow::Context,
        x: f32,
        y: f32,
        text: &str,
        colors: &[Option<[f32; 3]>],
        styles: &[u8],
        metrics: CellMetrics,
        fallback_color: [f32; 4],
    ) {
        let mut current_x = x;

        for (char_idx, ch) in text.chars().enumerate() {
            let fg = colors
                .get(char_idx)
                .and_then(|c| *c)
                .map(|c| [c[0], c[1], c[2], 1.0])
                .unwrap_or(fallback_color);

            let style = styles.get(char_idx).copied().unwrap_or(0);

            // Apply dim style
            let mut color = fg;
            if style & STYLE_DIM != 0 {
                color[3] *= 0.55;
            }

            self.push_glyph(ch, current_x, y, metrics.width, metrics.height, color);
            self.ensure_char_in_atlas(gl, ch, style & (STYLE_BOLD | STYLE_ITALIC));

            current_x += metrics.width;
        }
    }

    /// Render a Scene of backend-independent commands.
    /// This is a compatibility bridge for components to emit Scene commands instead of
    /// calling OpenGL directly. Processes layers in defined order: Background, Main, Floating, Overlay, Toast, Debug.
    pub(crate) fn render_scene(
        &mut self,
        gl: &glow::Context,
        scene: &Scene,
        metrics: CellMetrics,
        width: f32,
        height: f32,
    ) {
        // Clear batch structures
        self.batches.flat.clear();
        self.batches.glyph.clear();
        self.batches.emoji.clear();

        // Clear clipping stack
        self.clip_stack.clear();
        unsafe {
            gl.disable(glow::SCISSOR_TEST);
        }

        // Process layers in defined order
        for (_layer, commands) in scene.iter_layers() {
            for command in commands {
                match command {
                    RenderCommand::Rect(cmd) => {
                        let rect = &cmd.rect;
                        self.push_rect(rect.x, rect.y, rect.x + rect.w, rect.y + rect.h, cmd.color);
                    }
                    RenderCommand::Text(cmd) => {
                        // Simple text rendering for overlays (monospace, no complex shaping).
                        // Terminal/editor text continues through the old paths (draw_terminal_text, etc).
                        self.render_text_simple(gl, cmd, metrics);
                    }
                    RenderCommand::ClipPush(rect) => {
                        self.push_clip_rect(gl, rect, height);
                    }
                    RenderCommand::ClipPop => {
                        self.pop_clip_rect(gl);
                    }
                }
            }
        }

        self.flush_passes(gl, width, height);
    }

    fn render_pane_backgrounds(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let bg = |c| with_backdrop_alpha(c, snapshot.opacity);
        self.push_rect(
            0.0,
            layout.tab_bar_h,
            layout.width,
            layout.terminal_h,
            bg(snapshot.theme.terminal_bg),
        );
        let editor_bg = if snapshot.editor_disabled {
            let [r, g, b, a] = snapshot.theme.editor_bg;
            [r * 0.55, g * 0.55, b * 0.55, a]
        } else {
            snapshot.theme.editor_bg
        };
        self.push_rect(
            0.0,
            layout.editor_top,
            layout.width,
            layout.height,
            bg(editor_bg),
        );
        self.push_rect(
            0.0,
            layout.terminal_h,
            layout.width,
            layout.editor_top,
            bg(if snapshot.editor_focused {
                snapshot.theme.separator_focused
            } else {
                snapshot.theme.separator
            }),
        );
        if snapshot.bell_active {
            self.push_rect(
                0.0,
                layout.tab_bar_h,
                layout.width,
                layout.terminal_h,
                bg([0.60, 0.20, 0.20, 0.15]),
            );
        }
    }

    /// Flush accumulated vertices from batches to the GPU,
    /// then clear batches ready for the next accumulation phase.
    fn flush_passes(&mut self, gl: &glow::Context, width: f32, height: f32) {
        // ── Flush flat-colour geometry (backgrounds, borders, cursor…) ────
        if !self.batches.flat.is_empty() {
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    self.batches.flat.vertices.as_ptr() as *const u8,
                    self.batches.flat.vertices.len() * size_of::<f32>(),
                )
            };

            unsafe {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

                gl.use_program(Some(self.gpu_state.flat.program));
                gl.bind_vertex_array(Some(self.gpu_state.flat.vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.gpu_state.flat.vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

                if let Some(loc) = self.gpu_state.flat.u_screen.as_ref() {
                    gl.uniform_2_f32(Some(loc), width, height);
                }

                gl.draw_arrays(
                    glow::TRIANGLES,
                    0,
                    (self.batches.flat.vertices.len() / 6) as i32,
                );

                gl.bind_buffer(glow::ARRAY_BUFFER, None);
                gl.bind_vertex_array(None);
                gl.use_program(None);
                gl.disable(glow::BLEND);
            }
            self.batches.flat.clear();
        }

        // ── Flush atlas-textured glyph quads (text) ──────────────────────
        if !self.batches.glyph.is_empty() {
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    self.batches.glyph.vertices.as_ptr() as *const u8,
                    self.batches.glyph.vertices.len() * size_of::<f32>(),
                )
            };

            unsafe {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

                gl.use_program(Some(self.gpu_state.glyph.program));
                gl.bind_vertex_array(Some(self.gpu_state.glyph.vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.gpu_state.glyph.vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

                if let Some(loc) = &self.gpu_state.glyph.u_screen {
                    gl.uniform_2_f32(Some(loc), width, height);
                }

                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.gpu_state.glyph.texture));
                if let Some(loc) = &self.gpu_state.glyph.u_sampler {
                    gl.uniform_1_i32(Some(loc), 0);
                }

                gl.draw_arrays(
                    glow::TRIANGLES,
                    0,
                    (self.batches.glyph.vertices.len() / 8) as i32,
                );

                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.bind_buffer(glow::ARRAY_BUFFER, None);
                gl.bind_vertex_array(None);
                gl.use_program(None);
                gl.disable(glow::BLEND);
            }
            self.batches.glyph.clear();
        }

        // ── Flush color-emoji RGBA atlas quads ───────────────────────────────
        if !self.batches.emoji.is_empty() {
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    self.batches.emoji.vertices.as_ptr() as *const u8,
                    self.batches.emoji.vertices.len() * size_of::<f32>(),
                )
            };

            unsafe {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

                gl.use_program(Some(self.gpu_state.emoji.program));
                gl.bind_vertex_array(Some(self.gpu_state.emoji.vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.gpu_state.emoji.vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

                if let Some(loc) = &self.gpu_state.emoji.u_screen {
                    gl.uniform_2_f32(Some(loc), width, height);
                }

                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.gpu_state.emoji.texture));
                if let Some(loc) = &self.gpu_state.emoji.u_sampler {
                    gl.uniform_1_i32(Some(loc), 0);
                }

                gl.draw_arrays(
                    glow::TRIANGLES,
                    0,
                    (self.batches.emoji.vertices.len() / 8) as i32,
                );

                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.bind_buffer(glow::ARRAY_BUFFER, None);
                gl.bind_vertex_array(None);
                gl.use_program(None);
                gl.disable(glow::BLEND);
            }
            self.batches.emoji.clear();
        }
    }

    // draw_terminal_text: REMOVED - migrated to emit_terminal_text_to_scene()

    fn draw_editor_text(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let dim = if snapshot.editor_disabled { 0.35 } else { 1.0 };
        let default_fg = [
            snapshot.theme.text[0] * dim,
            snapshot.theme.text[1] * dim,
            snapshot.theme.text[2] * dim,
            1.0,
        ];
        let max_x = layout.width - layout.padding_h;
        let max_y = layout.height - layout.padding_v;
        let row_offset = snapshot.editor_scroll_offset;
        let mut char_idx = 0usize; // tracks index into `hl` as we iterate chars

        for (line_idx, line) in snapshot.editor_text.lines().enumerate() {
            if line_idx < row_offset {
                char_idx = char_idx.saturating_add(line.chars().count() + 1);
                continue;
            }
            let row = line_idx - row_offset;
            let y = layout.editor_top + layout.padding_v + row as f32 * layout.cell_h_px;
            if y + layout.cell_h_px > max_y {
                break;
            }
            let mut vcol = 0usize; // visual column (accounts for wide chars)
            for ch in line.chars() {
                let cw = char_col_width(ch);
                let x = layout.padding_h
                    + (vcol as f32 - snapshot.editor_horizontal_scroll_offset as f32)
                        * layout.cell_w_px;
                if x + layout.cell_w_px > max_x {
                    break;
                }
                let fg = snapshot
                    .editor_fg_colors
                    .get(char_idx)
                    .and_then(|c| *c)
                    .map(|c| [c[0] * dim, c[1] * dim, c[2] * dim, 1.0])
                    .unwrap_or(default_fg);
                // Wide chars (emoji, CJK) occupy 2 columns: pass their full
                // physical width so push_color_emoji centres correctly.
                let phys_w = cw as f32 * layout.cell_w_px;
                if !self.push_color_emoji(ch, x, y, phys_w, layout.cell_h_px) {
                    self.push_glyph(ch, x, y, phys_w, layout.cell_h_px, fg);
                }
                vcol += cw;
                char_idx += 1;
            }
            char_idx = char_idx.saturating_add(1); // newline
        }
    }

    fn draw_editor_suggestion(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        if snapshot.editor_suggestion.is_empty() {
            return;
        }
        let (row, col) =
            editor_offset_to_row_col(&snapshot.editor_text, snapshot.editor_cursor_offset);
        let visible_row = row.saturating_sub(snapshot.editor_scroll_offset);
        let y = layout.editor_top + layout.padding_v + visible_row as f32 * layout.cell_h_px;
        let base_x = layout.padding_h
            + (col as f32 - snapshot.editor_horizontal_scroll_offset as f32) * layout.cell_w_px;
        let color = [
            snapshot.theme.text[0],
            snapshot.theme.text[1],
            snapshot.theme.text[2],
            0.45,
        ];
        for (i, ch) in snapshot.editor_suggestion.chars().enumerate() {
            let x = base_x + i as f32 * layout.cell_w_px;
            if x + layout.cell_w_px > layout.width - layout.padding_h {
                break;
            }
            self.push_glyph(ch, x, y, layout.cell_w_px, layout.cell_h_px, color);
        }
    }

    fn draw_tab_bar(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        if snapshot.tab_labels.is_empty() || layout.tab_bar_h <= 0.0 {
            return;
        }
        let bg = |c| with_backdrop_alpha(c, snapshot.opacity);
        let tab_bar_bg = clamp_color(snapshot.theme.terminal_bg, 0.05);
        let tab_inactive = clamp_color(snapshot.theme.terminal_bg, 0.02);
        let tab_active = mix_color(tab_bar_bg, snapshot.theme.separator_focused, 0.22);
        let add_btn_bg = [
            (snapshot.theme.terminal_bg[0] + 0.05).clamp(0.0, 1.0),
            (snapshot.theme.terminal_bg[1] + 0.10).clamp(0.0, 1.0),
            (snapshot.theme.terminal_bg[2] + 0.03).clamp(0.0, 1.0),
            0.90,
        ];
        self.push_rect(0.0, 0.0, layout.width, layout.tab_bar_h, bg(tab_bar_bg));

        let n = snapshot.tab_labels.len().max(1);
        let add_w = layout.cell_w_px * 2.0;
        let tab_area_w = (layout.width - add_w).max(layout.cell_w_px * 2.0);
        let tab_w = (tab_area_w / n as f32).max(layout.cell_w_px * 3.0);
        let gap = 1.0;

        for (i, label) in snapshot.tab_labels.iter().enumerate() {
            let x0 = i as f32 * tab_w + gap;
            let x1 = ((i + 1) as f32 * tab_w - gap).min(tab_area_w - gap);
            let y0 = 1.0;
            let y1 = (layout.tab_bar_h - 1.0).max(y0 + 1.0);
            let color = if i == snapshot.active_tab {
                tab_active
            } else {
                tab_inactive
            };
            self.push_rect(x0, y0, x1, y1, bg(color));

            let text_x = x0 + layout.cell_w_px * 0.5;
            let text_y = (layout.tab_bar_h - layout.cell_h_px).max(0.0) * 0.5;
            for (ci, ch) in label.chars().take(18).enumerate() {
                let gx = text_x + ci as f32 * layout.cell_w_px;
                if gx + layout.cell_w_px > x1 - layout.cell_w_px * 2.0 {
                    break;
                }
                self.push_glyph(
                    ch,
                    gx,
                    text_y,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    [
                        snapshot.theme.text[0],
                        snapshot.theme.text[1],
                        snapshot.theme.text[2],
                        1.0,
                    ],
                );
            }

            // Close button '×' at the right edge of the tab
            let close_x = x1 - layout.cell_w_px * 1.4;
            self.push_glyph(
                '×',
                close_x,
                text_y,
                layout.cell_w_px,
                layout.cell_h_px,
                [
                    snapshot.theme.text[0],
                    snapshot.theme.text[1],
                    snapshot.theme.text[2],
                    0.65,
                ],
            );
        }

        let add_x0 = tab_area_w + gap;
        let add_x1 = (layout.width - gap).max(add_x0 + 1.0);
        self.push_rect(
            add_x0,
            1.0,
            add_x1,
            (layout.tab_bar_h - 1.0).max(2.0),
            bg(add_btn_bg),
        );
        self.push_glyph(
            '+',
            add_x0 + (add_w - layout.cell_w_px) * 0.5,
            (layout.tab_bar_h - layout.cell_h_px).max(0.0) * 0.5,
            layout.cell_w_px,
            layout.cell_h_px,
            [
                snapshot.theme.text[0],
                snapshot.theme.text[1],
                snapshot.theme.text[2],
                1.0,
            ],
        );

        if let Some(insert_before) = snapshot.tab_drag_insert_before {
            let ib = insert_before.min(n);
            let x = (ib as f32 * tab_w).clamp(0.0, tab_area_w);
            self.push_rect(
                (x - 1.0).max(0.0),
                0.0,
                (x + 1.0).min(layout.width),
                layout.tab_bar_h,
                bg(snapshot.theme.separator_focused),
            );
        }
    }

    fn draw_terminal_highlights(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let hl = [0.40, 0.55, 0.85, 0.35];
        let current = [0.85, 0.65, 0.20, 0.45];
        for (row, start, len) in &snapshot.search_highlights {
            if *len == 0 {
                continue;
            }
            let y = layout.terminal_text_top + *row as f32 * layout.cell_h_px;
            if y < layout.terminal_text_top || y + layout.cell_h_px > layout.terminal_text_bottom {
                continue;
            }
            let x0 = layout.padding_h + *start as f32 * layout.cell_w_px;
            let x1 = x0 + *len as f32 * layout.cell_w_px;
            self.push_rect(x0, y, x1, y + layout.cell_h_px, hl);
        }
        if let Some((row, start, len)) = snapshot.search_current_highlight
            && len > 0
        {
            let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;
            if y >= layout.terminal_text_top && y + layout.cell_h_px <= layout.terminal_text_bottom
            {
                let x0 = layout.padding_h + start as f32 * layout.cell_w_px;
                let x1 = x0 + len as f32 * layout.cell_w_px;
                self.push_rect(x0, y, x1, y + layout.cell_h_px, current);
            }
        }
        if let Some((r0, c0, r1, c1)) = snapshot.selection {
            let (sr, sc, er, ec) = normalize_rect_selection(r0, c0, r1, c1);
            let sel = [0.35, 0.50, 0.80, 0.35];
            for row in sr..=er {
                let from = if row == sr { sc } else { 0 };
                let to = if row == er {
                    ec
                } else {
                    (layout.width / layout.cell_w_px) as usize
                };
                if to <= from {
                    continue;
                }
                let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;
                if y < layout.terminal_text_top
                    || y + layout.cell_h_px > layout.terminal_text_bottom
                {
                    continue;
                }
                let x0 = layout.padding_h + from as f32 * layout.cell_w_px;
                let x1 = layout.padding_h + to as f32 * layout.cell_w_px;
                self.push_rect(x0, y, x1, y + layout.cell_h_px, sel);
            }
        }

        // Underline detected links.
        let link_c = [0.25, 0.70, 1.00, 0.90];
        for link in &snapshot.terminal_links {
            let y = layout.terminal_text_top + link.row as f32 * layout.cell_h_px;
            if y < layout.terminal_text_top || y + layout.cell_h_px > layout.terminal_text_bottom {
                continue;
            }
            let x0 = layout.padding_h + link.col_start as f32 * layout.cell_w_px;
            let x1 = layout.padding_h + link.col_end as f32 * layout.cell_w_px;
            let uy0 = y + layout.cell_h_px - (layout.cell_h_px * 0.10).max(1.0);
            self.push_rect(
                x0,
                uy0,
                x1,
                uy0 + (layout.cell_h_px * 0.08).max(1.0),
                link_c,
            );
        }
    }

    fn draw_editor_selection(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let Some((a, b)) = snapshot.editor_selection else {
            return;
        };
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        if end <= start {
            return;
        }
        let sel_c = [0.35, 0.50, 0.80, 0.35];
        let mut idx = 0usize;
        for (line_idx, line) in snapshot.editor_text.lines().enumerate() {
            if line_idx < snapshot.editor_scroll_offset {
                idx = idx.saturating_add(line.chars().count() + 1);
                continue;
            }
            let row = line_idx - snapshot.editor_scroll_offset;
            let row_start_idx = idx;
            let row_end_idx = idx + line.chars().count();
            if end < row_start_idx {
                break;
            }
            if start <= row_end_idx {
                let from = start.saturating_sub(row_start_idx);
                let to = end.min(row_end_idx).saturating_sub(row_start_idx);
                if to > from {
                    let horizontal_scroll = snapshot.editor_horizontal_scroll_offset as f32;
                    let y = layout.editor_top + layout.padding_v + row as f32 * layout.cell_h_px;
                    let x0 =
                        layout.padding_h + (from as f32 - horizontal_scroll) * layout.cell_w_px;
                    let x1 = layout.padding_h + (to as f32 - horizontal_scroll) * layout.cell_w_px;
                    self.push_rect(x0, y, x1, y + layout.cell_h_px, sel_c);
                }
            }
            idx = row_end_idx.saturating_add(1);
        }
    }

    fn draw_resize_overlay(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let Some(text) = &snapshot.resize_overlay else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let w = (text.chars().count() as f32 * layout.cell_w_px + layout.cell_w_px * 2.0)
            .min(layout.width * 0.8);
        let h = layout.cell_h_px * 2.0;
        let x0 = (layout.width - w) * 0.5;
        let y0 = (layout.height - h) * 0.5;
        self.push_rect(x0, y0, x0 + w, y0 + h, [0.08, 0.10, 0.16, 0.92]);
        self.push_rect(
            x0 - 1.0,
            y0 - 1.0,
            x0 + w + 1.0,
            y0 + h + 1.0,
            [0.35, 0.55, 0.90, 0.95],
        );
        let ty = y0 + (h - layout.cell_h_px) * 0.5;
        let tx = x0 + layout.cell_w_px;
        for (i, ch) in text.chars().enumerate() {
            self.push_glyph(
                ch,
                tx + i as f32 * layout.cell_w_px,
                ty,
                layout.cell_w_px,
                layout.cell_h_px,
                [0.92, 0.94, 0.98, 1.0],
            );
        }
    }

    fn draw_suggestion_dropdown(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let Some(dd) = &snapshot.suggestion_dropdown else {
            return;
        };
        if dd.items.is_empty() {
            return;
        }
        let max_visible = 8usize;
        let start = dd.scroll_offset.min(dd.items.len());
        let visible = dd.items.len().saturating_sub(start).min(max_visible);
        if visible == 0 {
            return;
        }
        // Match wgpu: anchor bottom of panel to the separator (editor_top),
        // grow upward into the terminal area so the editor stays clean.
        let row_h = layout.cell_h_px * 1.2;
        let panel_w = (layout.cell_w_px * 40.0).min(layout.width * 0.75);
        let panel_h = visible as f32 * row_h;
        let x0 = layout.padding_h;
        let x1 = x0 + panel_w;
        let y1 = layout.editor_top; // bottom edge flush with separator
        let y0 = (y1 - panel_h).max(layout.tab_bar_h); // grow upward
        self.push_rect(
            x0 - 1.0,
            y0 - 1.0,
            x1 + 1.0,
            y1 + 1.0,
            [0.30, 0.45, 0.70, 0.95],
        );
        self.push_rect(x0, y0, x1, y1, [0.09, 0.11, 0.18, 0.97]);
        for i in 0..visible {
            let idx = start + i;
            let row_y = y0 + i as f32 * row_h;
            if idx == dd.selected {
                self.push_rect(x0, row_y, x1, row_y + row_h, [0.20, 0.32, 0.58, 0.70]);
            }
            let fg = if idx == dd.selected {
                [0.92, 0.94, 0.98, 1.0]
            } else {
                let [r, g, b, _] = snapshot.theme.text;
                [r * 0.72, g * 0.72, b * 0.72, 0.9]
            };
            for (ci, ch) in dd.items[idx].chars().take(36).enumerate() {
                let gx = x0 + layout.cell_w_px * 0.6 + ci as f32 * layout.cell_w_px;
                self.push_glyph(
                    ch,
                    gx,
                    row_y + (row_h - layout.cell_h_px) * 0.5,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    fg,
                );
            }
        }

        // Scrollbar — shown only when there are more items than visible.
        let total = dd.items.len();
        if total > visible {
            let sb_w = (layout.cell_w_px * 0.35).max(3.0);
            let sb_x0 = x1 - sb_w;
            let sb_x1 = x1;
            // Track.
            self.push_rect(sb_x0, y0, sb_x1, y1, [0.17, 0.19, 0.26, 0.97]);
            // Thumb.
            let thumb_frac = visible as f32 / total as f32;
            let thumb_h = panel_h * thumb_frac;
            let max_scroll = (total - visible) as f32;
            let scroll_frac = dd.scroll_offset as f32 / max_scroll;
            let thumb_top = y0 + scroll_frac * (panel_h - thumb_h);
            self.push_rect(
                sb_x0,
                thumb_top,
                sb_x1,
                thumb_top + thumb_h,
                [0.30, 0.45, 0.70, 0.95],
            );
        }
    }

    fn draw_search_panel(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let Some(panel) = &snapshot.search_panel else {
            return;
        };
        let text = format!(
            "Find: {} [{}/{}]{}{}",
            panel.query,
            panel.current_match,
            panel.match_count,
            if panel.regex_mode { " R" } else { "" },
            if panel.case_sensitive { " C" } else { "" }
        );
        let w = (text.chars().count() as f32 * layout.cell_w_px + layout.cell_w_px * 2.0)
            .min(layout.width * 0.65);
        let h = layout.cell_h_px * 1.6;
        let x0 = (layout.width - w - layout.padding_h).max(0.0);
        let y0 = layout.tab_bar_h + layout.padding_v;
        self.push_rect(
            x0 - 1.0,
            y0 - 1.0,
            x0 + w + 1.0,
            y0 + h + 1.0,
            [0.30, 0.45, 0.70, 0.95],
        );
        self.push_rect(x0, y0, x0 + w, y0 + h, [0.09, 0.11, 0.18, 0.96]);
        for (i, ch) in text.chars().enumerate() {
            let gx = x0 + layout.cell_w_px * 0.6 + i as f32 * layout.cell_w_px;
            if gx + layout.cell_w_px > x0 + w {
                break;
            }
            self.push_glyph(
                ch,
                gx,
                y0 + (h - layout.cell_h_px) * 0.5,
                layout.cell_w_px,
                layout.cell_h_px,
                [0.92, 0.94, 0.98, 1.0],
            );
        }
        if let Some(err) = &panel.error {
            let ey = y0 + h + 2.0;
            let ew = (err.chars().count() as f32 * layout.cell_w_px + layout.cell_w_px)
                .min(layout.width * 0.70);
            self.push_rect(
                x0,
                ey,
                x0 + ew,
                ey + layout.cell_h_px * 1.3,
                [0.22, 0.08, 0.08, 0.96],
            );
            for (i, ch) in err.chars().take(48).enumerate() {
                self.push_glyph(
                    ch,
                    x0 + 4.0 + i as f32 * layout.cell_w_px,
                    ey + 2.0,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    [1.0, 0.9, 0.9, 1.0],
                );
            }
        }
    }

    fn draw_context_menu(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let Some(menu) = &snapshot.context_menu else {
            return;
        };
        if menu.items.is_empty() {
            return;
        }
        let max_chars = menu
            .items
            .iter()
            .map(|s| s.chars().count())
            .max()
            .unwrap_or(8) as f32;
        let w = (max_chars * layout.cell_w_px + layout.cell_w_px * 2.0).min(layout.width * 0.5);
        let row_h = layout.cell_h_px * 1.4;
        let h = row_h * menu.items.len() as f32;
        let x0 = menu.x_px.clamp(0.0, (layout.width - w).max(0.0));
        let y0 = menu.y_px.clamp(0.0, (layout.height - h).max(0.0));
        self.push_rect(
            x0 - 1.0,
            y0 - 1.0,
            x0 + w + 1.0,
            y0 + h + 1.0,
            [0.35, 0.55, 0.90, 0.95],
        );
        self.push_rect(x0, y0, x0 + w, y0 + h, [0.09, 0.11, 0.18, 0.97]);
        for (i, item) in menu.items.iter().enumerate() {
            let iy = y0 + i as f32 * row_h;
            if Some(i) == menu.hovered_item {
                self.push_rect(x0, iy, x0 + w, iy + row_h, [0.20, 0.32, 0.58, 0.75]);
            }
            for (ci, ch) in item.chars().take(36).enumerate() {
                self.push_glyph(
                    ch,
                    x0 + 6.0 + ci as f32 * layout.cell_w_px,
                    iy + 2.0,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    [0.92, 0.94, 0.98, 1.0],
                );
            }
        }
    }

    fn settings_item_display_val<'a>(
        item: &'a crate::SettingsItem,
        overlay: &'a SettingsOverlay,
        is_focused: bool,
    ) -> std::borrow::Cow<'a, str> {
        if item.is_searchable && is_focused && overlay.search_buf.is_some() {
            let sbuf = overlay.search_buf.as_deref().unwrap_or("");
            return format!("/ {}\u{258e}", sbuf).into();
        }
        if item.is_searchable {
            return format!("{} /", item.value).into();
        }
        if item.is_selectable && !item.is_action && !(is_focused && overlay.editing.is_some()) {
            return format!("\u{2190} {} \u{2192}", item.value).into();
        }
        if !item.is_selectable && is_focused && overlay.editing.is_none() {
            return format!("{}\u{258e}", item.value).into();
        }
        if is_focused && let Some(ref buf) = overlay.editing {
            return buf.as_str().into();
        }
        item.value.as_str().into()
    }

    fn draw_settings_panel_bg(
        &mut self,
        overlay: &SettingsOverlay,
        geom: &SettingsPanelGeom,
        colors: &SettingsPanelColors,
    ) -> usize {
        self.push_rect(
            geom.x0 - 2.0,
            geom.y0 - 2.0,
            geom.x1 + 2.0,
            geom.y1 + 2.0,
            colors.border,
        );
        self.push_rect(geom.x0, geom.y0, geom.x1, geom.y1, colors.bg);
        self.push_rect(
            geom.x0,
            geom.y0,
            geom.x1,
            geom.y0 + geom.title_h,
            colors.title,
        );
        let mut editable_idx = 0usize;
        for (i, item) in overlay.items.iter().enumerate() {
            let ry = geom.y0 + geom.title_h + i as f32 * geom.row_h;
            if item.is_header {
                self.push_rect(geom.x0, ry, geom.x1, ry + geom.row_h, colors.section);
            } else {
                if editable_idx == overlay.cursor {
                    let c = if overlay.editing.is_some() {
                        colors.edit
                    } else {
                        colors.select
                    };
                    self.push_rect(geom.x0, ry, geom.x1, ry + geom.row_h, c);
                }
                editable_idx += 1;
            }
        }
        if overlay.editing.is_some() {
            let ey = geom.y0 + geom.title_h + overlay.items.len() as f32 * geom.row_h;
            self.push_rect(geom.x0, ey, geom.x1, ey + geom.edit_h, colors.edit);
        }
        let focused_flat = {
            let mut ec = 0usize;
            let mut fi = 0usize;
            for (i, item) in overlay.items.iter().enumerate() {
                if !item.is_header {
                    if ec == overlay.cursor {
                        fi = i;
                        break;
                    }
                    ec = ec.saturating_add(1);
                }
            }
            fi
        };
        if overlay.search_buf.is_some() {
            let visible = overlay
                .search_matches
                .len()
                .saturating_sub(overlay.search_scroll_offset)
                .clamp(1, SETTINGS_MAX_VISIBLE_SEARCH);
            let dy = geom.y0 + geom.title_h + (focused_flat + 1) as f32 * geom.row_h;
            let dh = geom.row_h * visible as f32;
            self.push_rect(
                geom.x0 - 1.0,
                dy - 1.0,
                geom.x1 + 1.0,
                dy + dh + 1.0,
                [0.35, 0.50, 0.82, 1.0],
            );
            self.push_rect(geom.x0, dy, geom.x1, dy + dh, [0.15, 0.19, 0.30, 1.0]);
            let vis_sel = overlay
                .search_selected
                .saturating_sub(overlay.search_scroll_offset);
            if !overlay.search_matches.is_empty() && vis_sel < visible {
                let sy0 = dy + vis_sel as f32 * geom.row_h;
                self.push_rect(
                    geom.x0,
                    sy0,
                    geom.x1,
                    sy0 + geom.row_h,
                    [0.22, 0.34, 0.62, 1.0],
                );
            }
        }
        focused_flat
    }

    fn draw_settings_rows_text(
        &mut self,
        overlay: &SettingsOverlay,
        geom: &SettingsPanelGeom,
        focused_flat: usize,
        layout: &FrameLayout,
        th: &ColorTheme,
    ) -> usize {
        let title_text = if overlay.just_saved {
            "  SETTINGS  \u{2713} Saved"
        } else {
            "  SETTINGS  (Cmd+,)"
        };
        let ty = geom.y0 + (geom.title_h - layout.cell_h_px) * 0.5;
        let mut tx = geom.x0;
        for ch in title_text.chars() {
            self.push_glyph(ch, tx, ty, layout.cell_w_px, layout.cell_h_px, th.text);
            tx += layout.cell_w_px;
        }
        const SEARCH_MAX_VISIBLE: usize = 8;
        let search_cover_end = if overlay.search_buf.is_some() {
            let n_vis = overlay
                .search_matches
                .len()
                .saturating_sub(overlay.search_scroll_offset)
                .min(SEARCH_MAX_VISIBLE);
            focused_flat + n_vis
        } else {
            0
        };
        let mut editable_idx = 0usize;
        let mut focused_flat_idx = 0usize;
        for (i, item) in overlay.items.iter().enumerate() {
            let row_y = geom.y0
                + geom.title_h
                + i as f32 * geom.row_h
                + (geom.row_h - layout.cell_h_px) / 2.0;
            if item.is_header {
                if overlay.search_buf.is_some() && i > focused_flat && i <= search_cover_end {
                    continue;
                }
                let mut kx = geom.key_col;
                for ch in item.key.chars() {
                    self.push_glyph(
                        ch,
                        kx,
                        row_y,
                        layout.cell_w_px,
                        layout.cell_h_px,
                        th.separator_focused,
                    );
                    kx += layout.cell_w_px;
                }
            } else {
                let is_focused = editable_idx == overlay.cursor;
                if is_focused {
                    focused_flat_idx = i;
                }
                editable_idx += 1;
                if overlay.search_buf.is_some() && i > focused_flat && i <= search_cover_end {
                    continue;
                }
                let [r, g, b, _] = th.text;
                let [cr, cg, cb, _] = th.cursor;
                let (key_color, val_color) = if is_focused {
                    (th.text, th.cursor)
                } else {
                    (
                        [r * 0.85, g * 0.85, b * 0.85, 1.0_f32],
                        [cr * 0.75, cg * 0.85, cb * 0.85, 0.85_f32],
                    )
                };
                let mut kx = geom.key_col;
                for ch in item.key.chars() {
                    self.push_glyph(ch, kx, row_y, layout.cell_w_px, layout.cell_h_px, key_color);
                    kx += layout.cell_w_px;
                }
                let display_val = Self::settings_item_display_val(item, overlay, is_focused);
                let mut vx = geom.val_col;
                for ch in display_val.chars() {
                    self.push_glyph(ch, vx, row_y, layout.cell_w_px, layout.cell_h_px, val_color);
                    vx += layout.cell_w_px;
                }
            }
        }
        focused_flat_idx
    }

    fn draw_settings_footer_dropdown(
        &mut self,
        overlay: &SettingsOverlay,
        geom: &SettingsPanelGeom,
        focused_flat_idx: usize,
        layout: &FrameLayout,
        th: &ColorTheme,
    ) {
        if overlay.search_buf.is_none() {
            let footer_y = geom.y0
                + geom.title_h
                + overlay.items.len() as f32 * geom.row_h
                + geom.edit_h
                + (geom.footer_h - layout.cell_h_px) / 2.0;
            let footer_text = if overlay.editing.is_some() {
                "  Enter: confirm   Esc: cancel"
            } else {
                "  \u{2191}\u{2193} navigate   \u{2190}\u{2192} change   Enter: edit/search   Esc: close & save"
            };
            let [r, g, b, _] = th.text;
            let foot_color = [r * 0.55, g * 0.55, b * 0.55, 0.90_f32];
            let mut fx = geom.x0;
            for ch in footer_text.chars() {
                self.push_glyph(
                    ch,
                    fx,
                    footer_y,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    foot_color,
                );
                fx += layout.cell_w_px;
            }
        } else {
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
            let drop_top_px = geom.y0 + geom.title_h + (focused_flat_idx + 1) as f32 * geom.row_h;
            if overlay.search_matches.is_empty() {
                let item_y = drop_top_px + (geom.row_h - layout.cell_h_px) / 2.0;
                let [r, g, b, _] = th.text;
                let c = [r * 0.45, g * 0.45, b * 0.45, 0.70];
                let mut sx = geom.key_col;
                for ch in "(no results)".chars() {
                    self.push_glyph(ch, sx, item_y, layout.cell_w_px, layout.cell_h_px, c);
                    sx += layout.cell_w_px;
                }
            } else {
                for (i, match_str) in overlay.search_matches
                    [overlay.search_scroll_offset..visible_end]
                    .iter()
                    .enumerate()
                {
                    let item_y =
                        drop_top_px + i as f32 * geom.row_h + (geom.row_h - layout.cell_h_px) / 2.0;
                    let is_sel = i == vis_sel;
                    let [r, g, b, _] = th.text;
                    let color = if is_sel {
                        th.text
                    } else {
                        [r * 0.60, g * 0.60, b * 0.60, 1.0]
                    };
                    let labeled = if is_sel {
                        format!("\u{25b6} {}", match_str)
                    } else {
                        format!("  {}", match_str)
                    };
                    let mut sx = geom.key_col;
                    for ch in labeled.chars() {
                        self.push_glyph(ch, sx, item_y, layout.cell_w_px, layout.cell_h_px, color);
                        sx += layout.cell_w_px;
                    }
                }
            }
        }
    }

    fn draw_settings_overlay(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let Some(overlay) = &snapshot.settings_overlay else {
            return;
        };
        self.push_rect(0.0, 0.0, layout.width, layout.height, [0.0, 0.0, 0.0, 0.68]);

        let title_h = layout.cell_h_px * 2.2;
        let row_h = layout.cell_h_px * 1.7;
        let footer_h = layout.cell_h_px * 1.9;
        let edit_h = if overlay.editing.is_some() {
            layout.cell_h_px * 1.8
        } else {
            0.0
        };
        let panel_h = title_h + overlay.items.len() as f32 * row_h + edit_h + footer_h;
        let panel_w = (layout.cell_w_px * 72.0)
            .min(layout.width * 0.92)
            .max(layout.cell_w_px * 40.0);
        let x0 = (layout.width - panel_w) * 0.5;
        let y0 = (layout.height - panel_h) * 0.5;
        let geom = SettingsPanelGeom {
            x0,
            y0,
            x1: x0 + panel_w,
            y1: y0 + panel_h,
            title_h,
            row_h,
            edit_h,
            footer_h,
            key_col: x0 + layout.cell_w_px * 1.5,
            val_col: x0 + panel_w * 0.50,
        };
        let colors = SettingsPanelColors {
            bg: clamp_color(snapshot.theme.terminal_bg, 0.01),
            border: snapshot.theme.separator_focused,
            title: clamp_color(snapshot.theme.terminal_bg, -0.01),
            section: clamp_color(snapshot.theme.terminal_bg, 0.04),
            select: mix_color(
                clamp_color(snapshot.theme.terminal_bg, 0.08),
                snapshot.theme.separator_focused,
                0.20,
            ),
            edit: mix_color(
                clamp_color(snapshot.theme.terminal_bg, 0.08),
                snapshot.theme.separator_focused,
                0.28,
            ),
        };
        let focused_flat = self.draw_settings_panel_bg(overlay, &geom, &colors);
        let focused_flat_idx =
            self.draw_settings_rows_text(overlay, &geom, focused_flat, layout, &snapshot.theme);
        self.draw_settings_footer_dropdown(
            overlay,
            &geom,
            focused_flat_idx,
            layout,
            &snapshot.theme,
        );
    }

    fn draw_keybindings_rows(
        &mut self,
        overlay: &KeybindingsOverlay,
        geom: &KeybindingsPanelGeom,
        layout: &FrameLayout,
        th: &ColorTheme,
        colors: &KeybindingsPanelColors,
    ) {
        let (ov_bg, ov_row_alt, ov_select, ov_record) =
            (colors.bg, colors.row_alt, colors.select, colors.record);
        let n_rows = overlay.rows.len();
        let visible = overlay.visible_rows.min(n_rows);
        let scroll = overlay.scroll_offset;
        let visible_rows = &overlay.rows[scroll..(scroll + visible).min(n_rows)];
        for (i, row) in visible_rows.iter().enumerate() {
            let flat_idx = scroll + i;
            let ry = geom.y0 + geom.title_h + i as f32 * geom.row_h;
            let is_cursor = flat_idx == overlay.cursor;
            let row_bg = if is_cursor {
                if overlay.recording {
                    ov_record
                } else {
                    ov_select
                }
            } else if i % 2 == 1 {
                ov_row_alt
            } else {
                ov_bg
            };
            self.push_rect(geom.x0, ry, geom.x1, ry + geom.row_h, row_bg);
            let text_y = ry + (geom.row_h - layout.cell_h_px) * 0.5;
            let [r, g, b, _] = th.text;
            let label_color = if is_cursor {
                th.text
            } else {
                [r * 0.85, g * 0.85, b * 0.85, 1.0]
            };
            let binding_color = if is_cursor && overlay.recording {
                [1.0, 0.70, 0.25, 1.0_f32]
            } else if row.binding.is_some() && !row.is_default {
                th.cursor
            } else if row.is_default {
                [r * 0.65, g * 0.65, b * 0.65, 1.0]
            } else {
                [r * 0.40, g * 0.40, b * 0.40, 1.0]
            };
            let mut lx = geom.key_col;
            for ch in row.label.chars() {
                self.push_glyph(
                    ch,
                    lx,
                    text_y,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    label_color,
                );
                lx += layout.cell_w_px;
            }
            let binding_str: std::borrow::Cow<str> = if is_cursor && overlay.recording {
                "\u{25cf} press combo\u{2026}".into()
            } else if let Some(ref b) = row.binding {
                if row.is_default {
                    format!("{b}  (default)").into()
                } else {
                    b.as_str().into()
                }
            } else {
                "(not bound)".into()
            };
            let mut bx = geom.bind_col;
            for ch in binding_str.chars() {
                self.push_glyph(
                    ch,
                    bx,
                    text_y,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    binding_color,
                );
                bx += layout.cell_w_px;
            }
        }
    }

    fn draw_keybindings_footer(
        &mut self,
        overlay: &KeybindingsOverlay,
        geom: &KeybindingsPanelGeom,
        layout: &FrameLayout,
        th: &ColorTheme,
        colors: &KeybindingsPanelColors,
    ) {
        let ov_border = colors.border;
        let n_rows = overlay.rows.len();
        let visible = overlay.visible_rows.min(n_rows);
        let scroll = overlay.scroll_offset;
        if n_rows > visible {
            let sb_x = geom.x1 - layout.cell_w_px * 0.4;
            let sb_w = layout.cell_w_px * 0.25;
            let track_h = visible as f32 * geom.row_h;
            let thumb_h = (track_h * visible as f32 / n_rows as f32).max(geom.row_h * 0.5);
            let thumb_frac = scroll as f32 / (n_rows - visible) as f32;
            let thumb_y = geom.y0 + geom.title_h + thumb_frac * (track_h - thumb_h);
            self.push_rect(
                sb_x,
                geom.y0 + geom.title_h,
                sb_x + sb_w,
                geom.y0 + geom.title_h + track_h,
                [0.3, 0.3, 0.3, 0.3],
            );
            self.push_rect(sb_x, thumb_y, sb_x + sb_w, thumb_y + thumb_h, ov_border);
        }
        let footer_y = geom.y1 - geom.footer_h + (geom.footer_h - layout.cell_h_px) * 0.5;
        let footer_text = if overlay.recording {
            "  Esc \u{2192} cancel"
        } else {
            "  Enter \u{2192} bind    Backspace \u{2192} remove    Esc \u{2192} close"
        };
        let [r, g, b, _] = th.text;
        let hint_color = [r * 0.55, g * 0.55, b * 0.55, 1.0_f32];
        let mut fx = geom.x0;
        for ch in footer_text.chars() {
            self.push_glyph(
                ch,
                fx,
                footer_y,
                layout.cell_w_px,
                layout.cell_h_px,
                hint_color,
            );
            fx += layout.cell_w_px;
        }
    }

    fn draw_keybindings_overlay(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let Some(overlay) = &snapshot.keybindings_overlay else {
            return;
        };

        let n_rows = overlay.rows.len();
        let visible = overlay.visible_rows.min(n_rows);
        let row_h = layout.cell_h_px * 1.7;
        let title_h = layout.cell_h_px * 2.2;
        let footer_h = layout.cell_h_px * 2.0;
        let panel_h = title_h + visible as f32 * row_h + footer_h;
        let panel_w = (layout.cell_w_px * 72.0)
            .min(layout.width * 0.92)
            .max(layout.cell_w_px * 44.0);
        let x0 = (layout.width - panel_w) * 0.5;
        let y0 = (layout.height - panel_h) * 0.5;
        let th = &snapshot.theme;
        let colors = KeybindingsPanelColors {
            bg: clamp_color(th.terminal_bg, 0.01),
            border: th.separator_focused,
            title: clamp_color(th.terminal_bg, -0.01),
            row_alt: clamp_color(th.terminal_bg, 0.03),
            select: mix_color(
                clamp_color(th.terminal_bg, 0.08),
                th.separator_focused,
                0.22,
            ),
            record: mix_color(
                clamp_color(th.terminal_bg, 0.06),
                [0.9, 0.5, 0.1, 1.0],
                0.20,
            ),
        };
        let geom = KeybindingsPanelGeom {
            x0,
            y0,
            x1: x0 + panel_w,
            y1: y0 + panel_h,
            title_h,
            row_h,
            footer_h,
            key_col: x0 + layout.cell_w_px * 2.0,
            bind_col: x0 + panel_w * 0.55,
        };
        self.push_rect(0.0, 0.0, layout.width, layout.height, [0.0, 0.0, 0.0, 0.65]);
        self.push_rect(
            geom.x0 - 2.0,
            geom.y0 - 2.0,
            geom.x1 + 2.0,
            geom.y1 + 2.0,
            colors.border,
        );
        self.push_rect(geom.x0, geom.y0, geom.x1, geom.y1, colors.bg);
        self.push_rect(
            geom.x0,
            geom.y0,
            geom.x1,
            geom.y0 + geom.title_h,
            colors.title,
        );
        let title_str = if overlay.just_saved {
            "  KEYBINDINGS  \u{2713} Saved"
        } else if overlay.recording {
            "  KEYBINDINGS  \u{25cf} Press key combo..."
        } else {
            "  KEYBINDINGS"
        };
        let ty = geom.y0 + (geom.title_h - layout.cell_h_px) * 0.5;
        let mut tx = geom.x0;
        for ch in title_str.chars() {
            self.push_glyph(ch, tx, ty, layout.cell_w_px, layout.cell_h_px, th.text);
            tx += layout.cell_w_px;
        }
        self.draw_keybindings_rows(overlay, &geom, layout, th, &colors);
        self.draw_keybindings_footer(overlay, &geom, layout, th, &colors);
    }

    fn draw_toasts(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        if snapshot.toast_stack.is_empty() {
            return;
        }
        for (rev_idx, toast) in snapshot.toast_stack.iter().rev().enumerate() {
            let max_chars = toast.text.chars().count().max(4) as f32;
            let h = layout.cell_h_px * 1.5;
            let margin = layout.cell_h_px * 0.35;
            let pad_h = layout.cell_w_px * 1.2;
            let w = (max_chars * layout.cell_w_px + pad_h * 2.0).min(layout.width * 0.45);
            let bottom = layout.height - margin - rev_idx as f32 * (h + margin);
            let top = bottom - h;
            let right = layout.width - margin;
            let left = right - w;
            let (bg, border, text) = match toast.kind {
                crate::ToastKind::Info => (
                    [0.12, 0.15, 0.25, 0.93],
                    [0.35, 0.50, 0.90, 1.0],
                    [0.92, 0.94, 0.98, 1.0],
                ),
                crate::ToastKind::Success => (
                    [0.08, 0.20, 0.10, 0.93],
                    [0.25, 0.78, 0.35, 1.0],
                    [0.90, 1.00, 0.90, 1.0],
                ),
                crate::ToastKind::Warn => (
                    [0.22, 0.18, 0.05, 0.93],
                    [0.90, 0.72, 0.20, 1.0],
                    [1.00, 0.97, 0.85, 1.0],
                ),
                crate::ToastKind::Error => (
                    [0.22, 0.08, 0.08, 0.93],
                    [0.90, 0.30, 0.30, 1.0],
                    [1.00, 0.90, 0.90, 1.0],
                ),
            };
            self.push_rect(left - 1.0, top - 1.0, right + 1.0, bottom + 1.0, border);
            self.push_rect(left, top, right, bottom, bg);
            for (ci, ch) in toast
                .text
                .chars()
                .take(((w - pad_h * 2.0) / layout.cell_w_px) as usize)
                .enumerate()
            {
                self.push_glyph(
                    ch,
                    left + pad_h + ci as f32 * layout.cell_w_px,
                    top + (h - layout.cell_h_px) * 0.5,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    text,
                );
            }
        }
    }

    fn draw_scrollbar(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let sb_w = SCROLLBAR_W_PX;
        let sb_left = layout.width - sb_w;
        let [r, g, b, _] = snapshot.theme.separator_focused;
        let thumb_color = [r, g, b, 0.85];

        if snapshot.scrollback_lines > 0 {
            let track_top = layout.tab_bar_h;
            let track_bottom = layout.terminal_h;
            let track_h = track_bottom - track_top;
            if track_h > 0.0 && layout.cell_h_px > 0.0 {
                self.push_rect(
                    sb_left,
                    track_top,
                    layout.width,
                    track_bottom,
                    snapshot.theme.separator,
                );
                let visible_rows = (track_h / layout.cell_h_px).floor();
                let total_rows = visible_rows + snapshot.scrollback_lines as f32;
                let thumb_h = (visible_rows / total_rows).clamp(0.05, 1.0) * track_h;
                let scroll_pos = (snapshot.scroll_offset as f32 / snapshot.scrollback_lines as f32)
                    .clamp(0.0, 1.0);
                let thumb_top = track_top + (1.0 - scroll_pos) * (track_h - thumb_h);
                self.push_rect(
                    sb_left,
                    thumb_top,
                    layout.width,
                    thumb_top + thumb_h,
                    thumb_color,
                );
            }
        }

        let editor_h = layout.height - layout.editor_top;
        let visible_rows = ((editor_h - layout.padding_v) / layout.cell_h_px)
            .floor()
            .max(1.0);
        if snapshot.editor_line_count as f32 > visible_rows {
            self.push_rect(
                sb_left,
                layout.editor_top,
                layout.width,
                layout.height,
                snapshot.theme.separator,
            );
            let thumb_h =
                (visible_rows / snapshot.editor_line_count as f32).clamp(0.05, 1.0) * editor_h;
            let max_scroll = snapshot.editor_line_count as f32 - visible_rows;
            let scroll_pos = (snapshot.editor_scroll_offset as f32 / max_scroll).clamp(0.0, 1.0);
            let thumb_top = layout.editor_top + scroll_pos * (editor_h - thumb_h);
            self.push_rect(
                sb_left,
                thumb_top,
                layout.width,
                thumb_top + thumb_h,
                thumb_color,
            );
        }

        let max_cols = snapshot
            .editor_text
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        let track_left = layout.padding_h;
        let track_right = sb_left - layout.padding_h;
        let track_w = track_right - track_left;
        let visible_cols = (track_w / layout.cell_w_px).floor().max(1.0);
        if max_cols as f32 > visible_cols && track_w > 0.0 {
            let track_top = layout.height - SCROLLBAR_W_PX;
            self.push_rect(
                track_left,
                track_top,
                track_right,
                layout.height,
                snapshot.theme.separator,
            );
            let thumb_w = (visible_cols / max_cols as f32).clamp(0.05, 1.0) * track_w;
            let max_scroll = max_cols as f32 - visible_cols;
            let scroll_pos =
                (snapshot.editor_horizontal_scroll_offset as f32 / max_scroll).clamp(0.0, 1.0);
            let thumb_left = track_left + scroll_pos * (track_w - thumb_w);
            self.push_rect(
                thumb_left,
                track_top,
                thumb_left + thumb_w,
                layout.height,
                thumb_color,
            );
        }
    }

    fn draw_scroll_indicator(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        if snapshot.scroll_offset == 0 {
            return;
        }
        let h = layout.cell_h_px * 1.4;
        let w = layout.cell_w_px * 14.0;
        let margin = layout.cell_h_px * 0.5;
        let cx = layout.width * 0.5;
        let bottom = layout.terminal_h - margin;
        let top = bottom - h;
        let left = cx - w * 0.5;
        let right = cx + w * 0.5;
        self.push_rect(
            left - 1.0,
            top - 1.0,
            right + 1.0,
            bottom + 1.0,
            [0.40, 0.70, 1.00, 0.80],
        );
        self.push_rect(left, top, right, bottom, [0.08, 0.10, 0.18, 0.88]);
        let text = format!("↑ {} lines", snapshot.scroll_offset);
        for (ci, ch) in text.chars().take(18).enumerate() {
            self.push_glyph(
                ch,
                left + layout.cell_w_px * 0.6 + ci as f32 * layout.cell_w_px,
                top + (h - layout.cell_h_px) * 0.5,
                layout.cell_w_px,
                layout.cell_h_px,
                [0.92, 0.94, 0.98, 1.0],
            );
        }
    }

    fn draw_cursor(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        if !snapshot.cursor_blink_on {
            return;
        }
        let color = snapshot.theme.cursor;

        if snapshot.editor_focused && !snapshot.terminal_fullscreen && !snapshot.editor_disabled {
            let (row, col) =
                editor_offset_to_row_col(&snapshot.editor_text, snapshot.editor_cursor_offset);
            let visible_row = row.saturating_sub(snapshot.editor_scroll_offset);
            let x = layout.padding_h
                + (col as f32 - snapshot.editor_horizontal_scroll_offset as f32) * layout.cell_w_px;
            let y = layout.editor_top + layout.padding_v + visible_row as f32 * layout.cell_h_px;
            self.push_rect(x, y, x + layout.cell_w_px, y + layout.cell_h_px, color);
            return;
        }

        let row = snapshot.terminal_cursor_row;
        let col = snapshot.terminal_cursor_col;
        let x = layout.padding_h + col as f32 * layout.cell_w_px;
        let y = layout.terminal_text_top + row as f32 * layout.cell_h_px;
        if y < layout.terminal_text_bottom {
            match snapshot.cursor_shape {
                3 | 4 => {
                    let h = (layout.cell_h_px * 0.12).max(2.0);
                    self.push_rect(
                        x,
                        y + layout.cell_h_px - h,
                        x + layout.cell_w_px,
                        y + layout.cell_h_px,
                        color,
                    );
                }
                5 | 6 => {
                    let w = (layout.cell_w_px * 0.12).max(2.0);
                    self.push_rect(x, y, x + w, y + layout.cell_h_px, color);
                }
                _ => {
                    self.push_rect(x, y, x + layout.cell_w_px, y + layout.cell_h_px, color);
                }
            }
        }
    }

    // ── Glyph atlas management ────────────────────────────────────────────────

    // ── Color-emoji atlas helpers ─────────────────────────────────────────────

    /// Upload `img` (RGBA) into the color atlas and return its UV entry.
    fn pack_rgba_to_color_atlas(
        &mut self,
        gl: &glow::Context,
        img: &image::RgbaImage,
    ) -> Option<ColorAtlasEntry> {
        let w = img.width();
        let h = img.height();
        if w == 0 || h == 0 {
            return None;
        }

        // Use ColorAtlas for allocation
        let (dest_x, dest_y) = self.emoji_atlas.allocate(w, h)?;

        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.gpu_state.emoji.texture));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                dest_x as i32,
                dest_y as i32,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(img.as_raw()),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }

        let atf = COLOR_ATLAS_TEX_SIZE as f32;
        Some(ColorAtlasEntry {
            u0: dest_x as f32 / atf,
            v0: dest_y as f32 / atf,
            u1: (dest_x + w) as f32 / atf,
            v1: (dest_y + h) as f32 / atf,
            w_px: w,
            h_px: h,
        })
    }

    /// Ensure `ch` has a color-emoji entry in the RGBA atlas.
    fn ensure_color_emoji_in_atlas(&mut self, gl: &glow::Context, ch: char) {
        // Check atlas
        if self.emoji_atlas.lookup(ch).is_some() {
            return;
        }
        if let Some(img) = self.rasterizer.color_rasterize(ch)
            && let Some(entry) = self.pack_rgba_to_color_atlas(gl, &img)
        {
            self.emoji_atlas.insert(ch, entry);
        }
    }

    /// Push a color-emoji quad for `ch` into the color atlas vertex buffer.
    /// Returns `true` if a color atlas entry exists and was queued for drawing.
    fn push_color_emoji(&mut self, ch: char, x: f32, y: f32, w: f32, h: f32) -> bool {
        let Some(entry) = self.emoji_atlas.lookup(ch) else {
            return false;
        };

        // Scale to fill the full cell height (emoji bitmaps are square).
        // If the height-scaled width would exceed the allocated slot width,
        // scale down to fit the slot instead, preventing left/right overflow.
        let img_w = entry.w_px as f32;
        let img_h = entry.h_px as f32;
        let scale_h = h / img_h;
        let draw_w_by_h = img_w * scale_h;
        let (draw_w, draw_h) = if draw_w_by_h <= w {
            (draw_w_by_h, img_h * scale_h)
        } else {
            let scale_w = w / img_w;
            (w, img_h * scale_w)
        };
        let ox = x + (w - draw_w) * 0.5;
        let oy = y + (h - draw_h) * 0.5;

        let (u0, v0, u1, v1) = (entry.u0, entry.v0, entry.u1, entry.v1);
        let a = 1.0_f32;

        self.batches.emoji.push_quad(
            ox,
            oy,
            ox + draw_w,
            oy + draw_h,
            u0,
            v0,
            u1,
            v1,
            0.0,
            0.0,
            0.0,
            a,
        );

        true
    }

    /// Upload `glyph`'s coverage bitmap to the next free slot in the atlas
    /// texture and return the resulting [`AtlasGlyph`] with UV coordinates.
    /// Returns `None` when the atlas is full.
    fn pack_bitmap_to_atlas(
        &mut self,
        gl: &glow::Context,
        glyph: &GlyphBitmap,
    ) -> Option<AtlasGlyph> {
        let gw = glyph.width as u32;
        let gh = glyph.height as u32;
        if gw == 0 || gh == 0 {
            return None;
        }

        // Use GlyphAtlas for allocation
        let (dest_x, dest_y) = self.glyph_atlas.allocate(gw, gh)?;

        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.gpu_state.glyph.texture));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                dest_x as i32,
                dest_y as i32,
                gw as i32,
                gh as i32,
                glow::RED,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(&glyph.alpha),
            );
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4); // restore default
            gl.bind_texture(glow::TEXTURE_2D, None);
        }

        let atf = ATLAS_TEX_SIZE as f32;
        Some(AtlasGlyph {
            u0: dest_x as f32 / atf,
            v0: dest_y as f32 / atf,
            u1: (dest_x + gw) as f32 / atf,
            v1: (dest_y + gh) as f32 / atf,
            xmin: glyph.xmin,
            ymin: glyph.ymin,
            source_gw: gw as f32,
            source_gh: gh as f32,
            advance_width: glyph.advance_width,
        })
    }

    fn ensure_char_in_atlas(&mut self, gl: &glow::Context, ch: char, style: u8) {
        let style_key = style & (STYLE_BOLD | STYLE_ITALIC);
        // Check atlas
        if self.glyph_atlas.lookup_char(ch, style_key).is_some() {
            return;
        }
        let Some(glyph) = self.rasterizer.glyph(ch, style_key) else {
            return;
        };
        if let Some(ag) = self.pack_bitmap_to_atlas(gl, &glyph) {
            self.glyph_atlas.insert_char(ch, style_key, ag);
        }
    }

    fn ensure_glyph_id_in_atlas(&mut self, gl: &glow::Context, glyph_id: u16, style: u8) {
        if glyph_id == 0 {
            // glyph_id 0 is .notdef in the primary font; skip it so uncovered
            // characters fall back to the character-based rendering path.
            return;
        }
        let style_key = style & STYLE_BOLD;
        // Check atlas
        if self
            .glyph_atlas
            .lookup_glyph_id(glyph_id, style_key)
            .is_some()
        {
            return;
        }
        let Some(glyph) = self.rasterizer.glyph_indexed(glyph_id, style_key) else {
            return;
        };
        if let Some(ag) = self.pack_bitmap_to_atlas(gl, &glyph) {
            self.glyph_atlas.insert_glyph_id(glyph_id, style_key, ag);
        }
    }

    /// Pre-populate the atlas with every glyph that will be drawn this frame
    /// so that [`push_atlas_quad`] needs no GL access at draw time.
    fn warm_atlas(&mut self, gl: &glow::Context, snapshot: &RenderSnapshot) {
        // Shaped terminal glyphs
        let terminal_text = snapshot.terminal_text_from_rows();
        let shaped = self.shape_terminal_lines_cached(snapshot, &terminal_text);
        if let Some(shaped) = shaped {
            for row in shaped.iter() {
                for sg in row {
                    let style = snapshot
                        .terminal_styles
                        .get(sg.full_char_idx)
                        .copied()
                        .unwrap_or(0);
                    if sg.glyph_id == 0 && sg.source_char != ' ' && sg.source_char != '\0' {
                        // Primary font has no glyph — try color emoji first,
                        // then fall back to the outline/char-based atlas.
                        self.ensure_color_emoji_in_atlas(gl, sg.source_char);
                        if self.emoji_atlas.lookup(sg.source_char).is_none() {
                            self.ensure_char_in_atlas(gl, sg.source_char, style);
                        }
                    } else {
                        self.ensure_glyph_id_in_atlas(gl, sg.glyph_id, style);
                        if sg.source_char != ' ' && sg.source_char != '\0' {
                            self.ensure_char_in_atlas(gl, sg.source_char, style);
                        }
                    }
                }
            }
        }

        // Terminal raw chars (used by the Scene-based text rendering path)
        // Always load with style 0 since render_text_simple uses global style.
        {
            for line in terminal_text.lines() {
                for ch in line.chars() {
                    if ch != ' ' && ch != '\0' {
                        // Load with style 0 (default) - matches what render_text_simple uses
                        self.ensure_char_in_atlas(gl, ch, 0);
                    }
                }
            }
        }

        // Editor text
        for ch in snapshot.editor_text.chars() {
            if ch != '\n' && ch != ' ' && ch != '\0' {
                self.ensure_char_in_atlas(gl, ch, 0);
            }
        }

        // Common printable ASCII (covers tab labels, overlays, etc.)
        for cp in 0x21u32..=0x7eu32 {
            if let Some(ch) = char::from_u32(cp) {
                self.ensure_char_in_atlas(gl, ch, 0);
                self.ensure_char_in_atlas(gl, ch, STYLE_BOLD);
            }
        }
    }

    /// Emit a single textured quad (two triangles) for an atlas-backed glyph.
    /// Strikethrough is rendered separately by the caller if needed.
    fn push_atlas_quad(
        &mut self,
        source_char: char,
        ag: AtlasGlyph,
        cell: &GlyphCell,
        x_off: f32,
        y_off: f32,
    ) {
        let GlyphCell {
            x,
            y,
            w,
            h,
            color,
            style,
        } = *cell;
        let gw = ag.source_gw;
        let gh = ag.source_gh;
        let adv_w = ag.advance_width.max(gw).max(1.0);
        let mut scale = ((w * 0.98) / adv_w).min((h * 0.94) / gh).max(0.1);
        if is_icon_like(source_char) {
            let boosted = scale * 1.16;
            let max_fit = ((w * 1.08) / adv_w).min((h * 1.05) / gh).max(scale);
            scale = boosted.min(max_fit);
        }

        let draw_adv = adv_w * scale;
        let baseline_y = y + h * 0.80 - y_off;
        let origin_x = x + (w - draw_adv) * 0.5 + ag.xmin * scale + x_off;
        let origin_y = baseline_y - (gh + ag.ymin) * scale;
        let draw_w = gw * scale;
        let draw_h = gh * scale;

        let [r, g, b, a] = color;
        let italic_shear = if style & STYLE_ITALIC != 0 {
            draw_h * 0.15
        } else {
            0.0
        };

        let tl_x = origin_x + italic_shear;
        let tr_x = origin_x + draw_w + italic_shear;
        let bl_x = origin_x;
        let br_x = origin_x + draw_w;
        let top_y = origin_y;
        let bot_y = origin_y + draw_h;
        let (u0, v0, u1, v1) = (ag.u0, ag.v0, ag.u1, ag.v1);

        self.batches
            .glyph
            .push_quad(tl_x, top_y, br_x, bot_y, u0, v0, u1, v1, r, g, b, a);

        // Synthetic bold: second pass shifted right
        if style & STYLE_BOLD != 0 {
            let shift = (draw_w * 0.08).max(0.5);
            self.batches.glyph.push_quad(
                tl_x + shift,
                top_y,
                br_x + shift,
                bot_y,
                u0,
                v0,
                u1,
                v1,
                r,
                g,
                b,
                a,
            );
        }
    }

    fn push_glyph(&mut self, ch: char, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.push_glyph_styled(
            ch,
            &GlyphCell {
                x,
                y,
                w,
                h,
                color,
                style: 0,
            },
        );
    }

    fn push_glyph_styled(&mut self, ch: char, cell: &GlyphCell) {
        if ch == ' ' || ch == '\0' {
            return;
        }

        if self.push_raster_glyph(ch, cell) {
            if cell.style & STYLE_STRIKE != 0 {
                let strike_h = (cell.h * 0.08).max(1.0);
                let strike_y = cell.y + cell.h * 0.55;
                self.push_rect(
                    cell.x,
                    strike_y,
                    cell.x + cell.w,
                    strike_y + strike_h,
                    cell.color,
                );
            }
            return;
        }

        if let Some(bitmap) = font8x8::BASIC_FONTS.get(ch) {
            let px_w = cell.w / 8.0;
            let px_h = cell.h / 8.0;
            for (gy, bits) in bitmap.iter().enumerate() {
                for gx in 0..8 {
                    if (bits >> gx) & 1 == 1 {
                        let x0 = cell.x + gx as f32 * px_w;
                        let y0 = cell.y + gy as f32 * px_h;
                        self.push_rect(x0, y0, x0 + px_w, y0 + px_h, cell.color);
                    }
                }
            }
        }

        // No glyph available in any font or bitmap table — skip silently.
        // Drawing a box placeholder is more confusing than blank space.
    }

    fn push_raster_glyph(&mut self, ch: char, cell: &GlyphCell) -> bool {
        let style_key = cell.style & (STYLE_BOLD | STYLE_ITALIC);
        if let Some(ag) = self.glyph_atlas.lookup_char(ch, style_key) {
            self.push_atlas_quad(ch, ag, cell, 0.0, 0.0);
            return true;
        }
        // Atlas miss (warm_atlas didn't cover this glyph): fall back to pixel rects.
        let Some(glyph) = self.rasterizer.glyph(ch, cell.style) else {
            return false;
        };
        self.push_bitmap_glyph(&glyph, ch, cell, 0.0, 0.0)
    }

    fn push_shaped_glyph(
        &mut self,
        source_char: char,
        glyph_id: u16,
        cell: &GlyphCell,
        x_offset_px: f32,
        y_offset_px: f32,
    ) -> bool {
        if glyph_id == 0 {
            // glyph_id 0 means the primary font has no glyph for this character.
            // Return false so the caller falls back to character-based rendering.
            return false;
        }
        let style_key = cell.style & STYLE_BOLD;
        if let Some(ag) = self.glyph_atlas.lookup_glyph_id(glyph_id, style_key) {
            self.push_atlas_quad(source_char, ag, cell, x_offset_px, y_offset_px);
            return true;
        }
        // Atlas miss: fall back to pixel rects.
        let Some(glyph) = self.rasterizer.glyph_indexed(glyph_id, cell.style) else {
            return false;
        };
        self.push_bitmap_glyph(&glyph, source_char, cell, x_offset_px, y_offset_px)
    }

    fn push_bitmap_glyph(
        &mut self,
        glyph: &GlyphBitmap,
        source_char: char,
        cell: &GlyphCell,
        x_offset_px: f32,
        y_offset_px: f32,
    ) -> bool {
        if glyph.width == 0 || glyph.height == 0 {
            return false;
        }

        let GlyphCell {
            x,
            y,
            w,
            h,
            color,
            style,
        } = *cell;
        let gw = glyph.width as f32;
        let gh = glyph.height as f32;
        let adv_w = glyph.advance_width.max(gw).max(1.0);
        let mut scale = ((w * 0.98) / adv_w).min((h * 0.94) / gh).max(0.1);

        if is_icon_like(source_char) {
            let boosted = scale * 1.16;
            let max_fit = ((w * 1.08) / adv_w).min((h * 1.05) / gh).max(scale);
            scale = boosted.min(max_fit);
        }

        let draw_adv = adv_w * scale;
        let baseline_y = y + h * 0.80 - y_offset_px;
        let origin_x = x + (w - draw_adv) * 0.5 + glyph.xmin * scale + x_offset_px;
        let origin_y = baseline_y - (gh + glyph.ymin) * scale;
        let pixel = scale.max(0.6);

        for py in 0..glyph.height {
            for px in 0..glyph.width {
                let alpha = glyph.alpha[py * glyph.width + px];
                if alpha < 12 {
                    continue;
                }
                let mut a = color[3] * (alpha as f32 / 255.0);
                if a < 0.02 {
                    continue;
                }

                let italic_shift = if style & STYLE_ITALIC != 0 {
                    (1.0 - py as f32 / gh) * pixel * 0.65
                } else {
                    0.0
                };
                let x0 = origin_x + px as f32 * pixel + italic_shift;
                let y0 = origin_y + py as f32 * pixel;
                self.push_rect(
                    x0,
                    y0,
                    x0 + pixel,
                    y0 + pixel,
                    [color[0], color[1], color[2], a],
                );

                if style & STYLE_BOLD != 0 {
                    a = (a * 0.85).clamp(0.0, 1.0);
                    let bx = x0 + pixel * 0.45;
                    self.push_rect(
                        bx,
                        y0,
                        bx + pixel,
                        y0 + pixel,
                        [color[0], color[1], color[2], a],
                    );
                }
            }
        }

        true
    }

    fn push_rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 4]) {
        if x1 <= x0 || y1 <= y0 {
            return;
        }

        self.batches
            .flat
            .push_quad(x0, y0, x1, y1, color[0], color[1], color[2], color[3]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_scene_commands_are_matched() {
        let mut scene = Scene::new();

        // Create various scene commands
        scene.rect(10.0, 20.0, 100.0, 50.0, [1.0, 0.0, 0.0, 1.0]);
        scene.text(5.0, 15.0, "Test", [0.0, 1.0, 0.0, 1.0]);

        let rect = render_model::Rect::new(0.0, 0.0, 100.0, 100.0);
        scene.clip_push(rect);
        scene.clip_pop();

        assert_eq!(scene.len(), 4);

        // Verify all command types can be matched without panic
        let mut rect_count = 0;
        let mut text_count = 0;
        let mut clip_push_count = 0;
        let mut clip_pop_count = 0;

        for (_, commands) in scene.iter_layers() {
            for command in commands {
                match command {
                    RenderCommand::Rect(_) => rect_count += 1,
                    RenderCommand::Text(_) => text_count += 1,
                    RenderCommand::ClipPush(_) => clip_push_count += 1,
                    RenderCommand::ClipPop => clip_pop_count += 1,
                }
            }
        }

        assert_eq!(rect_count, 1);
        assert_eq!(text_count, 1);
        assert_eq!(clip_push_count, 1);
        assert_eq!(clip_pop_count, 1);
    }

    #[test]
    fn test_render_scene_rect_command_structure() {
        let mut scene = Scene::new();
        let color = [1.0, 0.0, 0.0, 1.0];

        scene.rect(10.0, 20.0, 100.0, 50.0, color);

        assert_eq!(scene.len(), 1);

        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 10.0);
                assert_eq!(cmd.rect.y, 20.0);
                assert_eq!(cmd.rect.w, 100.0);
                assert_eq!(cmd.rect.h, 50.0);
                assert_eq!(cmd.color, color);
            }
            _ => panic!("Expected Rect command"),
        }
    }
}
