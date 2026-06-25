use std::sync::Arc;
use std::time::Duration;

// `AppWindowEvent` is defined in `platform-abstraction` so that the UI crate
// can consume window events without depending on the GPU renderer.
pub use platform_abstraction::AppWindowEvent;

/// Severity level for a transient toast notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
}

/// A transient notification entry for display in the bottom-right corner.
#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
}
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
    /// Row-wise terminal data model used by the renderer. This is the primary
    /// representation for terminal glyphs and per-cell style/color state.
    pub terminal_rows: Vec<RenderRow>,
    /// Per-frame terminal damage map. Shared via `Arc` to avoid copying when
    /// multiple rendering stages consume the same damage metadata.
    pub terminal_damage: Arc<DamageRegion>,
    /// Flattened text form retained for transitional compatibility with
    /// call-sites that still expect newline-separated terminal text.
    pub terminal_text: String,
    /// Transitional flattened foreground colors parallel to `terminal_text`.
    pub terminal_fg_colors: Vec<Option<[f32; 3]>>,
    /// Transitional flattened background colors parallel to `terminal_text`.
    pub terminal_bg_colors: Vec<Option<[f32; 3]>>,
    /// Style bits per terminal character: bit 0 = bold, bit 1 = italic, bit 2 = strikethrough.
    pub terminal_styles: Vec<u8>,
    pub editor_text: String,
    /// Optional per-character editor foreground color, parallel to
    /// `editor_text.chars()` (including newlines).
    pub editor_fg_colors: Vec<Option<[f32; 3]>>,
    pub editor_cursor_offset: usize,
    pub scroll_offset: usize,
    pub scrollback_lines: usize,
    pub editor_focused: bool,
    /// `true` when a command is running and the user has not unlocked the
    /// editor with Ctrl+N.  The editor is shown dimmed and does not accept input.
    pub editor_disabled: bool,
    pub split_ratio: f32,
    pub resize_overlay: Option<String>,
    pub editor_line_count: usize,
    pub editor_scroll_offset: usize,
    /// Horizontal editor viewport offset in character cells.
    pub editor_horizontal_scroll_offset: usize,
    pub editor_selection: Option<(usize, usize)>,
    pub selection: Option<(usize, usize, usize, usize)>,
    /// Highlight ranges for all terminal search matches in viewport coordinates.
    pub search_highlights: Vec<(usize, usize, usize)>,
    /// Highlight range for the active terminal search match in viewport coordinates.
    pub search_current_highlight: Option<(usize, usize, usize)>,
    /// Label for every open tab (e.g. "Tab 1", "Tab 2"). When empty the tab bar
    /// is not rendered. Populated by the application layer.
    pub tab_labels: Vec<String>,
    /// Index of the currently active (visible) tab.
    pub active_tab: usize,
    /// When `Some`, draw a floating context menu over the UI.
    pub context_menu: Option<ContextMenu>,
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
    /// When `Some`, display the interactive keybindings editor overlay.
    pub keybindings_overlay: Option<KeybindingsOverlay>,
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
    /// Inline terminal search panel shown near the top-right of the terminal pane.
    pub search_panel: Option<SearchPanel>,
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
    /// Monotonic version counter from the terminal screen — incremented on
    /// every write.  Renderers can compare this against their last-rendered
    /// version to skip expensive terminal vertex uploads when content is
    /// unchanged.
    pub terminal_screen_version: u64,
    /// Transient toast notifications to display at the bottom-right corner.
    pub toast_stack: Vec<Toast>,
    /// When `Some`, the command palette (Cmd+Shift+P) overlay is open.
    pub command_palette: Option<CommandPalette>,
    /// Current logical font size in points (unscaled). When this changes the
    /// renderer should rebuild font metrics and reflow the terminal.
    pub font_size: f32,
}

/// Visual state for the inline terminal search panel.
#[derive(Debug, Clone)]
pub struct SearchPanel {
    /// Current query text.
    pub query: String,
    /// Total number of matches.
    pub match_count: usize,
    /// 1-based index of the current match.
    pub current_match: usize,
    /// When `true`, the search uses regex matching.
    pub regex_mode: bool,
    /// When `true`, the search is case-sensitive.
    pub case_sensitive: bool,
    /// Non-empty when the current regex query failed to compile.
    pub error: Option<String>,
    /// Character index (not byte) of the input cursor within `query`.
    pub cursor_char: usize,
    /// Active selection: `(start_char, end_char)` where `start < end`, or `None`.
    pub sel_char_range: Option<(usize, usize)>,
}

impl RenderSnapshot {
    pub fn terminal_rows_len(&self) -> usize {
        self.terminal_rows.len().max(1)
    }

    pub fn terminal_text_from_rows(&self) -> String {
        if self.terminal_rows.is_empty() {
            return self.terminal_text.clone();
        }
        let mut out = String::new();
        for (idx, row) in self.terminal_rows.iter().enumerate() {
            out.push_str(&row.text());
            if idx + 1 < self.terminal_rows.len() {
                out.push('\n');
            }
        }
        out
    }

