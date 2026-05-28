use std::collections::HashMap;

use anyhow::{Context, Result};
use wgpu::SurfaceError;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::atlas::{load_font_bytes, pack_glyph, CachedGlyph, TEXT_ATLAS_SIZE};
use crate::geometry::{
    add_text_verts, build_panel_vertices, build_settings_overlay_bg_verts,
    build_suggestion_dropdown_bg_verts, floats_as_bytes,
    SHADER_WGSL, TEXT_SHADER_WGSL, TEXT_VERTEX_BUF_CAPACITY, VERTEX_BUF_CAPACITY,
};
use crate::types::{ColorTheme, RenderConfig, RenderSnapshot, VsyncMode};

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
    atlas_texture: wgpu::Texture,
    atlas_bind_group: wgpu::BindGroup,
    font: Option<fontdue::Font>,
    font_size: f32,
    pub(crate) cell_w_px: f32,
    pub(crate) cell_h_px: f32,
    glyph_cache: HashMap<char, CachedGlyph>,
    atlas_alloc_x: u32,
    atlas_alloc_y: u32,
    atlas_row_h: u32,
    theme: ColorTheme,
}

impl<'a> GpuState<'a> {
    pub(crate) async fn new(window: &'a Window, render_config: &RenderConfig) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window).context("create wgpu surface")?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .context("request adapter")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("teletipo-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .context("request device")?;

        let caps = surface.get_capabilities(&adapter);
        // Prefer a non-sRGB surface format.  Our colours are already in sRGB space
        // (theme hex values, ANSI palette entries).  Choosing an sRGB target would
        // cause the GPU to apply an additional linear→sRGB gamma encode step, making
        // every colour appear significantly lighter / "washed out".
        let format = caps
            .formats
            .iter()
            .find(|f| !f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let present_mode = match render_config.vsync {
            VsyncMode::On => wgpu::PresentMode::Fifo,
            VsyncMode::Off => wgpu::PresentMode::Immediate,
            VsyncMode::Adaptive => wgpu::PresentMode::AutoVsync,
        };

        let mut config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        if !caps.present_modes.contains(&present_mode) {
            config.present_mode = wgpu::PresentMode::Fifo;
        }
        surface.configure(&device, &config);

        // Background (flat-color) pipeline
        let bg_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("teletipo-bg-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_WGSL.into()),
        });
        let bg_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("teletipo-bg-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let bg_vattrs = [
            wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { offset: 8, shader_location: 1, format: wgpu::VertexFormat::Float32x4 },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("teletipo-bg-pipeline"),
            layout: Some(&bg_layout),
            vertex: wgpu::VertexState {
                module: &bg_shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &bg_vattrs,
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &bg_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });
        let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("teletipo-vertex-buf"),
            size: VERTEX_BUF_CAPACITY,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Glyph atlas texture
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("teletipo-atlas"),
            size: wgpu::Extent3d { width: TEXT_ATLAS_SIZE, height: TEXT_ATLAS_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view    = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let atlas_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("teletipo-atlas-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("teletipo-atlas-bg"),
            layout: &atlas_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&atlas_sampler) },
            ],
        });

        // Text (atlas-sampled) pipeline
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("teletipo-text-shader"),
            source: wgpu::ShaderSource::Wgsl(TEXT_SHADER_WGSL.into()),
        });
        let text_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("teletipo-text-layout"),
            bind_group_layouts: &[&atlas_bgl],
            push_constant_ranges: &[],
        });
        let text_vattrs = [
            wgpu::VertexAttribute { offset:  0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { offset:  8, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { offset: 16, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
        ];
        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("teletipo-text-pipeline"),
            layout: Some(&text_layout),
            vertex: wgpu::VertexState {
                module: &text_shader,
                entry_point: "vs_text",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &text_vattrs,
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: "fs_text",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });
        let text_vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("teletipo-text-vertex-buf"),
            size: TEXT_VERTEX_BUF_CAPACITY,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Font loading and ASCII glyph pre-rasterization
        let font_size = render_config.font.font_size;
        let font_bytes = load_font_bytes(&render_config.font);
        let font = font_bytes.as_ref().and_then(|bytes| {
            fontdue::Font::from_bytes(bytes.as_slice(), fontdue::FontSettings::default()).ok()
        });
        let (cell_w_px, cell_h_px) = font
            .as_ref()
            .map(|f| (f.metrics('M', font_size).advance_width, font_size * 1.2))
            .unwrap_or((font_size * 0.6, font_size * 1.2));

        let mut glyph_cache: HashMap<char, CachedGlyph> = HashMap::new();
        let mut atlas_alloc_x = 0u32;
        let mut atlas_alloc_y = 0u32;
        let mut atlas_row_h   = 0u32;
        if let Some(ref f) = font {
            for ch in ' '..='~' {
                let (metrics, bitmap) = f.rasterize(ch, font_size);
                let cached = pack_glyph(
                    &queue, &atlas_texture,
                    &mut atlas_alloc_x, &mut atlas_alloc_y, &mut atlas_row_h,
                    &metrics, &bitmap, cell_h_px,
                );
                glyph_cache.insert(ch, cached);
            }
        }

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
            atlas_alloc_x,
            atlas_alloc_y,
            atlas_row_h,
            theme: render_config.theme.clone(),
        })
    }

    pub(crate) fn rescale_font(&mut self, base_font_size: f32, scale_factor: f64) {
        let new_size = base_font_size * scale_factor as f32;
        if (new_size - self.font_size).abs() < 0.5 {
            return;
        }
        self.font_size = new_size;
        self.cell_h_px = new_size * 1.2;
        if let Some(ref f) = self.font {
            self.cell_w_px = f.metrics('M', new_size).advance_width;
        }
        self.glyph_cache.clear();
        self.atlas_alloc_x = 0;
        self.atlas_alloc_y = 0;
        self.atlas_row_h = 0;
    }

    pub(crate) fn ensure_glyph(&mut self, ch: char) {
        if self.glyph_cache.contains_key(&ch) {
            return;
        }
        let font = match self.font.take() {
            Some(f) => f,
            None => return,
        };
        let (metrics, bitmap) = font.rasterize(ch, self.font_size);
        let cached = pack_glyph(
            &self.queue,
            &self.atlas_texture,
            &mut self.atlas_alloc_x,
            &mut self.atlas_alloc_y,
            &mut self.atlas_row_h,
            &metrics,
            &bitmap,
            self.cell_h_px,
        );
        self.glyph_cache.insert(ch, cached);
        self.font = Some(font);
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
        let tab_bar_h = if !snapshot.tab_labels.is_empty() { self.cell_h_px } else { 0.0 };
        let tab_bar_h_u32 = tab_bar_h.round() as u32;
        let available_h = self.size.height as f32 - tab_bar_h;

        let pad_h = snapshot.padding_h as f32;
        let pad_v = snapshot.padding_v as f32;

        // Bottom-align terminal content within the padded area: pad_v is reserved at
        // both the top and bottom of the terminal pane so the last visible row ends
        // pad_v pixels above the separator.
        let terminal_rows = snapshot.terminal_text.lines().count().max(1) as f32;
        let term_pane_h_px = snapshot.split_ratio * available_h;
        let effective_term_h = (term_pane_h_px - 2.0 * pad_v).max(0.0);
        let content_h_px = (terminal_rows * self.cell_h_px).min(effective_term_h);
        let term_top_offset_px = tab_bar_h + (effective_term_h - content_h_px).max(0.0);

        let panel_verts = build_panel_vertices(self.size, snapshot, term_top_offset_px, self.cell_w_px, self.cell_h_px, pad_h, pad_v);
        let panel_vertex_count = (panel_verts.len() / 6) as u32;
        let dropdown_bg_verts = build_suggestion_dropdown_bg_verts(self.size, snapshot, self.cell_w_px, self.cell_h_px, pad_h);
        let dropdown_bg_start = panel_vertex_count;
        let dropdown_bg_count = (dropdown_bg_verts.len() / 6) as u32;
        let overlay_bg_verts = build_settings_overlay_bg_verts(self.size, snapshot, self.cell_w_px, self.cell_h_px);
        let overlay_bg_start  = dropdown_bg_start + dropdown_bg_count;
        let overlay_bg_count  = (overlay_bg_verts.len() / 6) as u32;
        {
            // Upload main bg + dropdown bg + settings overlay bg, in draw order.
            let mut all_bg = panel_verts;
            all_bg.extend_from_slice(&dropdown_bg_verts);
            all_bg.extend_from_slice(&overlay_bg_verts);
            if !all_bg.is_empty() {
                let bytes = floats_as_bytes(&all_bg);
                let cap = VERTEX_BUF_CAPACITY as usize;
                self.queue.write_buffer(&self.vertex_buf, 0, &bytes[..bytes.len().min(cap)]);
            }
        }

        self.ensure_glyph('\u{276f}');
        self.ensure_glyph('\u{d7}'); // × close-button character
        for ch in snapshot.terminal_text.chars()
            .chain(snapshot.editor_text.chars())
            .chain(snapshot.editor_suggestion.chars())
        {
            if ch != '\n' && ch != '\r' && ch != '\t' && ch != ' ' {
                self.ensure_glyph(ch);
            }
        }
        if let Some(ref overlay_text) = snapshot.resize_overlay {
            for ch in overlay_text.chars() {
                if ch != ' ' { self.ensure_glyph(ch); }
            }
        }
        // Pre-cache all tab label characters plus × and + used in the tab bar.
        for label in &snapshot.tab_labels {
            for ch in label.chars() {
                if ch != ' ' { self.ensure_glyph(ch); }
            }
        }
        // Context menu item text characters (all ASCII, already cached by the ' '..='~'
        // loop in `new`, but ensure_glyph is idempotent so this is safe).
        if let Some(ref menu) = snapshot.tab_context_menu {
            let _ = menu; // characters are ASCII — already in cache
        }
        // Pre-cache suggestion dropdown characters.
        if let Some(ref dd) = snapshot.suggestion_dropdown {
            for item in &dd.items {
                for ch in item.chars() {
                    if ch != ' ' { self.ensure_glyph(ch); }
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

        add_text_verts(&snapshot.terminal_text, term_top_px + pad_v, pad_h, self.theme.text,
            &snapshot.terminal_fg_colors, &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size, &mut text_verts, 0);

        if let Some(ref overlay_text) = snapshot.resize_overlay {
            let n_chars = overlay_text.chars().count() as f32;
            let text_w_px = n_chars * self.cell_w_px;
            let term_h_px = tab_bar_h + snapshot.split_ratio * available_h;
            let x_start = (self.size.width as f32 - text_w_px) / 2.0;
            let y_start = (tab_bar_h + term_h_px) / 2.0 - self.cell_h_px / 2.0;
            add_text_verts(overlay_text, y_start, x_start, [1.0, 1.0, 1.0, 1.0],
                &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size, &mut text_verts, 0);
        }

        let terminal_vert_count = (text_verts.len() / 8) as u32;

        let editor_skip = snapshot.editor_scroll_offset;
        let prefix_color = [0.40, 0.80, 1.00, 1.0_f32];
        if editor_skip == 0 {
            add_text_verts("\u{276f} ", edit_top_px + pad_v, pad_h, prefix_color,
                &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size, &mut text_verts, 0);
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
        add_text_verts(&padded_editor, edit_top_px + pad_v, pad_h, self.theme.text,
            &padded_hl, &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size, &mut text_verts, editor_skip);

        // Tab label text — rendered inside the tab bar region at the very top.
        // The rightmost (2 × cell_w) pixels are reserved for the "+" button.
        let tab_text_vert_start = (text_verts.len() / 8) as u32;
        if !snapshot.tab_labels.is_empty() {
            let n = snapshot.tab_labels.len();
            let add_btn_w  = self.cell_w_px * 2.0;
            let tab_area_w = self.size.width as f32 - add_btn_w;
            let tab_w_px   = tab_area_w / n as f32;
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
                add_text_verts(label, 0.0, tab_x0 + self.cell_w_px * 0.4, text_color,
                    &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size, &mut text_verts, 0);
                // × close button at the right edge of the tab.
                let close_x = tab_x1 - self.cell_w_px * 1.3;
                let close_color = { let [r,g,b] = th.ansi_palette[9]; [r*0.80, g*0.65+0.20, b*0.65+0.20, 0.85_f32] };
                add_text_verts("\u{d7}", 0.0, close_x, close_color,
                    &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size, &mut text_verts, 0);
            }
            // "+" button text on the far right.
            let add_x = self.size.width as f32 - add_btn_w + self.cell_w_px * 0.5;
            add_text_verts("+", 0.0, add_x, { let [r,g,b] = th.ansi_palette[10]; [r, g, b, 0.95_f32] },
                &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size, &mut text_verts, 0);
        }

        // Pre-cache settings overlay characters.
        if let Some(ref overlay) = snapshot.settings_overlay {
            for item in &overlay.items {
                for ch in item.key.chars().chain(item.value.chars()) {
                    if ch != ' ' { self.ensure_glyph(ch); }
                }
            }
            if let Some(ref buf) = overlay.editing {
                for ch in buf.chars() {
                    if ch != ' ' { self.ensure_glyph(ch); }
                }
            }
            // Pre-cache search buffer and match list characters.
            if let Some(ref sbuf) = overlay.search_buf {
                for ch in sbuf.chars() {
                    if ch != ' ' { self.ensure_glyph(ch); }
                }
            }
            for m in &overlay.search_matches {
                for ch in m.chars() {
                    if ch != ' ' { self.ensure_glyph(ch); }
                }
            }
            // Fixed UI characters used in settings overlay rendering:
            // ← → (arrows), ↑ ↓ (footer nav), ▶ (dropdown marker), ▌ (cursor hint).
            for ch in ['\u{2190}', '\u{2192}', '\u{2191}', '\u{2193}', '\u{25b6}', '\u{258e}'] {
                self.ensure_glyph(ch);
            }
        }

        // Context menu item text — drawn with no scissor so it floats above everything.
        let context_text_vert_start = (text_verts.len() / 8) as u32;
        if let Some(ref menu) = snapshot.tab_context_menu {
            const ITEMS: &[&str] = &["New Tab", "Close Tab", "Move Left", "Move Right"];
            let menu_item_h = self.cell_h_px * 1.15;
            let menu_w = self.cell_w_px * 13.0;
            let menu_h = menu_item_h * ITEMS.len() as f32;
            let mx = menu.x_px.min(self.size.width  as f32 - menu_w).max(0.0);
            let my = menu.y_px.min(self.size.height as f32 - menu_h).max(0.0);
            for (i, &item) in ITEMS.iter().enumerate() {
                let text_color = if menu.hovered_item == Some(i) {
                    [1.0_f32, 1.0, 1.0, 1.0]
                } else {
                    [0.78_f32, 0.82, 0.87, 1.0]
                };
                // Vertically centre the text within each item row.
                let y_item = my + i as f32 * menu_item_h + (menu_item_h - self.cell_h_px) * 0.5;
                add_text_verts(item, y_item, mx + self.cell_w_px * 0.5, text_color,
                    &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size, &mut text_verts, 0);
            }
        }

        // Settings overlay text — rendered last, no scissor.
        let dropdown_text_vert_start = (text_verts.len() / 8) as u32;
        if let Some(ref dd) = snapshot.suggestion_dropdown {
            let th = &snapshot.theme;
            let n_visible  = dd.items.len().saturating_sub(dd.scroll_offset).min(8);
            let visible_end = dd.scroll_offset + n_visible;
            let visible_selected = dd.selected.saturating_sub(dd.scroll_offset);
            let row_h      = self.cell_h_px * 1.2;
            let panel_h    = n_visible as f32 * row_h;
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
                add_text_verts(item, row_y, pad_h + self.cell_w_px, color,
                    &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size,
                    &mut text_verts, 0);
            }
        }
        let settings_text_vert_start = (text_verts.len() / 8) as u32;
        if let Some(ref overlay) = snapshot.settings_overlay
            && self.size.width > 0 && self.size.height > 0 && self.cell_w_px > 0.0 && self.cell_h_px > 0.0 {
                let th = &snapshot.theme;
                let win_w = self.size.width as f32;
                let win_h = self.size.height as f32;
                let title_h  = self.cell_h_px * 2.2;
                let row_h    = self.cell_h_px * 1.7;
                let footer_h = self.cell_h_px * 1.9;
                let edit_h   = if overlay.editing.is_some() { self.cell_h_px * 1.8 } else { 0.0 };
                let n_items  = overlay.items.len() as f32;
                let panel_h  = title_h + n_items * row_h + edit_h + footer_h;
                let panel_w  = (self.cell_w_px * 72.0).min(win_w * 0.92).max(self.cell_w_px * 40.0);
                let panel_x0 = (win_w - panel_w) / 2.0;
                let panel_y0 = (win_h - panel_h) / 2.0;

                // Title
                let title_text = if overlay.just_saved { "  SETTINGS  \u{2713} Saved" } else { "  SETTINGS  (Cmd+,)" };
                let title_y = panel_y0 + (title_h - self.cell_h_px) / 2.0;
                add_text_verts(title_text, title_y, panel_x0 + self.cell_w_px,
                    th.text,
                    &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size,
                    &mut text_verts, 0);

                // Rows
                let key_col  = panel_x0 + self.cell_w_px * 1.5;
                let val_col  = panel_x0 + panel_w * 0.50;

                // Pre-compute the flat (non-header) item index of the focused row.
                // This lets us skip rendering text for rows that are physically covered
                // by the search dropdown, which otherwise bleeds through the opaque BG
                // (all text is accumulated in one draw call, so order matters).
                let pre_focused_flat = {
                    let mut ec = 0usize;
                    let mut fi = 0usize;
                    for (idx, itm) in overlay.items.iter().enumerate() {
                        if !itm.is_header {
                            if ec == overlay.cursor { fi = idx; break; }
                            ec += 1;
                        }
                    }
                    fi
                };
                // Flat indices [pre_focused_flat+1 .. pre_focused_flat+n_visible] are
                // covered by the dropdown and must not have their text rendered.
                const SEARCH_MAX_VISIBLE: usize = 8;
                let search_cover_end = if overlay.search_buf.is_some() {
                    let n_vis = overlay.search_matches.len()
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
                        if overlay.search_buf.is_some()
                            && i > pre_focused_flat
                            && i <= search_cover_end
                        {
                            continue;
                        }
                        add_text_verts(&item.key, row_y, key_col,
                            th.separator_focused,
                            &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size,
                            &mut text_verts, 0);
                    } else {
                        let is_focused = editable_idx == overlay.cursor;
                        if is_focused { focused_flat_idx = i; }
                        // Increment before the potential early-continue so the cursor
                        // mapping stays correct even for visually-skipped rows.
                        editable_idx += 1;
                        // Skip rows hidden under the search dropdown.
                        if overlay.search_buf.is_some()
                            && i > pre_focused_flat
                            && i <= search_cover_end
                        {
                            continue;
                        }
                        let (key_color, val_color) = if is_focused {
                            (th.text, th.cursor)
                        } else {
                            ({ let [r,g,b,_]=th.text; [r*0.85,g*0.85,b*0.85,1.0_f32] },
                             { let [r,g,b,_]=th.cursor; [r*0.75,g*0.85,b*0.85,0.85_f32] })
                        };
                        add_text_verts(&item.key, row_y, key_col, key_color,
                            &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size,
                            &mut text_verts, 0);

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
                        let arrows_buf: Option<String> =
                            if item.is_selectable && !item.is_searchable
                                && search_val_buf.is_none()
                                && !(is_focused && overlay.editing.is_some())
                            {
                                Some(format!("\u{2190} {} \u{2192}", item.value))
                            } else {
                                None
                            };
                        // Free-text fields get a dim cursor hint when focused and not yet editing.
                        let freetext_hint: Option<String> =
                            if !item.is_selectable && !item.is_searchable
                                && is_focused && overlay.editing.is_none()
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
                            if let Some(ref buf) = overlay.editing { buf.as_str() } else { &item.value }
                        } else {
                            &item.value
                        };
                        add_text_verts(display_val, row_y, val_col, val_color,
                            &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size,
                            &mut text_verts, 0);
                    }
                }

                // Footer help text — hidden when the search dropdown is open to avoid
                // it bleeding through the dropdown (footer y falls inside the dropdown area
                // when the focused item is in the lower half of the panel).
                if overlay.search_buf.is_none() {
                    let footer_y = panel_y0 + title_h + n_items * row_h + edit_h
                        + (footer_h - self.cell_h_px) / 2.0;
                    let footer_text = if overlay.editing.is_some() {
                        "  Enter: confirm   Esc: cancel"
                    } else {
                        "  \u{2191}\u{2193} navigate   \u{2190}\u{2192} change   Enter: edit/search   Esc: close & save"
                    };
                    add_text_verts(footer_text, footer_y, panel_x0,
                        { let [r,g,b,_]=th.text; [r*0.55,g*0.55,b*0.55,0.90_f32] },
                        &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size,
                        &mut text_verts, 0);
                }

                // Search dropdown text — rendered on top of the dropdown background.
                if overlay.search_buf.is_some() {
                    const SEARCH_MAX_VISIBLE: usize = 8;
                    let n_visible = overlay.search_matches.len()
                        .saturating_sub(overlay.search_scroll_offset)
                        .min(SEARCH_MAX_VISIBLE);
                    let visible_end = overlay.search_scroll_offset + n_visible;
                    let vis_sel = overlay.search_selected.saturating_sub(overlay.search_scroll_offset);
                    let drop_top_px = panel_y0 + title_h + (focused_flat_idx + 1) as f32 * row_h;
                    for (i, match_str) in overlay.search_matches[overlay.search_scroll_offset..visible_end].iter().enumerate() {
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
                        add_text_verts(&labeled, item_y, key_col, color,
                            &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size,
                            &mut text_verts, 0);
                    }
                    // "no results" hint when the query matched nothing.
                    if overlay.search_matches.is_empty() {
                        let item_y = drop_top_px + (row_h - self.cell_h_px) / 2.0;
                        add_text_verts("(no results)", item_y, key_col,
                            { let [r,g,b,_]=th.text; [r*0.45, g*0.45, b*0.45, 0.70] },
                            &[], &self.glyph_cache, self.cell_w_px, self.cell_h_px, self.size,
                            &mut text_verts, 0);
                    }
                }
        }

        let total_vert_count = (text_verts.len() / 8) as u32;
        if !text_verts.is_empty() {
            let bytes = floats_as_bytes(&text_verts);
            let cap = TEXT_VERTEX_BUF_CAPACITY as usize;
            self.queue.write_buffer(&self.text_vertex_buf, 0, &bytes[..bytes.len().min(cap)]);
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
                                + if snapshot.bell_active { 0.12 } else { 0.0 }).min(1.0),
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
                    let editor_pane_h = self.size.height.saturating_sub(split_y_px).saturating_sub(snapshot.padding_v);
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
                    pass.draw(context_text_vert_start.min(total_capped)..dropdown_text_vert_start.min(total_capped), 0..1);
                }

                // Suggestion dropdown background — drawn after main panel bg so it
                // sits on top of the terminal/editor backgrounds.
                if dropdown_bg_count > 0 {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.set_pipeline(&self.pipeline);
                    pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
                    pass.draw(dropdown_bg_start..dropdown_bg_start + dropdown_bg_count, 0..1);
                    // Restore text pipeline for dropdown text below.
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(0, &self.atlas_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.text_vertex_buf.slice(..));
                }

                // Suggestion dropdown text: no scissor.
                if total_capped > dropdown_text_vert_start {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.draw(dropdown_text_vert_start.min(total_capped)..settings_text_vert_start.min(total_capped), 0..1);
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
                if total_capped > settings_text_vert_start {
                    pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
                    pass.draw(settings_text_vert_start.min(total_capped)..total_capped, 0..1);
                }

                pass.set_scissor_rect(0, 0, self.size.width, self.size.height);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        Ok(())
    }
}

