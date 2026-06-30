use std::sync::Arc;

use crate::overlays::{
    CommandPalette, ContextMenu, KeybindingsOverlay, SettingsOverlay, StickyCommandOverlay,
    SuggestionDropdown, Toast,
};
use crate::screen::{DamageRegion, RenderRow};
use crate::theme::ColorTheme;

/// An image to be rendered on the terminal.
#[derive(Debug, Clone)]
pub struct SnapshotImage {
    /// Unique identifier for this image.
    pub id: u32,
    /// X position in pixels (top-left corner).
    pub x_px: usize,
    /// Y position in pixels (top-left corner).
    pub y_px: usize,
    /// Image width in pixels.
    pub width_px: usize,
    /// Image height in pixels.
    pub height_px: usize,
    /// RGBA pixel data (4 bytes per pixel).
    pub rgba: Arc<Vec<u8>>,
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
    /// Copy mode selection highlight ranges in viewport coordinates (row, col_start, col_end).
    pub copy_mode_highlights: Vec<(usize, usize, usize)>,
    /// Copy mode cursor position (row, col) in viewport coordinates, if copy mode is active.
    pub copy_mode_cursor: Option<(usize, usize)>,
    /// Images currently displayed on the terminal (in viewport pixel coordinates).
    pub terminal_images: Vec<SnapshotImage>,
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
    /// Fixed top overlay that anchors to the current command prompt.
    pub sticky_command_overlay: Option<StickyCommandOverlay>,
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
    /// Background opacity (0.1–1.0) configured by the user. Applied as the
    /// alpha component of the GL clear color so the compositor can show the
    /// desktop behind the terminal window.
    pub opacity: f32,
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
