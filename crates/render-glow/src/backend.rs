#![allow(dead_code)]

/// Backend module: GPU rendering infrastructure.
///
/// Organizes OpenGL state, pipelines, batches, and atlas structures
/// into logical units for gradual refactoring toward GlowBackend.
pub use crate::batch::{EmojiBatch, FlatBatch, GlyphBatch};

use glow::HasContext;

/// Vertex buffer batches for each rendering pipeline.
pub(crate) struct BatchContainer {
    pub flat: FlatBatch,
    pub glyph: GlyphBatch,
    pub emoji: EmojiBatch,
}

impl BatchContainer {
    /// Create a new batch container with empty batches.
    pub(crate) fn new() -> Self {
        // Conservative startup capacities to reduce first-frame realloc churn.
        BatchContainer {
            flat: FlatBatch::with_capacity(2048),
            glyph: GlyphBatch::with_capacity(4096),
            emoji: EmojiBatch::with_capacity(512),
        }
    }

    /// Clear all batches (called at start of each frame).
    #[allow(dead_code)]
    pub(crate) fn clear_all(&mut self) {
        self.flat.clear();
        self.glyph.clear();
        self.emoji.clear();
    }
}

/// GPU pipeline state: shader programs, VAOs, VBOs, and uniforms.
/// Groups the raw OpenGL objects for each rendering pipeline.
pub(crate) struct GpuState {
    /// Flat-color rendering (backgrounds, borders, overlays).
    pub flat: FlatPipelineState,
    /// Monochrome glyph atlas rendering.
    pub glyph: GlyphPipelineState,
    /// Color emoji atlas rendering.
    pub emoji: EmojiPipelineState,
}

pub(crate) struct FlatPipelineState {
    pub program: glow::Program,
    pub vbo: glow::Buffer,
    pub vao: glow::VertexArray,
    pub u_screen: Option<glow::UniformLocation>,
    /// Current VBO capacity in bytes for smart buffer updates.
    pub vbo_capacity_bytes: usize,
}

pub(crate) struct GlyphPipelineState {
    pub texture: glow::Texture,
    pub program: glow::Program,
    pub vbo: glow::Buffer,
    pub vao: glow::VertexArray,
    pub u_screen: Option<glow::UniformLocation>,
    pub u_sampler: Option<glow::UniformLocation>,
    /// Current VBO capacity in bytes for smart buffer updates.
    pub vbo_capacity_bytes: usize,
}

pub(crate) struct EmojiPipelineState {
    pub texture: glow::Texture,
    pub program: glow::Program,
    pub vbo: glow::Buffer,
    pub vao: glow::VertexArray,
    pub u_screen: Option<glow::UniformLocation>,
    pub u_sampler: Option<glow::UniformLocation>,
    /// Current VBO capacity in bytes for smart buffer updates.
    pub vbo_capacity_bytes: usize,
}

impl GpuState {
    /// Create GPU state from individual pipeline components.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        flat_program: glow::Program,
        flat_vbo: glow::Buffer,
        flat_vao: glow::VertexArray,
        flat_u_screen: Option<glow::UniformLocation>,
        glyph_texture: glow::Texture,
        glyph_program: glow::Program,
        glyph_vbo: glow::Buffer,
        glyph_vao: glow::VertexArray,
        glyph_u_screen: Option<glow::UniformLocation>,
        glyph_u_sampler: Option<glow::UniformLocation>,
        emoji_texture: glow::Texture,
        emoji_program: glow::Program,
        emoji_vbo: glow::Buffer,
        emoji_vao: glow::VertexArray,
        emoji_u_screen: Option<glow::UniformLocation>,
        emoji_u_sampler: Option<glow::UniformLocation>,
    ) -> Self {
        GpuState {
            flat: FlatPipelineState {
                program: flat_program,
                vbo: flat_vbo,
                vao: flat_vao,
                u_screen: flat_u_screen,
                vbo_capacity_bytes: 0,
            },
            glyph: GlyphPipelineState {
                texture: glyph_texture,
                program: glyph_program,
                vbo: glyph_vbo,
                vao: glyph_vao,
                u_screen: glyph_u_screen,
                u_sampler: glyph_u_sampler,
                vbo_capacity_bytes: 0,
            },
            emoji: EmojiPipelineState {
                texture: emoji_texture,
                program: emoji_program,
                vbo: emoji_vbo,
                vao: emoji_vao,
                u_screen: emoji_u_screen,
                u_sampler: emoji_u_sampler,
                vbo_capacity_bytes: 0,
            },
        }
    }

    /// Bind the flat-color pipeline state.
    pub(crate) fn bind_flat(&self, gl: &glow::Context) {
        unsafe {
            gl.use_program(Some(self.flat.program));
            gl.bind_vertex_array(Some(self.flat.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.flat.vbo));
        }
    }

    /// Bind the glyph-atlas pipeline state.
    pub(crate) fn bind_glyph(&self, gl: &glow::Context) {
        unsafe {
            gl.use_program(Some(self.glyph.program));
            gl.bind_vertex_array(Some(self.glyph.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.glyph.vbo));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.glyph.texture));
        }
    }

    /// Bind the emoji-atlas pipeline state.
    pub(crate) fn bind_emoji(&self, gl: &glow::Context) {
        unsafe {
            gl.use_program(Some(self.emoji.program));
            gl.bind_vertex_array(Some(self.emoji.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.emoji.vbo));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.emoji.texture));
        }
    }

    /// Upload data to a pipeline's VBO, using buffer_sub_data if capacity allows,
    /// otherwise allocating with buffer_data_u8_slice.
    /// Returns the number of bytes uploaded.
    pub(crate) fn upload_flat_vbo(&mut self, gl: &glow::Context, data: &[u8]) -> usize {
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.flat.vbo));
            if data.len() > self.flat.vbo_capacity_bytes {
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, data, glow::STREAM_DRAW);
                self.flat.vbo_capacity_bytes = data.len();
            } else {
                gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, data);
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }
        data.len()
    }

    /// Upload data to the glyph pipeline's VBO.
    pub(crate) fn upload_glyph_vbo(&mut self, gl: &glow::Context, data: &[u8]) -> usize {
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.glyph.vbo));
            if data.len() > self.glyph.vbo_capacity_bytes {
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, data, glow::STREAM_DRAW);
                self.glyph.vbo_capacity_bytes = data.len();
            } else {
                gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, data);
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }
        data.len()
    }

    /// Upload data to the emoji pipeline's VBO.
    pub(crate) fn upload_emoji_vbo(&mut self, gl: &glow::Context, data: &[u8]) -> usize {
        unsafe {
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.emoji.vbo));
            if data.len() > self.emoji.vbo_capacity_bytes {
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, data, glow::STREAM_DRAW);
                self.emoji.vbo_capacity_bytes = data.len();
            } else {
                gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, data);
            }
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
        }
        data.len()
    }
}
