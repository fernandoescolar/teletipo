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
        BatchContainer {
            flat: FlatBatch::new(),
            glyph: GlyphBatch::new(),
            emoji: EmojiBatch::new(),
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
}

pub(crate) struct GlyphPipelineState {
    pub texture: glow::Texture,
    pub program: glow::Program,
    pub vbo: glow::Buffer,
    pub vao: glow::VertexArray,
    pub u_screen: Option<glow::UniformLocation>,
    pub u_sampler: Option<glow::UniformLocation>,
}

pub(crate) struct EmojiPipelineState {
    pub texture: glow::Texture,
    pub program: glow::Program,
    pub vbo: glow::Buffer,
    pub vao: glow::VertexArray,
    pub u_screen: Option<glow::UniformLocation>,
    pub u_sampler: Option<glow::UniformLocation>,
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
            },
            glyph: GlyphPipelineState {
                texture: glyph_texture,
                program: glyph_program,
                vbo: glyph_vbo,
                vao: glyph_vao,
                u_screen: glyph_u_screen,
                u_sampler: glyph_u_sampler,
            },
            emoji: EmojiPipelineState {
                texture: emoji_texture,
                program: emoji_program,
                vbo: emoji_vbo,
                vao: emoji_vao,
                u_screen: emoji_u_screen,
                u_sampler: emoji_u_sampler,
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
}
