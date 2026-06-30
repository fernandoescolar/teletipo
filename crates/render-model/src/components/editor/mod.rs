/// Editor component: renders editor pane content.
///
/// Staged implementation:
/// - Step 8a: Background and disabled state (moved to Scene)
/// - Step 8b: Text rendering (deferred - requires glyph atlas/shaping)
/// - Step 8c: Selection highlighting (deferred - complex geometry)
/// - Step 8d: Cursor (deferred - animation/style state)
/// - Step 8e: Suggestion dropdown (deferred - complex positioning)
pub mod background;

use crate::{RenderContext, Scene};

/// Marker struct for the editor component.
pub struct Editor;

impl Editor {
    /// Emit editor-related render commands.
    /// Currently handles background and disabled dimming only.
    /// Text, cursor, selection, and suggestions remain on the old path.
    pub fn render(ctx: &RenderContext, scene: &mut Scene) {
        background::render_background(ctx, scene);
    }
}
