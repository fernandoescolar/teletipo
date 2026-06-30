/// A collection of rendering commands organized by layer.
/// Neutral, backend-independent scene representation for rendering.
/// Components emit `RenderCommand`s instead of calling OpenGL directly.
///
/// Scene layers are rendered in this order:
/// 1. Background - pane backgrounds, separators, basic geometry
/// 2. Main - terminal text, editor text, tabs, scrollbars
/// 3. Floating - context menus, suggestion dropdowns
/// 4. Overlay - settings, keybindings, command palette, modal overlays
/// 5. Toast - transient notifications
/// 6. Debug - debugging overlays (unused for now)
#[derive(Debug, Clone)]
pub struct Scene {
    /// Layer 0: Pane backgrounds, separators, basic geometry
    pub background: Vec<RenderCommand>,
    /// Layer 1: Main content (terminal text, editor, tabs, scrollbars)
    pub main: Vec<RenderCommand>,
    /// Layer 2: Floating panels (context menus, suggestion dropdowns)
    pub floating: Vec<RenderCommand>,
    /// Layer 3: Overlays (settings, keybindings, command palette)
    pub overlay: Vec<RenderCommand>,
    /// Layer 4: Transient notifications (toasts)
    pub toast: Vec<RenderCommand>,
    /// Layer 5: Debugging overlays (currently unused)
    pub debug: Vec<RenderCommand>,
}

/// A single rendering command. Backend-independent.
#[derive(Debug, Clone)]
pub enum RenderCommand {
    Rect(RectCommand),
    Text(TextCommand),
    ClipPush(Rect),
    ClipPop,
}

/// Rectangular area for drawing or clipping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }
}

/// RGBA color as `[r, g, b, a]` in range [0.0, 1.0].
pub type Color = [f32; 4];

/// Solid rectangle command.
#[derive(Debug, Clone)]
pub struct RectCommand {
    pub rect: Rect,
    pub color: Color,
}

/// Text rendering command.
#[derive(Debug, Clone)]
pub struct TextCommand {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub color: Color,
    pub style: TextStyle,
    /// Per-character colors (optional). If provided, overrides `color` for each character.
    /// Length should match character count in `text`.
    pub char_colors: Option<Vec<Color>>,
    /// Per-character styles (optional). If provided, overrides `style` for each character.
    /// Length should match character count in `text`.
    pub char_styles: Option<Vec<TextStyle>>,
}

/// Text style flags.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub strike: bool,
}

/// Layer identifier for scene commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneLayer {
    /// Layer 0: Pane backgrounds, separators, basic geometry
    Background,
    /// Layer 1: Main content (terminal text, editor, tabs, scrollbars)
    Main,
    /// Layer 2: Floating panels (context menus, suggestion dropdowns)
    Floating,
    /// Layer 3: Overlays (settings, keybindings, command palette)
    Overlay,
    /// Layer 4: Transient notifications (toasts)
    Toast,
    /// Layer 5: Debugging overlays
    Debug,
}

impl Scene {
    /// Create a new empty scene with all layers initialized.
    pub fn new() -> Self {
        Scene {
            background: Vec::new(),
            main: Vec::new(),
            floating: Vec::new(),
            overlay: Vec::new(),
            toast: Vec::new(),
            debug: Vec::new(),
        }
    }

    /// Push a command to a specific layer.
    pub fn push_to_layer(&mut self, layer: SceneLayer, command: RenderCommand) {
        match layer {
            SceneLayer::Background => self.background.push(command),
            SceneLayer::Main => self.main.push(command),
            SceneLayer::Floating => self.floating.push(command),
            SceneLayer::Overlay => self.overlay.push(command),
            SceneLayer::Toast => self.toast.push(command),
            SceneLayer::Debug => self.debug.push(command),
        }
    }

    /// Push a command to the main layer (default for most content).
    pub fn push(&mut self, command: RenderCommand) {
        self.push_to_layer(SceneLayer::Main, command);
    }

    /// Emit a solid rectangle to a specific layer.
    pub fn rect_to_layer(
        &mut self,
        layer: SceneLayer,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    ) {
        self.push_to_layer(
            layer,
            RenderCommand::Rect(RectCommand {
                rect: Rect::new(x, y, w, h),
                color,
            }),
        );
    }

