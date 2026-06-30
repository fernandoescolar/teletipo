/// Terminal component: renders terminal pane content.
///
/// Staged implementation:
/// - Step 7a: Background colors per cell (moved to Scene)
/// - Step 7b: Text rendering (deferred - requires glyph atlas/shaping)
/// - Step 7c: Selection/highlights (deferred - complex geometry)
/// - Step 7d: Cursor (deferred - animation state)
pub mod background;

use crate::{RenderContext, Scene};

/// Marker struct for the terminal component.
pub struct Terminal;

impl Terminal {
    /// Emit terminal-related render commands.
    /// Currently handles background color cells only.
    /// Text, cursor, and selection rendering remain on the old path.
    pub fn render(ctx: &RenderContext, scene: &mut Scene) {
        background::render_backgrounds(ctx, scene);
    }
}
