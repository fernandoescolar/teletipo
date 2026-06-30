/// Terminal component: renders terminal pane content.
///
/// Renders:
/// - Background colors per cell
/// - Selection/highlights (via separate components)
/// - Cursor (via separate component)
///
/// Note: Terminal text rendering uses font shaping (rustybuzz) and is handled
/// directly in painter.rs for proper ligature and complex script support.
pub mod background;

use crate::{RenderContext, Scene};

/// Marker struct for the terminal component.
pub struct Terminal;

impl Terminal {
    /// Emit terminal-related render commands (background only).
    /// Text is rendered by painter.rs using font shaping.
    pub fn render(ctx: &RenderContext, scene: &mut Scene) {
        background::render_backgrounds(ctx, scene);
    }
}