/// Returns a per-character syntax colour for a shell command string.
/// `None` means "use the renderer default colour".
/// Handles: keywords (purple), commands (cyan), flags (yellow),
/// quoted strings (amber), variable references (green), comments (dim gray).
fn highlight_shell(text: &str) -> Vec<Option<[f32; 3]>> {
    const KEYWORD: [f32; 3] = [0.78, 0.55, 0.96]; // soft purple
    const COMMAND: [f32; 3] = [0.40, 0.88, 1.00]; // cyan
    const FLAG:    [f32; 3] = [0.97, 0.90, 0.40]; // yellow
    const STRING:  [f32; 3] = [1.00, 0.72, 0.30]; // amber
    const COMMENT: [f32; 3] = [0.55, 0.57, 0.60]; // dim gray
    const VAR:     [f32; 3] = [0.56, 0.93, 0.56]; // soft green

    const SH_KEYWORDS: &[&str] = &[
        "if", "then", "else", "elif", "fi",
        "for", "while", "until", "do", "done",
        "case", "esac", "in",
        "function", "return", "break", "continue",
    ];

    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out: Vec<Option<[f32; 3]>> = vec![None; n];
    let mut i = 0usize;
    // true before the first real (non-whitespace, non-newline) word on each line
    let mut first_word = true;

    while i < n {
        let ch = chars[i];
        match ch {
            '\n' => {
                i += 1;
                first_word = true;
            }
            ' ' | '\t' => {
                i += 1;
            }
            '#' => {
                while i < n && chars[i] != '\n' {
                    out[i] = Some(COMMENT);
                    i += 1;
                }
            }
            '"' | '\'' => {
                let quote = ch;
                out[i] = Some(STRING);
                i += 1;
                while i < n {
                    if chars[i] == '\\' && quote == '"' && i + 1 < n {
                        out[i] = Some(STRING);
                        i += 1;
                        out[i] = Some(STRING);
                        i += 1;
                    } else if chars[i] == quote {
                        out[i] = Some(STRING);
                        i += 1;
                        break;
                    } else {
                        out[i] = Some(STRING);
                        i += 1;
                    }
                }
                first_word = false;
            }
            '$' => {
                let start = i;
                i += 1;
                if i < n && chars[i] == '{' {
                    i += 1;
                    while i < n && chars[i] != '}' {
                        i += 1;
                    }
                    if i < n { i += 1; } // consume '}'
                } else {
                    while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    // special vars: $@, $*, $#, $?, $!, $0-$9
                    if i == start + 1 && i < n && "@*#?!0123456789".contains(chars[i]) {
                        i += 1;
                    }
                }
                for item in out[start..i].iter_mut() { *item = Some(VAR); }
            }
            ';' => {
                i += 1;
                if i < n && chars[i] == ';' { i += 1; } // ;;
                first_word = true;
            }
            '|' | '&' => {
                out[i] = None;
                i += 1;
                if i < n && (chars[i] == '|' || chars[i] == '&') {
                    i += 1; // || or &&
                }
                first_word = true;
            }
            ch if ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '~' | '@' | ':' | '=') => {
                let word_start = i;
                while i < n {
                    let wch = chars[i];
                    if wch.is_whitespace()
                        || matches!(wch, '"' | '\'' | '$' | '#' | ';' | '|' | '&' | '(' | ')' | '<' | '>' | '`')
                    {
                        break;
                    }
                    i += 1;
                }
                let word: String = chars[word_start..i].iter().collect();
                let color = if word.starts_with('-') {
                    Some(FLAG)
                } else if SH_KEYWORDS.contains(&word.as_str()) {
                    Some(KEYWORD)
                } else if first_word {
                    Some(COMMAND)
                } else {
                    None
                };
                for item in out[word_start..i].iter_mut() { *item = color; }
                first_word = false;
            }
            _ => {
                i += 1;
            }
        }
    }

    out
}
