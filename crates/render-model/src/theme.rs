use crate::snapshot::RenderSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    Background,
    Text,
    Cursor,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Terminal,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneLayout {
    pub split_ratio: f32,
}

impl Default for PaneLayout {
    fn default() -> Self {
        Self { split_ratio: 0.7 }
    }
}

impl PaneLayout {
    pub fn terminal_bounds(&self) -> (f32, f32) {
        (1.0, 1.0 - 2.0 * self.split_ratio)
    }

    pub fn editor_bounds(&self) -> (f32, f32) {
        (1.0 - 2.0 * self.split_ratio, -1.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VsyncMode {
    On,
    Off,
    Adaptive,
}

#[derive(Debug, Clone)]
pub struct ColorTheme {
    pub terminal_bg: [f32; 4],
    pub editor_bg: [f32; 4],
    pub separator: [f32; 4],
    pub separator_focused: [f32; 4],
    pub cursor: [f32; 4],
    pub text: [f32; 4],
    /// ANSI 16-color palette override: indices 0-7 = normal, 8-15 = bright.
    /// Used by the terminal renderer instead of the built-in xterm table.
    pub ansi_palette: [[f32; 3]; 16],
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            terminal_bg: [0.05, 0.07, 0.09, 1.0],
            editor_bg: [0.09, 0.11, 0.14, 1.0],
            separator: [0.25, 0.27, 0.30, 1.0],
            separator_focused: [0.00, 0.75, 1.00, 1.0],
            cursor: [0.00, 0.85, 1.00, 0.90],
            text: [0.85, 0.87, 0.90, 1.0],
            ansi_palette: default_ansi_palette(),
        }
    }
}

/// The 16 standard ANSI/xterm colors matching the hardcoded table in
/// `terminal-screen`. Used when no theme file overrides the palette.
pub const fn default_ansi_palette() -> [[f32; 3]; 16] {
    [
        [0.000, 0.000, 0.000], // 0  black
        [0.502, 0.000, 0.000], // 1  red
        [0.000, 0.502, 0.000], // 2  green
        [0.502, 0.502, 0.000], // 3  yellow
        [0.000, 0.000, 0.502], // 4  blue
        [0.502, 0.000, 0.502], // 5  magenta
        [0.000, 0.502, 0.502], // 6  cyan
        [0.753, 0.753, 0.753], // 7  white
        [0.502, 0.502, 0.502], // 8  bright black
        [1.000, 0.333, 0.333], // 9  bright red
        [0.333, 1.000, 0.333], // 10 bright green
        [1.000, 1.000, 0.333], // 11 bright yellow
        [0.333, 0.333, 1.000], // 12 bright blue
        [1.000, 0.333, 1.000], // 13 bright magenta
        [0.333, 1.000, 1.000], // 14 bright cyan
        [1.000, 1.000, 1.000], // 15 bright white
    ]
}

#[derive(Debug, Clone)]
pub struct FontConfig {
    pub font_family: Option<String>,
    pub font_size: f32,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: 14.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub vsync: VsyncMode,
    pub target_fps: u32,
    pub glyph_atlas_size: (u32, u32),
    pub font: FontConfig,
    pub theme: ColorTheme,
    /// If set, the window opens at this logical-pixel size instead of the default 1280x720.
    pub initial_size: Option<(u32, u32)>,
    /// If `Some`, position the window at these physical-pixel screen coordinates on startup.
    pub initial_position: Option<(i32, i32)>,
    /// Background opacity (0.1–1.0). Values below 1.0 enable window transparency.
    /// Requires a compositing window manager on Linux/X11.
    pub opacity: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            vsync: VsyncMode::On,
            target_fps: 60,
            glyph_atlas_size: (2048, 2048),
            font: FontConfig::default(),
            theme: ColorTheme::default(),
            initial_size: None,
            initial_position: None,
            opacity: 1.0,
        }
    }
}

/// Scrollbar width in pixels
pub const SCROLLBAR_W_PX: f32 = 12.0;

/// Convert snapshot to IME area
pub fn snapshot_to_ime_area(_snapshot: &RenderSnapshot) -> Option<(f32, f32, f32, f32)> {
    None
}
