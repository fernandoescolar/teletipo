#![allow(dead_code)]

/// Batch for flat-color geometry.
/// Format: [x, y, r, g, b, a] × 2 per quad (6 vertices per quad)
///
/// Vertex batching: organizing commands into efficient GPU uploads.
///
/// Manages vertex buffers for different pipeline types:
/// - Flat color geometry (backgrounds, borders, solids)
/// - Glyph quads with atlas UV coordinates
/// - Emoji quads with color atlas UV coordinates
pub struct FlatBatch {
    pub vertices: Vec<f32>,
}

impl FlatBatch {
    /// Create a new empty batch.
    pub fn new() -> Self {
        FlatBatch {
            vertices: Vec::new(),
        }
    }

    /// Clear all vertices.
    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    /// Push a colored quad (2 triangles = 6 vertices).
    #[allow(clippy::too_many_arguments)]
    pub fn push_quad(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) {
        // Triangle 1: (x0,y0) - (x1,y0) - (x0,y1)
        self.vertices.extend_from_slice(&[x0, y0, r, g, b, a]);
        self.vertices.extend_from_slice(&[x1, y0, r, g, b, a]);
        self.vertices.extend_from_slice(&[x0, y1, r, g, b, a]);

        // Triangle 2: (x1,y0) - (x1,y1) - (x0,y1)
        self.vertices.extend_from_slice(&[x1, y0, r, g, b, a]);
        self.vertices.extend_from_slice(&[x1, y1, r, g, b, a]);
        self.vertices.extend_from_slice(&[x0, y1, r, g, b, a]);
    }

    /// Push a single color value (used for testing).
    pub fn push_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.vertices.extend_from_slice(&[r, g, b, a]);
    }

    /// Get the number of quads in the batch.
    pub fn quad_count(&self) -> usize {
        self.vertices.len() / 36 // 6 vertices × 6 components per quad
    }

    /// Get total vertex count.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len() / 6 // 6 components per vertex
    }

    /// Check if batch is empty.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

impl Default for FlatBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch for glyph atlas quads.
/// Format: [x, y, u, v, r, g, b, a] × 2 per quad
pub struct GlyphBatch {
    pub vertices: Vec<f32>,
}

impl GlyphBatch {
    /// Create a new empty batch.
    pub fn new() -> Self {
        GlyphBatch {
            vertices: Vec::new(),
        }
    }

    /// Clear all vertices.
    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    /// Push a textured quad with UV coordinates.
    #[allow(clippy::too_many_arguments)]
    pub fn push_quad(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) {
        // Triangle 1
        self.vertices
            .extend_from_slice(&[x0, y0, u0, v0, r, g, b, a]);
        self.vertices
            .extend_from_slice(&[x1, y0, u1, v0, r, g, b, a]);
        self.vertices
            .extend_from_slice(&[x0, y1, u0, v1, r, g, b, a]);

        // Triangle 2
        self.vertices
            .extend_from_slice(&[x1, y0, u1, v0, r, g, b, a]);
        self.vertices
            .extend_from_slice(&[x1, y1, u1, v1, r, g, b, a]);
        self.vertices
            .extend_from_slice(&[x0, y1, u0, v1, r, g, b, a]);
    }

    /// Get the number of quads in the batch.
    pub fn quad_count(&self) -> usize {
        if self.vertices.is_empty() {
            0
        } else {
            self.vertices.len() / 48 // 6 vertices × 8 components per quad
        }
    }

    /// Get total vertex count.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len() / 8 // 8 components per vertex
    }

    /// Check if batch is empty.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

impl Default for GlyphBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch for color emoji atlas quads (same format as glyph batch).
pub struct EmojiBatch {
    pub vertices: Vec<f32>,
}

impl EmojiBatch {
    /// Create a new empty batch.
    pub fn new() -> Self {
        EmojiBatch {
            vertices: Vec::new(),
        }
    }

    /// Clear all vertices.
    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    /// Push a textured emoji quad with UV coordinates.
    #[allow(clippy::too_many_arguments)]
    pub fn push_quad(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        u0: f32,
        v0: f32,
        u1: f32,
        v1: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) {
        // Triangle 1
        self.vertices
            .extend_from_slice(&[x0, y0, u0, v0, r, g, b, a]);
        self.vertices
            .extend_from_slice(&[x1, y0, u1, v0, r, g, b, a]);
        self.vertices
            .extend_from_slice(&[x0, y1, u0, v1, r, g, b, a]);

        // Triangle 2
        self.vertices
            .extend_from_slice(&[x1, y0, u1, v0, r, g, b, a]);
        self.vertices
            .extend_from_slice(&[x1, y1, u1, v1, r, g, b, a]);
        self.vertices
            .extend_from_slice(&[x0, y1, u0, v1, r, g, b, a]);
    }

    /// Get the number of quads in the batch.
    pub fn quad_count(&self) -> usize {
        if self.vertices.is_empty() {
            0
        } else {
            self.vertices.len() / 48 // 6 vertices × 8 components per quad
        }
    }

    /// Get total vertex count.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len() / 8 // 8 components per vertex
    }

    /// Check if batch is empty.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

impl Default for EmojiBatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flat_batch_new() {
        let batch = FlatBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.quad_count(), 0);
    }

    #[test]
    fn test_flat_batch_push_quad() {
        let mut batch = FlatBatch::new();
        batch.push_quad(0.0, 0.0, 10.0, 10.0, 1.0, 0.0, 0.0, 1.0);

        assert_eq!(batch.quad_count(), 1);
        assert_eq!(batch.vertex_count(), 6);
    }

    #[test]
    fn test_flat_batch_multiple_quads() {
        let mut batch = FlatBatch::new();
        batch.push_quad(0.0, 0.0, 10.0, 10.0, 1.0, 0.0, 0.0, 1.0);
        batch.push_quad(20.0, 20.0, 30.0, 30.0, 0.0, 1.0, 0.0, 1.0);

        assert_eq!(batch.quad_count(), 2);
        assert_eq!(batch.vertex_count(), 12);
    }

    #[test]
    fn test_flat_batch_clear() {
        let mut batch = FlatBatch::new();
        batch.push_quad(0.0, 0.0, 10.0, 10.0, 1.0, 0.0, 0.0, 1.0);
        assert_eq!(batch.quad_count(), 1);

        batch.clear();
        assert!(batch.is_empty());
        assert_eq!(batch.quad_count(), 0);
    }

    #[test]
    fn test_glyph_batch_new() {
        let batch = GlyphBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.quad_count(), 0);
    }

    #[test]
    fn test_glyph_batch_push_quad() {
        let mut batch = GlyphBatch::new();
        batch.push_quad(0.0, 0.0, 10.0, 10.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0);

        assert_eq!(batch.quad_count(), 1);
        assert_eq!(batch.vertex_count(), 6);
    }

    #[test]
    fn test_emoji_batch_new() {
        let batch = EmojiBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.quad_count(), 0);
    }

    #[test]
    fn test_emoji_batch_push_quad() {
        let mut batch = EmojiBatch::new();
        batch.push_quad(0.0, 0.0, 20.0, 20.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0);

        assert_eq!(batch.quad_count(), 1);
        assert_eq!(batch.vertex_count(), 6);
    }
}
