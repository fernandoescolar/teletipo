/// Terminal component: renders terminal pane content.
///
/// Renders:
/// - Background colors per cell
/// - Terminal text (line by line, with per-character colors)
/// - Selection/highlights (via separate components)
/// - Cursor (via separate component)
pub mod background;
pub mod text;

use crate::{RenderContext, Scene};

/// Marker struct for the terminal component.
pub struct Terminal;

impl Terminal {
    /// Emit terminal-related render commands (background and text).
    pub fn render(ctx: &RenderContext, scene: &mut Scene) {
        background::render_backgrounds(ctx, scene);
        text::render_text(ctx, scene);
    }
}