    /// Emit a solid rectangle to the main layer.
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.rect_to_layer(SceneLayer::Main, x, y, w, h, color);
    }

    /// Emit a text command with default style to a specific layer.
    pub fn text_to_layer(
        &mut self,
        layer: SceneLayer,
        x: f32,
        y: f32,
        text: impl Into<String>,
        color: Color,
    ) {
        self.text_styled_to_layer(layer, x, y, text, color, TextStyle::default());
    }

    /// Emit a text command with default style to the main layer.
    pub fn text(&mut self, x: f32, y: f32, text: impl Into<String>, color: Color) {
        self.text_to_layer(SceneLayer::Main, x, y, text, color);
    }

    /// Emit a text command with explicit style to a specific layer.
    pub fn text_styled_to_layer(
        &mut self,
        layer: SceneLayer,
        x: f32,
        y: f32,
        text: impl Into<String>,
        color: Color,
        style: TextStyle,
    ) {
        self.push_to_layer(
            layer,
            RenderCommand::Text(TextCommand {
                x,
                y,
                text: text.into(),
                color,
                style,
                char_colors: None,
                char_styles: None,
            }),
        );
    }

    /// Emit a text command with per-character colors to a specific layer.
    pub fn text_with_colors_to_layer(
        &mut self,
        layer: SceneLayer,
        x: f32,
        y: f32,
        text: impl Into<String>,
        char_colors: Vec<Color>,
        color: Color,
    ) {
        self.push_to_layer(
            layer,
            RenderCommand::Text(TextCommand {
                x,
                y,
                text: text.into(),
                color,
                style: TextStyle::default(),
                char_colors: Some(char_colors),
                char_styles: None,
            }),
        );
    }

    /// Emit a text command with per-character colors and styles to a specific layer.
    pub fn text_with_colors_and_styles_to_layer(
        &mut self,
        layer: SceneLayer,
        x: f32,
        y: f32,
        text: impl Into<String>,
        char_colors: Vec<Color>,
        char_styles: Vec<TextStyle>,
        color: Color,
    ) {
        self.push_to_layer(
            layer,
            RenderCommand::Text(TextCommand {
                x,
                y,
                text: text.into(),
                color,
                style: TextStyle::default(),
                char_colors: Some(char_colors),
                char_styles: Some(char_styles),
            }),
        );
    }

    /// Emit a text command with explicit style to the main layer.
    pub fn text_styled(
        &mut self,
        x: f32,
        y: f32,
        text: impl Into<String>,
        color: Color,
        style: TextStyle,
    ) {
        self.text_styled_to_layer(SceneLayer::Main, x, y, text, color, style);
    }

    /// Push a clipping rectangle to a specific layer.
    pub fn clip_push_to_layer(&mut self, layer: SceneLayer, rect: Rect) {
        self.push_to_layer(layer, RenderCommand::ClipPush(rect));
    }

    /// Push a clipping rectangle to the main layer.
    pub fn clip_push(&mut self, rect: Rect) {
        self.clip_push_to_layer(SceneLayer::Main, rect);
    }

    /// Pop the topmost clipping rectangle from a specific layer.
    pub fn clip_pop_to_layer(&mut self, layer: SceneLayer) {
        self.push_to_layer(layer, RenderCommand::ClipPop);
    }

    /// Pop the topmost clipping rectangle from the main layer.
    pub fn clip_pop(&mut self) {
        self.clip_pop_to_layer(SceneLayer::Main);
    }

    /// Clear all commands from all layers.
    pub fn clear(&mut self) {
        self.background.clear();
        self.main.clear();
        self.floating.clear();
        self.overlay.clear();
        self.toast.clear();
        self.debug.clear();
    }

    /// Check if the scene is empty (all layers empty).
    pub fn is_empty(&self) -> bool {
        self.background.is_empty()
            && self.main.is_empty()
            && self.floating.is_empty()
            && self.overlay.is_empty()
            && self.toast.is_empty()
            && self.debug.is_empty()
    }

    /// Return the total number of commands across all layers.
    pub fn len(&self) -> usize {
        self.background.len()
            + self.main.len()
            + self.floating.len()
            + self.overlay.len()
            + self.toast.len()
            + self.debug.len()
    }

    /// Iterate over all layers in render order.
    pub fn iter_layers(&self) -> impl Iterator<Item = (SceneLayer, &Vec<RenderCommand>)> {
        vec![
            (SceneLayer::Background, &self.background),
            (SceneLayer::Main, &self.main),
            (SceneLayer::Floating, &self.floating),
            (SceneLayer::Overlay, &self.overlay),
            (SceneLayer::Toast, &self.toast),
            (SceneLayer::Debug, &self.debug),
        ]
        .into_iter()
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scene_new() {
        let scene = Scene::new();
        assert!(scene.is_empty());
        assert_eq!(scene.len(), 0);
    }

    #[test]
    fn test_scene_rect() {
        let mut scene = Scene::new();
        let color = [1.0, 0.0, 0.0, 1.0];
        scene.rect(10.0, 20.0, 100.0, 50.0, color);

        assert_eq!(scene.len(), 1);
        assert!(!scene.is_empty());

        match &scene.main[0] {
            RenderCommand::Rect(cmd) => {
                assert_eq!(cmd.rect.x, 10.0);
                assert_eq!(cmd.rect.y, 20.0);
                assert_eq!(cmd.rect.w, 100.0);
                assert_eq!(cmd.rect.h, 50.0);
                assert_eq!(cmd.color, color);
            }
            _ => panic!("Expected Rect command"),
        }
    }

    #[test]
    fn test_scene_text() {
        let mut scene = Scene::new();
        let color = [0.0, 1.0, 0.0, 1.0];
        scene.text(5.0, 15.0, "Hello", color);

        assert_eq!(scene.len(), 1);

        match &scene.main[0] {
            RenderCommand::Text(cmd) => {
                assert_eq!(cmd.x, 5.0);
                assert_eq!(cmd.y, 15.0);
                assert_eq!(cmd.text, "Hello");
                assert_eq!(cmd.color, color);
                assert_eq!(cmd.style, TextStyle::default());
            }
            _ => panic!("Expected Text command"),
        }
    }

    #[test]
    fn test_scene_text_styled() {
        let mut scene = Scene::new();
        let color = [1.0, 1.0, 0.0, 1.0];
        let style = TextStyle {
            bold: true,
            italic: false,
            dim: false,
            strike: false,
        };

        scene.text_styled(0.0, 0.0, "Bold", color, style);

        assert_eq!(scene.len(), 1);

        match &scene.main[0] {
            RenderCommand::Text(cmd) => {
                assert!(cmd.style.bold);
                assert!(!cmd.style.italic);
            }
            _ => panic!("Expected Text command"),
        }
    }

    #[test]
    fn test_scene_clip_push_pop() {
        let mut scene = Scene::new();
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);

        scene.clip_push(rect);
        assert_eq!(scene.len(), 1);

        scene.clip_pop();
        assert_eq!(scene.len(), 2);

        match &scene.main[0] {
            RenderCommand::ClipPush(r) => {
                assert_eq!(r.x, 0.0);
                assert_eq!(r.w, 100.0);
            }
            _ => panic!("Expected ClipPush command"),
        }

        match &scene.main[1] {
            RenderCommand::ClipPop => {}
            _ => panic!("Expected ClipPop command"),
        }
    }

    #[test]
    fn test_scene_clear() {
        let mut scene = Scene::new();
        scene.rect(0.0, 0.0, 10.0, 10.0, [1.0; 4]);
        scene.text(0.0, 0.0, "test", [1.0; 4]);

        assert_eq!(scene.len(), 2);
        scene.clear();
        assert!(scene.is_empty());
        assert_eq!(scene.len(), 0);
    }

    #[test]
    fn test_scene_push() {
        let mut scene = Scene::new();
        let cmd = RenderCommand::ClipPop;
        scene.push(cmd);

        assert_eq!(scene.len(), 1);
        match &scene.main[0] {
            RenderCommand::ClipPop => {}
            _ => panic!("Expected ClipPop"),
        }
    }

    #[test]
    fn test_scene_layers() {
        let mut scene = Scene::new();
        scene.rect_to_layer(SceneLayer::Background, 0.0, 0.0, 10.0, 10.0, [1.0; 4]);
        scene.rect_to_layer(SceneLayer::Overlay, 20.0, 20.0, 10.0, 10.0, [0.0; 4]);
        scene.text(5.0, 5.0, "main", [1.0; 4]);

        assert_eq!(scene.len(), 3);
        assert_eq!(scene.background.len(), 1);
        assert_eq!(scene.main.len(), 1);
        assert_eq!(scene.overlay.len(), 1);
    }

    #[test]
    fn test_scene_layer_ordering() {
        let mut scene = Scene::new();
        // Add commands in reverse order
        scene.text_to_layer(SceneLayer::Toast, 0.0, 0.0, "toast", [1.0; 4]);
        scene.text_to_layer(SceneLayer::Overlay, 0.0, 0.0, "overlay", [1.0; 4]);
        scene.text_to_layer(SceneLayer::Main, 0.0, 0.0, "main", [1.0; 4]);
        scene.text_to_layer(SceneLayer::Background, 0.0, 0.0, "bg", [1.0; 4]);

        // Verify layers are in the correct order
        let mut layer_order = Vec::new();
        for (layer, commands) in scene.iter_layers() {
            if !commands.is_empty() {
                layer_order.push(layer);
            }
        }

        assert_eq!(
            layer_order,
            vec![
                SceneLayer::Background,
                SceneLayer::Main,
                SceneLayer::Overlay,
                SceneLayer::Toast,
            ]
        );
    }

    #[test]
    fn test_rect_new() {
        let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(rect.x, 10.0);
        assert_eq!(rect.y, 20.0);
        assert_eq!(rect.w, 100.0);
        assert_eq!(rect.h, 50.0);
    }

    #[test]
    fn test_text_style_default() {
        let style = TextStyle::default();
        assert!(!style.bold);
        assert!(!style.italic);
        assert!(!style.dim);
        assert!(!style.strike);
    }

    #[test]
    fn test_scene_default() {
        let scene = Scene::default();
        assert!(scene.is_empty());
    }
}
