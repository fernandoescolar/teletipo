/// Editor component: renders editor pane content.
///
/// Renders:
/// - Background and disabled state
/// - Editor text
/// - Selection/highlights (via separate component)
/// - Cursor (via separate component)
pub mod background;
pub mod text;

use crate::{RenderContext, Scene};

/// Marker struct for the editor component.
pub struct Editor;

impl Editor {
    /// Emit editor-related render commands (background and text).
    pub fn render(ctx: &RenderContext, scene: &mut Scene) {
        background::render_background(ctx, scene);
        text::render_text(ctx, scene);
    }
}
