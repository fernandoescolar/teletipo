use std::time::Duration;

use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::ModifiersState;

/// A detected link span in the terminal output (file path or URL).
#[derive(Debug, Clone)]
pub struct TerminalLink {
    /// Row index in the terminal grid (0-based).
    pub row: usize,
    /// First character column (inclusive).
    pub col_start: usize,
    /// Character column past the end (exclusive).
    pub col_end: usize,
    /// The raw string to pass to `open` / `xdg-open`.
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct RenderSnapshot {
    pub terminal_text: String,
    pub terminal_fg_colors: Vec<Option<[f32; 3]>>,
    pub terminal_bg_colors: Vec<Option<[f32; 3]>>,
    /// Style bits per terminal character: bit 0 = bold, bit 1 = italic, bit 2 = strikethrough.
    pub terminal_styles: Vec<u8>,
    pub editor_text: String,
    pub editor_cursor_offset: usize,
    pub scroll_offset: usize,
    pub scrollback_lines: usize,
    pub editor_focused: bool,
    pub split_ratio: f32,
    pub resize_overlay: Option<String>,
    pub editor_line_count: usize,
    pub editor_scroll_offset: usize,
    pub editor_selection: Option<(usize, usize)>,
    pub selection: Option<(usize, usize, usize, usize)>,
    /// Label for every open tab (e.g. "Tab 1", "Tab 2"). When empty the tab bar
    /// is not rendered. Populated by the application layer.
    pub tab_labels: Vec<String>,
    /// Index of the currently active (visible) tab.
    pub active_tab: usize,
    /// When `Some`, draw a floating context menu over the tab bar.
    pub tab_context_menu: Option<TabContextMenu>,
    /// Index of the tab currently being dragged (for visual feedback).
    pub tab_drag_from: Option<usize>,
    /// Insertion position for the drag indicator (0 = before first tab).
    pub tab_drag_insert_before: Option<usize>,
    /// Active color theme — applied every frame so live edits are reflected immediately.
    pub theme: ColorTheme,
    /// Horizontal text-grid padding in physical pixels.
    pub padding_h: u32,
    /// Vertical text-grid padding in physical pixels.
    pub padding_v: u32,
    /// When `Some`, display the in-app settings overlay.
    pub settings_overlay: Option<SettingsOverlay>,
    /// Active tab's working directory formatted for the window title (home dir
    /// replaced with `~`; never truncated).
    pub title_cwd: String,
    /// Ghost-text suffix shown after the cursor when a history entry starts
    /// with the current editor text. Empty when there is no suggestion or the
    /// cursor is not at the end of the input.
    pub editor_suggestion: String,
    /// When `Some`, render a suggestion dropdown panel above the editor line.
    /// Only populated while Tab/Shift+Tab cycling is active and there are at
    /// least two candidates.
    pub suggestion_dropdown: Option<SuggestionDropdown>,
    /// File paths and URLs detected in the terminal output.  Populated only
    /// when the Cmd key is held so the renderer can draw link underlines.
    pub terminal_links: Vec<TerminalLink>,
    /// When `true` the event loop should exit (e.g. last shell session ended).
    pub request_exit: bool,
    /// DECSCUSR cursor shape: 0/1/2 = block, 3/4 = underline, 5/6 = bar.
    pub cursor_shape: u16,
    /// When `true`, briefly tint the terminal background as a visual BEL indicator.
    pub bell_active: bool,
    /// When `false`, the cursor should be invisible this frame (blink-off half-cycle).
    pub cursor_blink_on: bool,
    /// Terminal cursor position (row, col), 0-based, in the visible grid.
    /// Used to draw the cursor block/underline/bar at the correct cell.
    pub terminal_cursor_row: usize,
    pub terminal_cursor_col: usize,
    /// Whether terminal fullscreen mode is active (alternate screen apps).
    pub terminal_fullscreen: bool,
}

/// Visual state for the suggestion cycling dropdown shown above the editor.
#[derive(Debug, Clone)]
pub struct SuggestionDropdown {
    /// All candidate entries in display (possibly truncated) form.
    pub items: Vec<String>,
    /// Absolute index (0..items.len()) of the currently highlighted item.
    pub selected: usize,
    /// Index of the first visible item.  The renderer shows
    /// `items[scroll_offset .. scroll_offset + MAX_VISIBLE]`.  Computed by
    /// the application layer so the selected item is always in the window.
    pub scroll_offset: usize,
}

/// State passed to the renderer to draw the tab context menu.
#[derive(Debug, Clone)]
pub struct TabContextMenu {
    /// Which tab was right-clicked.
    pub tab_idx: usize,
    /// Top-left corner of the menu (physical pixels from top-left of window).
    pub x_px: f32,
    pub y_px: f32,
    /// Currently hovered menu item (0=New Tab, 1=Close Tab, 2=Move Left, 3=Move Right).
    pub hovered_item: Option<usize>,
}

/// In-app settings overlay state passed to the renderer.
#[derive(Debug, Clone)]
pub struct SettingsOverlay {
    /// Flat ordered list of rows to display (headers + editable fields).
    pub items: Vec<SettingsItem>,
    /// Index of the currently highlighted *editable* field (among all items).
    pub cursor: usize,
    /// When `Some`, the selected field is in edit mode and this is the current buffer.
    pub editing: Option<String>,
    /// One-shot flag: show a brief "Saved" confirmation.
    pub just_saved: bool,
    /// When `Some`, the focused field is in search/filter mode (type-to-filter).
    pub search_buf: Option<String>,
    /// Filtered match list derived from `search_buf`.
    pub search_matches: Vec<String>,
    /// Index into `search_matches` of the currently highlighted result.
    pub search_selected: usize,
    /// First visible result index for scrolling the dropdown.
    pub search_scroll_offset: usize,
}

/// A single row in the settings overlay.
#[derive(Debug, Clone)]
pub struct SettingsItem {
    /// `true` → section header (e.g. "[theme]"); not selectable.
    pub is_header: bool,
    /// `true` → value is cycled with ← → rather than free-text edited.
    pub is_selectable: bool,
    /// `true` → pressing Enter activates type-to-filter search mode.
    pub is_searchable: bool,
    /// Left column text.
    pub key: String,
    /// Right column text (empty for headers).
    pub value: String,
}

#[derive(Debug, Clone)]
pub enum AppWindowEvent {
    CloseRequested,
    /// New top-left position of the window in physical pixels.
    WindowMoved { x: i32, y: i32 },
    /// Physical pixel dimensions of the window plus the actual cell size (physical px)
    /// as measured from the loaded font. Use `cell_w`/`cell_h` to compute col/row counts.
    Resized { width: u32, height: u32, scale_factor: f64, cell_w: f32, cell_h: f32 },
    CursorMoved { x: f64, y: f64 },
    MouseInput {
        state: ElementState,
        button: MouseButton,
    },
    MouseWheel { delta_lines: f32 },
    ModifiersChanged(ModifiersState),
    KeyboardInput(KeyEvent),
    ImeCommit(String),
}

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
    pub terminal_bg:       [f32; 4],
    pub editor_bg:         [f32; 4],
    pub separator:         [f32; 4],
    pub separator_focused: [f32; 4],
    pub cursor:            [f32; 4],
    pub text:              [f32; 4],
    /// ANSI 16-color palette override: indices 0-7 = normal, 8-15 = bright.
    /// Used by the terminal renderer instead of the built-in xterm table.
    pub ansi_palette:      [[f32; 3]; 16],
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            terminal_bg:       [0.05, 0.07, 0.09, 1.0],
            editor_bg:         [0.09, 0.11, 0.14, 1.0],
            separator:         [0.25, 0.27, 0.30, 1.0],
            separator_focused: [0.00, 0.75, 1.00, 1.0],
            cursor:            [0.00, 0.85, 1.00, 0.90],
            text:              [0.85, 0.87, 0.90, 1.0],
            ansi_palette:      default_ansi_palette(),
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
    /// If set, the window opens at this logical-pixel size instead of the default 1280×720.
    pub initial_size: Option<(u32, u32)>,
    /// If `Some`, position the window at these physical-pixel screen coordinates on startup.
    pub initial_position: Option<(i32, i32)>,
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
        }
    }
}

#[derive(Debug, Clone)]
pub struct DamageRegion {
    pub full_redraw: bool,
    pub dirty_rows: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct RenderStats {
    pub frame_count: u64,
    pub total_frame_time_us: u128,
    pub max_frame_time_us: u128,
}

impl RenderStats {
    pub fn record(&mut self, frame_time: Duration) {
        let micros = frame_time.as_micros();
        self.frame_count = self.frame_count.saturating_add(1);
        self.total_frame_time_us = self.total_frame_time_us.saturating_add(micros);
        self.max_frame_time_us = self.max_frame_time_us.max(micros);
    }

    pub fn avg_frame_time_us(&self) -> u128 {
        if self.frame_count == 0 {
            0
        } else {
            self.total_frame_time_us / self.frame_count as u128
        }
    }
}
