use std::collections::HashMap;
use std::mem::size_of;
use std::sync::Arc;

use font8x8::UnicodeFonts;
use glow::HasContext;
use render_wgpu::{RenderSnapshot, SCROLLBAR_W_PX, shell_highlight::highlight_shell};
use winit::dpi::PhysicalSize;

use crate::font::CpuFontRasterizer;
use crate::shaders::{compile_atlas_program, compile_color_atlas_program, compile_program};
use crate::types::{
    ATLAS_TEX_SIZE, AtlasGlyph, COLOR_ATLAS_TEX_SIZE, ColorAtlasEntry, FrameLayout, GlyphBitmap,
    PALETTE_MAX_VISIBLE, SEPARATOR_PX, SETTINGS_MAX_VISIBLE_SEARCH, STYLE_BOLD, STYLE_ITALIC,
    STYLE_STRIKE, ShapedLines, ShapedTerminalCache, TAB_H_MULT,
};
use crate::util::{
    char_col_width, editor_offset_to_row_col, hash_text, is_icon_like, normalize_rect_selection,
};

type Result<T> = anyhow::Result<T>;

// ── GlPainter struct ──────────────────────────────────────────────────────────

pub(crate) struct GlPainter {
    // Flat-colour pipeline (backgrounds, borders, cursor, overlays)
    program: glow::Program,
    vbo: glow::Buffer,
    vao: glow::VertexArray,
    u_screen: Option<glow::UniformLocation>,
    vertices: Vec<f32>,
    // Textured glyph-atlas pipeline
    atlas_texture: glow::Texture,
    atlas_program: glow::Program,
    atlas_vbo: glow::Buffer,
    atlas_vao: glow::VertexArray,
    atlas_u_screen: Option<glow::UniformLocation>,
    atlas_u_sampler: Option<glow::UniformLocation>,
    /// Vertex data for textured glyph quads: [x, y, u, v, r, g, b, a] × 6 per glyph.
    atlas_vertices: Vec<f32>,
    atlas_alloc_x: u32,
    atlas_alloc_y: u32,
    atlas_row_h: u32,
    /// Cached atlas entries keyed by (char, style_mask).
    char_atlas: HashMap<(char, u8), AtlasGlyph>,
    /// Cached atlas entries keyed by (rustybuzz glyph_id, style_mask).
    glyph_id_atlas: HashMap<(u16, u8), AtlasGlyph>,
    // ── Color-emoji RGBA atlas pipeline ──────────────────────────────────
    /// GL_RGBA texture atlas for color emoji bitmaps (SBIX / CBDT strikes).
    color_atlas_texture: glow::Texture,
    color_atlas_program: glow::Program,
    color_atlas_vbo: glow::Buffer,
    color_atlas_vao: glow::VertexArray,
    color_atlas_u_screen: Option<glow::UniformLocation>,
    color_atlas_u_sampler: Option<glow::UniformLocation>,
    color_atlas_vertices: Vec<f32>,
    color_atlas_alloc_x: u32,
    color_atlas_alloc_y: u32,
    color_atlas_row_h: u32,
    /// Cached color atlas entries keyed by char.
    color_char_atlas: HashMap<char, ColorAtlasEntry>,
    rasterizer: CpuFontRasterizer,
    shaped_terminal_cache: Option<ShapedTerminalCache>,
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