    #[allow(clippy::type_complexity)] // three parallel buffers, not worth a named tuple type
    pub fn terminal_flatten_fg_bg_style(
        &self,
    ) -> (Vec<Option<[f32; 3]>>, Vec<Option<[f32; 3]>>, Vec<u8>) {
        let mut fg = Vec::new();
        let mut bg = Vec::new();
        let mut style = Vec::new();
        for (row_idx, row) in self.terminal_rows.iter().enumerate() {
            for cell in &row.cells {
                fg.push(cell.fg);
                bg.push(cell.bg);
                style.push(cell.style);
            }
            if row_idx + 1 < self.terminal_rows.len() {
                fg.push(None);
                bg.push(None);
                style.push(0);
            }
        }
        (fg, bg, style)
    }
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

/// Visual state for the command palette overlay (Cmd+Shift+P).
#[derive(Debug, Clone)]
pub struct CommandPalette {
    /// Current query text entered by the user.
    pub query: String,
    /// Character (not byte) index of the text cursor within `query`.
    pub cursor_char: usize,
    /// All filtered items to display. The renderer shows
    /// `items[scroll_offset .. scroll_offset + MAX_VISIBLE]`.
    pub items: Vec<String>,
    /// Absolute index (0..items.len()) of the currently selected item.
    pub selected: usize,
    /// Index of the first visible item.
    pub scroll_offset: usize,
    /// When `Some`, the palette is in sub-prompt mode: no item list is shown and
    /// this string is the label displayed above the text input.
    pub sub_prompt_label: Option<String>,
}

/// State passed to the renderer to draw a floating context menu.
#[derive(Debug, Clone)]
pub struct ContextMenu {
    /// Top-left corner of the menu (physical pixels from top-left of window).
    pub x_px: f32,
    pub y_px: f32,
    /// Menu items in draw order.
    pub items: Vec<String>,
    /// Whether each item is available for interaction.
    pub enabled_items: Vec<bool>,
    /// Currently hovered menu item index.
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
    /// `true` → section header (e.g. `[theme]`); not selectable.
    pub is_header: bool,
    /// `true` → value is cycled with ← → rather than free-text edited.
    pub is_selectable: bool,
    /// `true` → pressing Enter activates type-to-filter search mode.
    pub is_searchable: bool,
    /// `true` → pressing Enter executes a side-effecting action (no value editing).
    pub is_action: bool,
    /// Left column text.
    pub key: String,
    /// Right column text (empty for headers).
    pub value: String,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderCell {
    pub ch: char,
    pub fg: Option<[f32; 3]>,
    pub bg: Option<[f32; 3]>,
    pub style: u8,
}

impl Default for RenderCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: None,
            bg: None,
            style: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderRow {
    pub cells: Vec<RenderCell>,
    /// True when the row was touched in the source damage model for this frame.
    pub dirty: bool,
}

impl RenderRow {
    pub fn text(&self) -> String {
        self.cells.iter().map(|c| c.ch).collect()
    }
}

#[derive(Debug, Clone)]
pub struct DamageRegion {
    pub full_redraw: bool,
    pub dirty_rows: Vec<usize>,
    /// Number of columns in the terminal grid used to index `dirty_cells`.
    pub cols: usize,
    /// Cell-level damage bitset in row-major order. Length = rows * cols.
    pub dirty_cells: Vec<bool>,
}

impl DamageRegion {
    pub fn is_empty(&self) -> bool {
        !self.full_redraw && self.dirty_rows.is_empty() && !self.dirty_cells.iter().any(|v| *v)
    }

    pub fn row_is_dirty(&self, row: usize) -> bool {
        if self.full_redraw || self.dirty_rows.contains(&row) {
            return true;
        }
        if self.cols == 0 {
            return false;
        }
        let start = row.saturating_mul(self.cols);
        let end = start.saturating_add(self.cols).min(self.dirty_cells.len());
        self.dirty_cells[start..end].iter().any(|v| *v)
    }

    pub fn merge_from(&mut self, other: &DamageRegion) {
        if other.full_redraw {
            self.full_redraw = true;
        }
        self.cols = self.cols.max(other.cols);
        self.dirty_rows.extend(other.dirty_rows.iter().copied());
        if self.dirty_cells.len() < other.dirty_cells.len() {
            self.dirty_cells.resize(other.dirty_cells.len(), false);
        }
        for (idx, dirty) in other.dirty_cells.iter().copied().enumerate() {
            if dirty {
                self.dirty_cells[idx] = true;
            }
        }
    }

    pub fn clear(&mut self) {
        self.full_redraw = false;
        self.dirty_rows.clear();
        for slot in &mut self.dirty_cells {
            *slot = false;
        }
    }
}

impl Default for DamageRegion {
    fn default() -> Self {
        Self {
            full_redraw: true,
            dirty_rows: Vec::new(),
            cols: 0,
            dirty_cells: Vec::new(),
        }
    }
}

/// A single row in the keybindings overlay.
#[derive(Debug, Clone)]
pub struct KeybindingRow {
    /// Snake_case action identifier (e.g. `"new_tab"`).
    pub action_id: String,
    /// Human-readable label (e.g. `"New Tab"`).
    pub label: String,
    /// Formatted key combo string (e.g. `"Cmd+T"`), or `None` when truly unbound.
    pub binding: Option<String>,
    /// `true` when `binding` comes from the built-in default (not user-configured).
    pub is_default: bool,
}

/// Overlay for the interactive keybindings editor.
#[derive(Debug, Clone)]
pub struct KeybindingsOverlay {
    /// All bindable actions, one per row.
    pub rows: Vec<KeybindingRow>,
    /// Index of the currently highlighted row.
    pub cursor: usize,
    /// First visible row (for scrolling).
    pub scroll_offset: usize,
    /// When `true`, the highlighted row is in "recording" mode — waiting for a key combo.
    pub recording: bool,
    /// One-shot flag: flash a "Saved" confirmation.
    pub just_saved: bool,
    /// How many rows fit on screen at once (set by the keybindings_ui layer).
    pub visible_rows: usize,
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

/// Scrollbar width in pixels
pub const SCROLLBAR_W_PX: f32 = 12.0;

/// Convert snapshot to IME area
pub fn snapshot_to_ime_area(_snapshot: &RenderSnapshot) -> Option<(f32, f32, f32, f32)> {
    None
}
