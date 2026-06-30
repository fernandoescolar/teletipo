/// GPU pipelines: shader programs, VAOs, VBOs, and uniform state.
///
/// Organizes flat-color, glyph-atlas, and color-emoji rendering pipelines.

use glow::HasContext;

/// Flat-color rendering pipeline (backgrounds, borders, overlays).
/// Format: [x, y, r, g, b, a] × 6 per quad
pub struct FlatPipeline {
    pub program: glow::Program,
    pub vbo: glow::Buffer,
    pub vao: glow::VertexArray,
    pub u_screen: Option<glow::UniformLocation>,
}

impl FlatPipeline {
    /// Create a new flat-color pipeline.
    pub fn new(
        gl: &glow::Context,
        program: glow::Program,
        vbo: glow::Buffer,
        vao: glow::VertexArray,
        u_screen: Option<glow::UniformLocation>,
    ) -> Self {
        FlatPipeline {
            program,
            vbo,
            vao,
            u_screen,
        }
    }

    /// Set the screen size uniform (projection matrix basis).
    pub fn set_screen_size(&self, gl: &glow::Context, width: f32, height: f32) {
        if let Some(loc) = &self.u_screen {
            unsafe {
                gl.uniform_2_f32(Some(loc), width, height);
            }
        }
    }
}

/// Glyph atlas rendering pipeline (monochrome text).
/// Format: [x, y, u, v, r, g, b, a] × 6 per quad
pub struct GlyphPipeline {
    pub program: glow::Program,
    pub vbo: glow::Buffer,
    pub vao: glow::VertexArray,
    pub u_screen: Option<glow::UniformLocation>,
    pub u_sampler: Option<glow::UniformLocation>,
    pub atlas_texture: glow::Texture,
}

impl GlyphPipeline {
    /// Create a new glyph pipeline.
    pub fn new(
        gl: &glow::Context,
        program: glow::Program,
        vbo: glow::Buffer,
        vao: glow::VertexArray,
        u_screen: Option<glow::UniformLocation>,
        u_sampler: Option<glow::UniformLocation>,
        atlas_texture: glow::Texture,
    ) -> Self {
        GlyphPipeline {
            program,
            vbo,
            vao,
            u_screen,
            u_sampler,
            atlas_texture,
        }
    }

    /// Set the screen size uniform.
    pub fn set_screen_size(&self, gl: &glow::Context, width: f32, height: f32) {
        if let Some(loc) = &self.u_screen {
            unsafe {
                gl.uniform_2_f32(Some(loc), width, height);
            }
        }
    }

    /// Bind the atlas texture and sampler uniform.
    pub fn bind_atlas(&self, gl: &glow::Context, texture_unit: u32) {
        unsafe {
            gl.active_texture(glow::TEXTURE0 + texture_unit);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas_texture));
            if let Some(loc) = self.u_sampler.as_ref() {
                gl.uniform_1_i32(Some(loc), texture_unit as i32);
            }
        }
    }
}

/// Color emoji rendering pipeline (RGBA emoji bitmaps).
/// Format: [x, y, u, v, r, g, b, a] × 6 per quad
pub struct EmojiPipeline {
    pub program: glow::Program,
    pub vbo: glow::Buffer,
    pub vao: glow::VertexArray,
    pub u_screen: Option<glow::UniformLocation>,
    pub u_sampler: Option<glow::UniformLocation>,
    pub color_atlas_texture: glow::Texture,
}

impl EmojiPipeline {
    /// Create a new emoji pipeline.
    pub fn new(
        gl: &glow::Context,
        program: glow::Program,
        vbo: glow::Buffer,
        vao: glow::VertexArray,
        u_screen: Option<glow::UniformLocation>,
        u_sampler: Option<glow::UniformLocation>,
        color_atlas_texture: glow::Texture,
    ) -> Self {
        EmojiPipeline {
            program,
            vbo,
            vao,
            u_screen,
            u_sampler,
            color_atlas_texture,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flat_pipeline_creation() {
        // Note: Can't actually create GL objects in tests without context.
        // This is a placeholder for documentation.
        // Real tests would require a GL context.
    }
}