        Ok(Self {
            program,
            vbo,
            vao,
            u_screen,
            vertices: Vec::with_capacity(64 * 1024),
            atlas_texture,
            atlas_program,
            atlas_vbo,
            atlas_vao,
            atlas_u_screen,
            atlas_u_sampler,
            atlas_vertices: Vec::with_capacity(64 * 1024),
            atlas_alloc_x: 0,
            atlas_alloc_y: 0,
            atlas_row_h: 0,
            char_atlas: HashMap::new(),
            glyph_id_atlas: HashMap::new(),
            color_atlas_texture,
            color_atlas_program,
            color_atlas_vbo,
            color_atlas_vao,
            color_atlas_u_screen,
            color_atlas_u_sampler,
            color_atlas_vertices: Vec::with_capacity(32 * 1024),
            color_atlas_alloc_x: 0,
            color_atlas_alloc_y: 0,
            color_atlas_row_h: 0,
            color_char_atlas: HashMap::new(),
            rasterizer: CpuFontRasterizer::new(font_family, font_size_px),
            shaped_terminal_cache: None,
        })
    }

    pub(crate) fn set_font_size(&mut self, font_size_px: f32) {
        let old_size = self.rasterizer.font_size_px;
        self.rasterizer.set_font_size(font_size_px);
        self.shaped_terminal_cache = None;
        // If the size actually changed, glyph bitmaps are now wrong size.
        if (old_size - font_size_px).abs() >= 0.5 {
            self.char_atlas.clear();
            self.glyph_id_atlas.clear();
            self.atlas_alloc_x = 0;
            self.atlas_alloc_y = 0;
            self.atlas_row_h = 0;
            // Color atlas entries are also size-dependent.
            self.color_char_atlas.clear();
            self.color_atlas_alloc_x = 0;
            self.color_atlas_alloc_y = 0;
            self.color_atlas_row_h = 0;
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

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub(crate) fn render(
        &mut self,
        gl: &glow::Context,
        snapshot: &RenderSnapshot,
        size: PhysicalSize<u32>,
        cell_w_px: f32,
        cell_h_px: f32,
    ) {
        let width = size.width.max(1) as f32;
        let height = size.height.max(1) as f32;
        let tab_bar_h = if snapshot.tab_labels.is_empty() {
            0.0
        } else {
            (cell_h_px * TAB_H_MULT).max(1.0)
        };
        let available_h = (height - tab_bar_h).max(1.0);
        // When the terminal is in fullscreen / alternate-screen mode (e.g. vim,
        // htop) honour split_ratio=1.0 exactly so the editor pane disappears.
        // For normal use, clamp to [0.05, 0.95] to keep both panes usable.
        let split_ratio = if snapshot.terminal_fullscreen {
            1.0_f32
        } else {
            snapshot.split_ratio.clamp(0.05, 0.95)
        };
        let terminal_h = (tab_bar_h + available_h * split_ratio).floor();
        let editor_top = (terminal_h + SEPARATOR_PX).min(height);
        let terminal_rows = snapshot.terminal_rows_len() as f32;
        let effective_term_h =
            (available_h * split_ratio - 2.0 * snapshot.padding_v as f32).max(0.0);
        let content_h_px = (terminal_rows * cell_h_px).min(effective_term_h);
        let terminal_text_top =
            tab_bar_h + snapshot.padding_v as f32 + (effective_term_h - content_h_px).max(0.0);
        let terminal_text_bottom = terminal_h - snapshot.padding_v as f32;
        let layout = FrameLayout {
            width,
            height,
            tab_bar_h,
            terminal_h,
            editor_top,
            terminal_text_top,
            terminal_text_bottom,
            padding_h: snapshot.padding_h as f32,
            padding_v: snapshot.padding_v as f32,
            cell_w_px,
            cell_h_px,
        };

        self.vertices.clear();
        self.atlas_vertices.clear();

        // Pre-populate atlas with every glyph that appears in this frame.
        self.warm_atlas(gl, snapshot);

        self.push_rect(
            0.0,
            layout.tab_bar_h,
            layout.width,
            layout.terminal_h,
            snapshot.theme.terminal_bg,
        );
        self.push_rect(
            0.0,
            layout.editor_top,
            layout.width,
            layout.height,
            snapshot.theme.editor_bg,
        );
        self.push_rect(
            0.0,
            layout.terminal_h,
            layout.width,
            layout.editor_top,
            if snapshot.editor_focused {
                snapshot.theme.separator_focused
            } else {
                snapshot.theme.separator
            },
        );

        if snapshot.bell_active {
            self.push_rect(
                0.0,
                layout.tab_bar_h,
                layout.width,
                layout.terminal_h,
                [0.60, 0.20, 0.20, 0.15],
            );
        }

        self.draw_tab_bar(snapshot, &layout);
        self.draw_terminal_highlights(snapshot, &layout);
        self.draw_editor_selection(snapshot, &layout);
        self.draw_terminal_text(snapshot, &layout);
        self.draw_editor_text(snapshot, &layout);
        self.draw_editor_suggestion(snapshot, &layout);
        self.draw_cursor(snapshot, &layout);
        self.draw_scrollbar(snapshot, &layout);

        // Flush main-content passes before drawing overlays so that overlay
        // backgrounds (drawn without blending) completely cover terminal text.
        self.flush_passes(gl, layout.width, layout.height);

        self.draw_search_panel(snapshot, &layout);
        self.draw_suggestion_dropdown(snapshot, &layout);
        self.draw_context_menu(snapshot, &layout);
        self.draw_settings_overlay(snapshot, &layout);
        self.draw_command_palette(snapshot, &layout);
        self.draw_toasts(snapshot, &layout);
        self.draw_resize_overlay(snapshot, &layout);
        self.draw_scroll_indicator(snapshot, &layout);

        self.flush_passes(gl, layout.width, layout.height);
    }

    /// Flush accumulated flat-colour vertices then atlas-textured glyph quads
    /// to the GPU, then clear both buffers ready for the next accumulation phase.
    fn flush_passes(&mut self, gl: &glow::Context, width: f32, height: f32) {
        // ── Flush flat-colour geometry (backgrounds, borders, cursor…) ────
        if !self.vertices.is_empty() {
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    self.vertices.as_ptr() as *const u8,
                    self.vertices.len() * size_of::<f32>(),
                )
            };

            unsafe {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

                gl.use_program(Some(self.program));
                gl.bind_vertex_array(Some(self.vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

                if let Some(loc) = self.u_screen.as_ref() {
                    gl.uniform_2_f32(Some(loc), width, height);
                }

                gl.draw_arrays(glow::TRIANGLES, 0, (self.vertices.len() / 6) as i32);

                gl.bind_buffer(glow::ARRAY_BUFFER, None);
                gl.bind_vertex_array(None);
                gl.use_program(None);
                gl.disable(glow::BLEND);
            }
            self.vertices.clear();
        }

        // ── Flush atlas-textured glyph quads (text) ──────────────────────
        if !self.atlas_vertices.is_empty() {
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    self.atlas_vertices.as_ptr() as *const u8,
                    self.atlas_vertices.len() * size_of::<f32>(),
                )
            };

            unsafe {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

                gl.use_program(Some(self.atlas_program));
                gl.bind_vertex_array(Some(self.atlas_vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.atlas_vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

                if let Some(loc) = &self.atlas_u_screen {
                    gl.uniform_2_f32(Some(loc), width, height);
                }

                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas_texture));
                if let Some(loc) = &self.atlas_u_sampler {
                    gl.uniform_1_i32(Some(loc), 0);
                }

                gl.draw_arrays(glow::TRIANGLES, 0, (self.atlas_vertices.len() / 8) as i32);

                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.bind_buffer(glow::ARRAY_BUFFER, None);
                gl.bind_vertex_array(None);
                gl.use_program(None);
                gl.disable(glow::BLEND);
            }
            self.atlas_vertices.clear();
        }

        // ── Flush color-emoji RGBA atlas quads ───────────────────────────────
        if !self.color_atlas_vertices.is_empty() {
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    self.color_atlas_vertices.as_ptr() as *const u8,
                    self.color_atlas_vertices.len() * size_of::<f32>(),
                )
            };

            unsafe {
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);

                gl.use_program(Some(self.color_atlas_program));
                gl.bind_vertex_array(Some(self.color_atlas_vao));
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.color_atlas_vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);

                if let Some(loc) = &self.color_atlas_u_screen {
                    gl.uniform_2_f32(Some(loc), width, height);
                }

                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(self.color_atlas_texture));
                if let Some(loc) = &self.color_atlas_u_sampler {
                    gl.uniform_1_i32(Some(loc), 0);
                }

                gl.draw_arrays(
                    glow::TRIANGLES,
                    0,
                    (self.color_atlas_vertices.len() / 8) as i32,
                );

                gl.bind_texture(glow::TEXTURE_2D, None);
                gl.bind_buffer(glow::ARRAY_BUFFER, None);
                gl.bind_vertex_array(None);
                gl.use_program(None);
                gl.disable(glow::BLEND);
            }
            self.color_atlas_vertices.clear();
        }
    }

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

        // Draw background color cells first so shaped glyph runs can paint on top.
        // Clip on the row's *top* edge so floating-point rounding in terminal_h
        // never silently drops the last row (its bottom may exceed max_y by <1px).
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
                    self.push_rect(
                        x,
                        y,
                        x + layout.cell_w_px,
                        y + layout.cell_h_px,
                        [bg[0], bg[1], bg[2], 1.0],
                    );
                }
            }

            line_char_start = line_char_start.saturating_add(line.chars().count() + 1);
        }

        line_char_start = 0;
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

                    let fg = snapshot
                        .terminal_fg_colors
                        .get(sg.full_char_idx)
                        .and_then(|c| *c)
                        .map(|c| [c[0], c[1], c[2], 1.0])
                        .unwrap_or(fallback_fg);
                    let style = snapshot
                        .terminal_styles
                        .get(sg.full_char_idx)
                        .copied()
                        .unwrap_or(0);

                    if sg.glyph_id == 0 {
                        // Primary font has no glyph for this character.
                        // Try the color-emoji atlas (SBIX / CBDT), then the
                        // outline char atlas, and finally silently skip.
                        if !self.push_color_emoji(sg.source_char, x, y, w, layout.cell_h_px) {
                            self.push_glyph_styled(
                                sg.source_char,
                                x,
                                y,
                                w,
                                layout.cell_h_px,
                                fg,
                                style,
                            );
                        }
                    } else if !self.push_shaped_glyph(
                        sg.source_char,
                        sg.glyph_id,
                        x,
                        y,
                        w,
                        layout.cell_h_px,
                        fg,
                        style,
                        sg.x_offset_px,
                        sg.y_offset_px,
                    ) {
                        self.push_glyph_styled(
                            sg.source_char,
                            x,
                            y,
                            w,
                            layout.cell_h_px,
                            fg,
                            style,
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
                    let fg = snapshot
                        .terminal_fg_colors
                        .get(idx)
                        .and_then(|c| *c)
                        .map(|c| [c[0], c[1], c[2], 1.0])
                        .unwrap_or(fallback_fg);
                    let style = snapshot.terminal_styles.get(idx).copied().unwrap_or(0);

                    self.push_glyph_styled(ch, x, y, layout.cell_w_px, layout.cell_h_px, fg, style);
                }
            }

            line_char_start = line_char_start.saturating_add(line.chars().count() + 1);
        }
    }

    fn draw_editor_text(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        // The editor has a 2-column prompt prefix "❯ " on the first visible
        // line, matching the wgpu renderer (EDITOR_PREFIX_COLS = 2).
        const PREFIX: &str = "\u{276f} ";
        const PREFIX_COLOR: [f32; 4] = [0.40, 0.80, 1.00, 1.0];
        const EDITOR_PREFIX_COLS: usize = 2;

        let default_fg = [
            snapshot.theme.text[0],
            snapshot.theme.text[1],
            snapshot.theme.text[2],
            1.0,
        ];
        let max_x = layout.width - layout.padding_h;
        let max_y = layout.height - layout.padding_v;
        let row_offset = snapshot.editor_scroll_offset;
        let hl = highlight_shell(&snapshot.editor_text);
        let mut char_idx = 0usize; // tracks index into `hl` as we iterate chars

        // Draw "❯ " prefix on the first visible line only.
        if row_offset == 0 {
            let y = layout.editor_top + layout.padding_v;
            for (ci, ch) in PREFIX.chars().enumerate() {
                let x = layout.padding_h + ci as f32 * layout.cell_w_px;
                self.push_glyph(ch, x, y, layout.cell_w_px, layout.cell_h_px, PREFIX_COLOR);
            }
        }

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
            // Row 0 starts after the prefix; subsequent rows start at col 0.
            let col_offset = if line_idx == 0 { EDITOR_PREFIX_COLS } else { 0 };
            let mut vcol = col_offset; // visual column (accounts for wide chars)
            for ch in line.chars() {
                let cw = char_col_width(ch);
                let x = layout.padding_h + vcol as f32 * layout.cell_w_px;
                if x + layout.cell_w_px > max_x {
                    break;
                }
                let fg = hl
                    .get(char_idx)
                    .and_then(|c| *c)
                    .map(|c| [c[0], c[1], c[2], 1.0])
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
        const EDITOR_PREFIX_COLS: usize = 2;
        let (row, col) =
            editor_offset_to_row_col(&snapshot.editor_text, snapshot.editor_cursor_offset);
        let visible_row = row.saturating_sub(snapshot.editor_scroll_offset);
        let y = layout.editor_top + layout.padding_v + visible_row as f32 * layout.cell_h_px;
        // Offset the column by the prefix width on the first editor line.
        let col_offset = if row == 0 { EDITOR_PREFIX_COLS } else { 0 };
        let base_x = layout.padding_h + (col + col_offset) as f32 * layout.cell_w_px;
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
        use crate::types::{clamp_color, mix_color};
        if snapshot.tab_labels.is_empty() || layout.tab_bar_h <= 0.0 {
            return;
        }
        let tab_bar_bg = clamp_color(snapshot.theme.terminal_bg, 0.05);
        let tab_inactive = clamp_color(snapshot.theme.terminal_bg, 0.02);
        let tab_active = mix_color(tab_bar_bg, snapshot.theme.separator_focused, 0.22);
        let add_btn_bg = [
            (snapshot.theme.terminal_bg[0] + 0.05).clamp(0.0, 1.0),
            (snapshot.theme.terminal_bg[1] + 0.10).clamp(0.0, 1.0),
            (snapshot.theme.terminal_bg[2] + 0.03).clamp(0.0, 1.0),
            0.90,
        ];
        self.push_rect(0.0, 0.0, layout.width, layout.tab_bar_h, tab_bar_bg);

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
            self.push_rect(x0, y0, x1, y1, color);

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
            add_btn_bg,
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
                snapshot.theme.separator_focused,
            );
        }
    }

    #[allow(clippy::too_many_lines)]
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
        const EDITOR_PREFIX_COLS: usize = 2;
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
                    let col_offset = if line_idx == 0 { EDITOR_PREFIX_COLS } else { 0 };
                    let y = layout.editor_top + layout.padding_v + row as f32 * layout.cell_h_px;
                    let x0 = layout.padding_h + (from + col_offset) as f32 * layout.cell_w_px;
                    let x1 = layout.padding_h + (to + col_offset) as f32 * layout.cell_w_px;
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

    #[allow(clippy::too_many_lines)]
    fn draw_settings_overlay(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        use crate::types::{clamp_color, mix_color};
        let Some(overlay) = &snapshot.settings_overlay else {
            return;
        };

        // Semi-transparent scrim so content behind is dimmed but still visible.
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
        let x1 = x0 + panel_w;
        let y1 = y0 + panel_h;

        let ov_bg = clamp_color(snapshot.theme.terminal_bg, 0.01);
        let ov_border = snapshot.theme.separator_focused;
        let ov_title = clamp_color(snapshot.theme.terminal_bg, -0.01);
        let ov_section = clamp_color(snapshot.theme.terminal_bg, 0.04);
        let ov_select = mix_color(
            clamp_color(snapshot.theme.terminal_bg, 0.08),
            snapshot.theme.separator_focused,
            0.20,
        );
        let ov_edit = mix_color(
            clamp_color(snapshot.theme.terminal_bg, 0.08),
            snapshot.theme.separator_focused,
            0.28,
        );

        self.push_rect(x0 - 2.0, y0 - 2.0, x1 + 2.0, y1 + 2.0, ov_border);
        self.push_rect(x0, y0, x1, y1, ov_bg);
        self.push_rect(x0, y0, x1, y0 + title_h, ov_title);

        // --- Row highlight backgrounds ---
        let mut editable_idx = 0usize;
        for (i, item) in overlay.items.iter().enumerate() {
            let ry = y0 + title_h + i as f32 * row_h;
            if item.is_header {
                self.push_rect(x0, ry, x1, ry + row_h, ov_section);
            } else {
                if editable_idx == overlay.cursor {
                    self.push_rect(
                        x0,
                        ry,
                        x1,
                        ry + row_h,
                        if overlay.editing.is_some() {
                            ov_edit
                        } else {
                            ov_select
                        },
                    );
                }
                editable_idx += 1;
            }
        }

        // Edit-mode input row background (below all items).
        if overlay.editing.is_some() {
            let ey = y0 + title_h + overlay.items.len() as f32 * row_h;
            self.push_rect(x0, ey, x1, ey + edit_h, ov_edit);
        }

        // Search-dropdown background (rendered above everything else so it occludes rows below).
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
            let dy = y0 + title_h + (focused_flat + 1) as f32 * row_h;
            let dh = row_h * visible as f32;
            self.push_rect(
                x0 - 1.0,
                dy - 1.0,
                x1 + 1.0,
                dy + dh + 1.0,
                [0.35, 0.50, 0.82, 1.0],
            );
            self.push_rect(x0, dy, x1, dy + dh, [0.15, 0.19, 0.30, 1.0]);
            let vis_sel = overlay
                .search_selected
                .saturating_sub(overlay.search_scroll_offset);
            if !overlay.search_matches.is_empty() && vis_sel < visible {
                let sy0 = dy + vis_sel as f32 * row_h;
                self.push_rect(x0, sy0, x1, sy0 + row_h, [0.22, 0.34, 0.62, 1.0]);
            }
        }

        // --- Title text ---
        let title_text = if overlay.just_saved {
            "  SETTINGS  \u{2713} Saved"
        } else {
            "  SETTINGS  (Cmd+,)"
        };
        let th = &snapshot.theme;
        let ty = y0 + (title_h - layout.cell_h_px) * 0.5;
        let mut tx = x0;
        for ch in title_text.chars() {
            self.push_glyph(ch, tx, ty, layout.cell_w_px, layout.cell_h_px, th.text);
            tx += layout.cell_w_px;
        }

        // --- Row text (two columns: key left, value right) ---
        let key_col = x0 + layout.cell_w_px * 1.5;
        let val_col = x0 + panel_w * 0.50;

        // Index of the focused item in terms of total (flat) overlay.items position,
        // used to determine which rows are obscured by the search dropdown.
        let pre_focused_flat = focused_flat;
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
            let row_y = y0 + title_h + i as f32 * row_h + (row_h - layout.cell_h_px) / 2.0;
            if item.is_header {
                // Skip header rows visually covered by the search dropdown.
                if overlay.search_buf.is_some() && i > pre_focused_flat && i <= search_cover_end {
                    continue;
                }
                let mut kx = key_col;
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
                // Skip rows hidden under the search dropdown.
                if overlay.search_buf.is_some() && i > pre_focused_flat && i <= search_cover_end {
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
                // Key column.
                let mut kx = key_col;
                for ch in item.key.chars() {
                    self.push_glyph(ch, kx, row_y, layout.cell_w_px, layout.cell_h_px, key_color);
                    kx += layout.cell_w_px;
                }
                // Determine value string to display.
                let search_val_buf: Option<String> =
                    if item.is_searchable && is_focused && overlay.search_buf.is_some() {
                        let sbuf = overlay.search_buf.as_deref().unwrap_or("");
                        Some(format!("/ {}\u{258e}", sbuf))
                    } else {
                        None
                    };
                let searchable_hint: Option<String> =
                    if item.is_searchable && search_val_buf.is_none() {
                        Some(format!("{} /", item.value))
                    } else {
                        None
                    };
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
                // Value column.
                let mut vx = val_col;
                for ch in display_val.chars() {
                    self.push_glyph(ch, vx, row_y, layout.cell_w_px, layout.cell_h_px, val_color);
                    vx += layout.cell_w_px;
                }
            }
        }

        // --- Footer help text ---
        if overlay.search_buf.is_none() {
            let footer_y = y0
                + title_h
                + overlay.items.len() as f32 * row_h
                + edit_h
                + (footer_h - layout.cell_h_px) / 2.0;
            let footer_text = if overlay.editing.is_some() {
                "  Enter: confirm   Esc: cancel"
            } else {
                "  \u{2191}\u{2193} navigate   \u{2190}\u{2192} change   Enter: edit/search   Esc: close & save"
            };
            let [r, g, b, _] = th.text;
            let foot_color = [r * 0.55, g * 0.55, b * 0.55, 0.90_f32];
            let mut fx = x0;
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
        }

        // --- Search dropdown text ---
        if overlay.search_buf.is_some() {
            let n_visible = overlay
                .search_matches
                .len()
                .saturating_sub(overlay.search_scroll_offset)
                .min(SEARCH_MAX_VISIBLE);
            let visible_end = overlay.search_scroll_offset + n_visible;
            let vis_sel = overlay
                .search_selected
                .saturating_sub(overlay.search_scroll_offset);
            let drop_top_px = y0 + title_h + (focused_flat_idx + 1) as f32 * row_h;
            if overlay.search_matches.is_empty() {
                let item_y = drop_top_px + (row_h - layout.cell_h_px) / 2.0;
                let [r, g, b, _] = th.text;
                let c = [r * 0.45, g * 0.45, b * 0.45, 0.70];
                let mut sx = key_col;
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
                    let item_y = drop_top_px + i as f32 * row_h + (row_h - layout.cell_h_px) / 2.0;
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
                    let mut sx = key_col;
                    for ch in labeled.chars() {
                        self.push_glyph(ch, sx, item_y, layout.cell_w_px, layout.cell_h_px, color);
                        sx += layout.cell_w_px;
                    }
                }
            }
        }
    }

    fn draw_command_palette(&mut self, snapshot: &RenderSnapshot, layout: &FrameLayout) {
        let Some(cp) = &snapshot.command_palette else {
            return;
        };
        let visible = cp
            .items
            .len()
            .saturating_sub(cp.scroll_offset)
            .min(PALETTE_MAX_VISIBLE);
        let palette_w = layout.cell_w_px * 50.0;
        let header_h = layout.cell_h_px * 2.2;
        let item_h = layout.cell_h_px * 1.4;
        let palette_h = header_h + item_h * visible as f32;
        let cx = layout.width * 0.5;
        let x0 = (cx - palette_w * 0.5).max(0.0);
        let x1 = (cx + palette_w * 0.5).min(layout.width);
        let y0 = layout.tab_bar_h + layout.height * 0.08;
        let y1 = (y0 + palette_h).min(layout.height);
        self.push_rect(
            x0 - 2.0,
            y0 - 2.0,
            x1 + 2.0,
            y1 + 2.0,
            [0.35, 0.55, 0.90, 1.0],
        );
        self.push_rect(x0, y0, x1, y1, [0.09, 0.11, 0.18, 0.97]);
        self.push_rect(
            x0,
            y0 + header_h - 1.0,
            x1,
            y0 + header_h,
            [0.30, 0.45, 0.70, 0.80],
        );
        let query = format!("> {}", cp.query);
        for (ci, ch) in query.chars().take(48).enumerate() {
            self.push_glyph(
                ch,
                x0 + layout.cell_w_px * 0.8 + ci as f32 * layout.cell_w_px,
                y0 + (header_h - layout.cell_h_px) * 0.5,
                layout.cell_w_px,
                layout.cell_h_px,
                [0.92, 0.94, 0.98, 1.0],
            );
        }
        for i in 0..visible {
            let idx = cp.scroll_offset + i;
            if idx >= cp.items.len() {
                break;
            }
            let ry = y0 + header_h + i as f32 * item_h;
            if idx == cp.selected {
                self.push_rect(x0, ry, x1, ry + item_h, [0.20, 0.32, 0.58, 0.70]);
            }
            for (ci, ch) in cp.items[idx].chars().take(48).enumerate() {
                self.push_glyph(
                    ch,
                    x0 + layout.cell_w_px * 0.8 + ci as f32 * layout.cell_w_px,
                    ry + (item_h - layout.cell_h_px) * 0.5,
                    layout.cell_w_px,
                    layout.cell_h_px,
                    [0.92, 0.94, 0.98, 1.0],
                );
            }
        }
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
                render_wgpu::ToastKind::Info => (
                    [0.12, 0.15, 0.25, 0.93],
                    [0.35, 0.50, 0.90, 1.0],
                    [0.92, 0.94, 0.98, 1.0],
                ),
                render_wgpu::ToastKind::Success => (
                    [0.08, 0.20, 0.10, 0.93],
                    [0.25, 0.78, 0.35, 1.0],
                    [0.90, 1.00, 0.90, 1.0],
                ),
                render_wgpu::ToastKind::Warn => (
                    [0.22, 0.18, 0.05, 0.93],
                    [0.90, 0.72, 0.20, 1.0],
                    [1.00, 0.97, 0.85, 1.0],
                ),
                render_wgpu::ToastKind::Error => (
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
        if snapshot.scrollback_lines == 0 {
            return;
        }
        let sb_w = SCROLLBAR_W_PX;
        let track_top = layout.tab_bar_h;
        let track_bottom = layout.terminal_h;
        let track_h = track_bottom - track_top;
        if track_h <= 0.0 || layout.cell_h_px <= 0.0 {
            return;
        }
        let sb_left = layout.width - sb_w;

        // Track
        self.push_rect(
            sb_left,
            track_top,
            layout.width,
            track_bottom,
            snapshot.theme.separator,
        );

        // Thumb: fraction = visible_rows / total_rows, clamped to [5%, 100%]
        let term_h_px = layout.terminal_h - layout.tab_bar_h;
        let visible_rows = (term_h_px / layout.cell_h_px).floor();
        let total_rows = visible_rows + snapshot.scrollback_lines as f32;
        let thumb_fraction = (visible_rows / total_rows).clamp(0.05, 1.0);
        let thumb_h = thumb_fraction * track_h;
        let scrollable_h = track_h - thumb_h;

        // scroll_offset=0 → thumb at bottom (newest); max → thumb at top (oldest)
        let scroll_pos =
            (snapshot.scroll_offset as f32 / snapshot.scrollback_lines as f32).clamp(0.0, 1.0);
        let thumb_top = track_top + (1.0 - scroll_pos) * scrollable_h;
        let thumb_bottom = thumb_top + thumb_h;

        let [r, g, b, _] = snapshot.theme.separator_focused;
        self.push_rect(
            sb_left,
            thumb_top,
            layout.width,
            thumb_bottom,
            [r, g, b, 0.85],
        );
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

        if snapshot.editor_focused && !snapshot.terminal_fullscreen {
            const EDITOR_PREFIX_COLS: usize = 2;
            let (row, col) =
                editor_offset_to_row_col(&snapshot.editor_text, snapshot.editor_cursor_offset);
            let visible_row = row.saturating_sub(snapshot.editor_scroll_offset);
            let col_offset = if row == 0 { EDITOR_PREFIX_COLS } else { 0 };
            let x = layout.padding_h + (col + col_offset) as f32 * layout.cell_w_px;
            let y = layout.editor_top + layout.padding_v + visible_row as f32 * layout.cell_h_px;
            let w = (layout.cell_w_px * 0.12).max(2.0);
            self.push_rect(x, y, x + w, y + layout.cell_h_px, color);
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

        if self.color_atlas_alloc_x + w + 1 > COLOR_ATLAS_TEX_SIZE {
            self.color_atlas_alloc_y += self.color_atlas_row_h + 1;
            self.color_atlas_alloc_x = 0;
            self.color_atlas_row_h = 0;
        }
        if self.color_atlas_alloc_y + h > COLOR_ATLAS_TEX_SIZE {
            return None; // Atlas full
        }

        let dest_x = self.color_atlas_alloc_x;
        let dest_y = self.color_atlas_alloc_y;

        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.color_atlas_texture));
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

        self.color_atlas_alloc_x += w + 1;
        if h > self.color_atlas_row_h {
            self.color_atlas_row_h = h;
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
        if self.color_char_atlas.contains_key(&ch) {
            return;
        }
        if let Some(img) = self.rasterizer.color_rasterize(ch)
            && let Some(entry) = self.pack_rgba_to_color_atlas(gl, &img)
        {
            self.color_char_atlas.insert(ch, entry);
        }
    }

    /// Push a color-emoji quad for `ch` into the color atlas vertex buffer.
    /// Returns `true` if a color atlas entry exists and was queued for drawing.
    fn push_color_emoji(&mut self, ch: char, x: f32, y: f32, w: f32, h: f32) -> bool {
        let Some(entry) = self.color_char_atlas.get(&ch).copied() else {
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

        // 6 vertices; dummy rgb (0,0,0) — fragment shader uses texture color.
        #[rustfmt::skip]
        self.color_atlas_vertices.extend_from_slice(&[
            ox,          oy,          u0, v0, 0.0, 0.0, 0.0, a,
            ox + draw_w, oy,          u1, v0, 0.0, 0.0, 0.0, a,
            ox + draw_w, oy + draw_h, u1, v1, 0.0, 0.0, 0.0, a,
            ox,          oy,          u0, v0, 0.0, 0.0, 0.0, a,
            ox + draw_w, oy + draw_h, u1, v1, 0.0, 0.0, 0.0, a,
            ox,          oy + draw_h, u0, v1, 0.0, 0.0, 0.0, a,
        ]);
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

        // Shelf-packing allocator
        if self.atlas_alloc_x + gw + 1 > ATLAS_TEX_SIZE {
            self.atlas_alloc_y += self.atlas_row_h + 1;
            self.atlas_alloc_x = 0;
            self.atlas_row_h = 0;
        }
        if self.atlas_alloc_y + gh > ATLAS_TEX_SIZE {
            return None; // Atlas full
        }

        let dest_x = self.atlas_alloc_x;
        let dest_y = self.atlas_alloc_y;

        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas_texture));
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

        self.atlas_alloc_x += gw + 1;
        if gh > self.atlas_row_h {
            self.atlas_row_h = gh;
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
        if self.char_atlas.contains_key(&(ch, style_key)) {
            return;
        }
        let Some(glyph) = self.rasterizer.glyph(ch, style_key) else {
            return;
        };
        if let Some(ag) = self.pack_bitmap_to_atlas(gl, &glyph) {
            self.char_atlas.insert((ch, style_key), ag);
        }
    }

    fn ensure_glyph_id_in_atlas(&mut self, gl: &glow::Context, glyph_id: u16, style: u8) {
        if glyph_id == 0 {
            // glyph_id 0 is .notdef in the primary font; skip it so uncovered
            // characters fall back to the character-based rendering path.
            return;
        }
        let style_key = style & STYLE_BOLD;
        if self.glyph_id_atlas.contains_key(&(glyph_id, style_key)) {
            return;
        }
        let Some(glyph) = self.rasterizer.glyph_indexed(glyph_id, style_key) else {
            return;
        };
        if let Some(ag) = self.pack_bitmap_to_atlas(gl, &glyph) {
            self.glyph_id_atlas.insert((glyph_id, style_key), ag);
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
                        if !self.color_char_atlas.contains_key(&sg.source_char) {
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

        // Terminal raw chars (unshaped fallback path)
        {
            let mut idx = 0usize;
            for line in terminal_text.lines() {
                for ch in line.chars() {
                    if ch != ' ' && ch != '\0' {
                        let style = snapshot.terminal_styles.get(idx).copied().unwrap_or(0);
                        self.ensure_char_in_atlas(gl, ch, style);
                    }
                    idx += 1;
                }
                idx += 1; // newline
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
    #[allow(clippy::too_many_arguments)]
    fn push_atlas_quad(
        &mut self,
        source_char: char,
        ag: AtlasGlyph,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        style: u8,
        x_off: f32,
        y_off: f32,
    ) {
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

        // 6 vertices (two CCW triangles); 8 floats each: x, y, u, v, r, g, b, a
        self.atlas_vertices.extend_from_slice(&[
            tl_x, top_y, u0, v0, r, g, b, a, tr_x, top_y, u1, v0, r, g, b, a, br_x, bot_y, u1, v1,
            r, g, b, a, tl_x, top_y, u0, v0, r, g, b, a, br_x, bot_y, u1, v1, r, g, b, a, bl_x,
            bot_y, u0, v1, r, g, b, a,
        ]);

        // Synthetic bold: second pass shifted right
        if style & STYLE_BOLD != 0 {
            let shift = (draw_w * 0.08).max(0.5);
            self.atlas_vertices.extend_from_slice(&[
                tl_x + shift,
                top_y,
                u0,
                v0,
                r,
                g,
                b,
                a,
                tr_x + shift,
                top_y,
                u1,
                v0,
                r,
                g,
                b,
                a,
                br_x + shift,
                bot_y,
                u1,
                v1,
                r,
                g,
                b,
                a,
                tl_x + shift,
                top_y,
                u0,
                v0,
                r,
                g,
                b,
                a,
                br_x + shift,
                bot_y,
                u1,
                v1,
                r,
                g,
                b,
                a,
                bl_x + shift,
                bot_y,
                u0,
                v1,
                r,
                g,
                b,
                a,
            ]);
        }
    }

    fn push_glyph(&mut self, ch: char, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.push_glyph_styled(ch, x, y, w, h, color, 0);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_glyph_styled(
        &mut self,
        ch: char,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        style: u8,
    ) {
        if ch == ' ' || ch == '\0' {
            return;
        }

        if self.push_raster_glyph(ch, x, y, w, h, color, style) {
            if style & STYLE_STRIKE != 0 {
                let strike_h = (h * 0.08).max(1.0);
                let strike_y = y + h * 0.55;
                self.push_rect(x, strike_y, x + w, strike_y + strike_h, color);
            }
            return;
        }

        if let Some(bitmap) = font8x8::BASIC_FONTS.get(ch) {
            let px_w = w / 8.0;
            let px_h = h / 8.0;
            for (gy, bits) in bitmap.iter().enumerate() {
                for gx in 0..8 {
                    if (bits >> gx) & 1 == 1 {
                        let x0 = x + gx as f32 * px_w;
                        let y0 = y + gy as f32 * px_h;
                        self.push_rect(x0, y0, x0 + px_w, y0 + px_h, color);
                    }
                }
            }
            return;
        }

        // Fallback for glyphs outside the tiny bitmap table.
        let inset_x = w * 0.2;
        let inset_y = h * 0.2;
        self.push_rect(
            x + inset_x,
            y + inset_y,
            x + w - inset_x,
            y + h - inset_y,
            color,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_raster_glyph(
        &mut self,
        ch: char,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        style: u8,
    ) -> bool {
        let style_key = style & (STYLE_BOLD | STYLE_ITALIC);
        if let Some(ag) = self.char_atlas.get(&(ch, style_key)).copied() {
            self.push_atlas_quad(ch, ag, x, y, w, h, color, style, 0.0, 0.0);
            return true;
        }
        // Atlas miss (warm_atlas didn't cover this glyph): fall back to pixel rects.
        let Some(glyph) = self.rasterizer.glyph(ch, style) else {
            return false;
        };
        self.push_bitmap_glyph(&glyph, ch, x, y, w, h, color, style, 0.0, 0.0)
    }

    #[allow(clippy::too_many_arguments)]
    fn push_shaped_glyph(
        &mut self,
        source_char: char,
        glyph_id: u16,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        style: u8,
        x_offset_px: f32,
        y_offset_px: f32,
    ) -> bool {
        if glyph_id == 0 {
            // glyph_id 0 means the primary font has no glyph for this character.
            // Return false so the caller falls back to character-based rendering.
            return false;
        }
        let style_key = style & STYLE_BOLD;
        if let Some(ag) = self.glyph_id_atlas.get(&(glyph_id, style_key)).copied() {
            self.push_atlas_quad(
                source_char,
                ag,
                x,
                y,
                w,
                h,
                color,
                style,
                x_offset_px,
                y_offset_px,
            );
            return true;
        }
        // Atlas miss: fall back to pixel rects.
        let Some(glyph) = self.rasterizer.glyph_indexed(glyph_id, style) else {
            return false;
        };
        self.push_bitmap_glyph(
            &glyph,
            source_char,
            x,
            y,
            w,
            h,
            color,
            style,
            x_offset_px,
            y_offset_px,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn push_bitmap_glyph(
        &mut self,
        glyph: &GlyphBitmap,
        source_char: char,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        style: u8,
        x_offset_px: f32,
        y_offset_px: f32,
    ) -> bool {
        if glyph.width == 0 || glyph.height == 0 {
            return false;
        }

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

        // two triangles, CCW winding
        self.vertices.extend_from_slice(&[
            x0, y0, color[0], color[1], color[2], color[3], x1, y0, color[0], color[1], color[2],
            color[3], x1, y1, color[0], color[1], color[2], color[3], x0, y0, color[0], color[1],
            color[2], color[3], x1, y1, color[0], color[1], color[2], color[3], x0, y1, color[0],
            color[1], color[2], color[3],
        ]);
    }
}
