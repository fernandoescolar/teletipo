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

/// Sticky command overlay shown when a single command block spans beyond view.
#[derive(Debug, Clone)]
pub struct StickyCommandOverlay {
    /// Truncated one-line command text to render.
    pub text: String,
    /// Absolute prompt row to jump to when the overlay is clicked.
    pub prompt_row: usize,
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
    /// `true` -> section header (e.g. `[theme]`); not selectable.
    pub is_header: bool,
    /// `true` -> value is cycled with <- -> rather than free-text edited.
    pub is_selectable: bool,
    /// `true` -> pressing Enter activates type-to-filter search mode.
    pub is_searchable: bool,
    /// `true` -> pressing Enter executes a side-effecting action (no value editing).
    pub is_action: bool,
    /// Left column text.
    pub key: String,
    /// Right column text (empty for headers).
    pub value: String,
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
    /// When `true`, the highlighted row is in "recording" mode - waiting for a key combo.
    pub recording: bool,
    /// One-shot flag: flash a "Saved" confirmation.
    pub just_saved: bool,
    /// How many rows fit on screen at once (set by the keybindings_ui layer).
    pub visible_rows: usize,
}
