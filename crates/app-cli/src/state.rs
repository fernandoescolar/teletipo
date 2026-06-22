use crate::launch::FontEntry;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Currently held keyboard modifier keys, refreshed on every
/// [`winit::event::WindowEvent::ModifiersChanged`].
#[derive(Default)]
pub(crate) struct ModifierState {
    pub(crate) ctrl_down: bool,
    /// Whether the Super/Command key (⌘ on macOS) is currently held.
    pub(crate) super_down: bool,
    pub(crate) shift_down: bool,
    /// Whether the Alt/Option key is currently held.
    pub(crate) alt_down: bool,
}

/// State for the various pointer-driven drag interactions (separator,
/// scrollbars, tab reorder). All fields default to a neutral "no drag in
/// progress" state.
#[derive(Default)]
pub(crate) struct DragState {
    /// Whether the user is currently dragging the separator bar.
    pub(crate) dragging_separator: bool,
    /// Whether the user is currently dragging the terminal scrollbar thumb.
    pub(crate) dragging_terminal_scrollbar: bool,
    /// Whether the user is currently dragging the editor scrollbar thumb.
    pub(crate) dragging_editor_scrollbar: bool,
    /// Whether the user is currently dragging the editor horizontal scrollbar thumb.
    pub(crate) dragging_editor_horizontal_scrollbar: bool,
    /// Index of the tab being dragged, if any.
    pub(crate) tab_drag: Option<usize>,
    /// Cursor x position at the moment the tab drag began.
    pub(crate) tab_drag_start_x: f64,
}

/// Last known cursor position and held mouse button. Updated on
/// [`winit::event::WindowEvent::CursorMoved`] and `MouseInput`.
#[derive(Default)]
pub(crate) struct CursorState {
    pub(crate) cursor_x: f64,
    pub(crate) cursor_y: f64,
    /// Which mouse button (0=left, 1=mid, 2=right) is currently held, for
    /// motion-reporting passthrough to the PTY (modes 1002/1003).
    pub(crate) mouse_btn_held: Option<u8>,
    /// Time of the last left-click press (for double/triple-click detection).
    pub(crate) last_click_time: Option<std::time::Instant>,
    /// Terminal cell (row, col) of the last left-click (for proximity check).
    pub(crate) last_click_cell: Option<(usize, usize)>,
    /// Whether the previous click was inside the command editor.
    pub(crate) last_click_was_editor: bool,
    /// Consecutive click count: 1 = single, 2 = double, 3 = triple.
    pub(crate) click_count: u8,
}

/// Window geometry plus the renderer's reported per-cell physical pixel size.
/// Updated on `Resized`, `WindowMoved`, and `ScaleFactorChanged` events.
pub(crate) struct LayoutState {
    pub(crate) window_width: u32,
    pub(crate) window_height: u32,
    /// Last known window top-left position in physical pixels.
    pub(crate) window_x: i32,
    pub(crate) window_y: i32,
    /// Current display scale factor (1.0 on standard, 2.0 on Retina, etc.).
    pub(crate) scale_factor: f64,
    /// Actual physical-pixel cell dimensions from the renderer font.
    pub(crate) cell_w: f32,
    pub(crate) cell_h: f32,
}

/// Transient UI overlay state: cursor blink, BEL flash, last-resize hint,
/// update-available banner, and right-click tab context menu.
pub(crate) struct OverlayState {
    /// Time and dimensions of the last PTY resize, shown as an overlay for 1 s.
    pub(crate) last_resize: Option<(Instant, u16, u16)>,
    /// Set when a visual-only resize has been applied but the PTY has not yet
    /// received SIGWINCH. Flushed 100 ms after the last resize event (debounce)
    /// or immediately on drag-release, so rapid resize doesn't spam the shell.
    pub(crate) pending_pty_resize: Option<Instant>,
    /// `false` until the very first Resized event has been applied (startup
    /// correction of the hardcoded initial cell metrics).
    pub(crate) initial_resize_done: bool,
    /// Transient PTY/session status message shown in the resize overlay slot.
    pub(crate) pty_status: Option<(Instant, String)>,
    /// Duration of the last completed command, shown as overlay for 4 s.
    /// Only set when the command took ≥ 1 s.  Format: "3.0s ✓" / "12s ✗".
    pub(crate) last_cmd_duration: Option<(Instant, String)>,
    /// Open generic context menu state.
    pub(crate) context_menu: Option<ContextMenuState>,
    /// Status banner for the last background update check.
    pub(crate) pending_update: Option<UpdateBanner>,
    /// When `Some`, flash the terminal background as a visual BEL indicator
    /// until the contained `Instant`.
    pub(crate) bell_flash_until: Option<Instant>,
    /// Time the cursor blink half-cycle last toggled.
    pub(crate) cursor_blink_last: Instant,
    /// `true` = cursor visible (on-phase); `false` = cursor hidden (off-phase).
    pub(crate) cursor_blink_phase: bool,
    /// Queue of transient toast notifications shown at the bottom-right.
    pub(crate) toasts: VecDeque<Toast>,
    /// The last search query entered by the user so it can be restored on re-open.
    pub(crate) last_search_query: Option<String>,
    /// Currently active modal overlay, if any.
    pub(crate) active_modal: Option<ModalOverlay>,
}

/// Mutually-exclusive modal overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModalOverlay {
    Settings,
    CommandPalette,
    Keybindings,
}

/// State for the interactive keybindings editor panel.
#[derive(Debug, Default)]
pub(crate) struct KeybindingsUiState {
    /// Whether the panel is open.
    pub(crate) open: bool,
    /// Currently highlighted action row index.
    pub(crate) cursor: usize,
    /// First visible row (scroll offset).
    pub(crate) scroll_offset: usize,
    /// When `true`, waiting for the user to press a key combo for the selected action.
    pub(crate) recording: bool,
    /// One-shot: show "Saved" confirmation after a binding is set.
    pub(crate) just_saved: bool,
}

/// Context menu origin and dispatch target.
#[derive(Clone, Debug)]
pub(crate) enum ContextMenuKind {
    Tab { tab_idx: usize },
    Terminal,
    Editor,
}

/// Open context menu state shared by tab bar and terminal pane.
#[derive(Clone, Debug)]
pub(crate) struct ContextMenuState {
    pub(crate) kind: ContextMenuKind,
    pub(crate) x_px: f64,
    pub(crate) y_px: f64,
    pub(crate) hovered_item: Option<usize>,
    pub(crate) items: Vec<String>,
    pub(crate) enabled_items: Vec<bool>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
}

pub(crate) struct Toast {
    pub(crate) text: String,
    pub(crate) kind: ToastKind,
    pub(crate) expires_at: Instant,
}

impl Toast {
    pub(crate) fn new(text: impl Into<String>, kind: ToastKind, ttl: Duration) -> Self {
        Self {
            text: text.into(),
            kind,
            expires_at: Instant::now() + ttl,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum UpdateBanner {
    Available(String),
    Failed(String),
}

// ── Command palette ───────────────────────────────────────────────────────────

/// An action that can be invoked from the command palette.
#[derive(Clone, Debug)]
pub(crate) enum PaletteAction {
    Command(crate::commands::CommandId),
    SetTheme(usize),
    SetFont(usize),
    NewTabWithShell(String),
    /// Open a new tab running the given SSH command (e.g. `ssh user@host`).
    NewSshTab(String),
    /// Switch the palette to SSH sub-prompt mode so the user can type a destination.
    OpenSshPrompt,
    /// Switch to a different open tab by index.
    SwitchToTab(usize),
    /// Insert a command from history into the editor.
    InsertHistoryCommand(String),
    /// Fill the query with a prefix and refilter without closing the palette.
    FilterByPrefix(String),
}

/// A single item in the command palette list.
#[derive(Clone, Debug)]
pub(crate) struct PaletteItem {
    pub(crate) label: String,
    pub(crate) action: PaletteAction,
}

/// Runtime state for the command palette overlay (Cmd+Shift+P).
pub(crate) struct CommandPaletteState {
    /// Current filter query entered by the user.
    pub(crate) query: String,
    /// Byte offset of the text cursor within `query`.
    pub(crate) cursor_byte: usize,
    /// All available items (built when the palette opens and kept stable).
    pub(crate) all_items: Vec<PaletteItem>,
    /// Indices into `all_items` shown when the query is empty (primary actions +
    /// category headers). When the query is non-empty, all items are searched instead.
    pub(crate) default_filtered: Vec<usize>,
    /// Indices into `all_items` that match `query` (all items when query is empty).
    pub(crate) filtered: Vec<usize>,
    /// Index into `filtered` of the currently selected item.
    pub(crate) selected: usize,
    /// Index of the first visible item in the scroll window.
    pub(crate) scroll_offset: usize,
    /// When `Some`, the palette is in sub-prompt mode: the items list is hidden
    /// and this string is used as a label above the text input. Enter executes
    /// the action associated with the prompt (e.g. opening an SSH connection).
    pub(crate) sub_prompt: Option<SubPrompt>,
}

/// Which action to run when the user confirms a sub-prompt.
#[derive(Clone, Debug)]
pub(crate) enum SubPrompt {
    Ssh,
}

const PALETTE_MAX_VISIBLE: usize = 10;

/// Public constant so other modules can share the same scroll-window size.
pub(crate) const PALETTE_MAX_VISIBLE_PUB: usize = PALETTE_MAX_VISIBLE;

impl CommandPaletteState {
    /// Re-build `filtered` from `all_items` and the current `query`.
    /// When the query is empty, shows only `default_filtered` (primary actions +
    /// category headers). When non-empty, searches across all items.
    pub(crate) fn refilter(&mut self) {
        let q = self.query.to_lowercase();
        if q.is_empty() {
            self.filtered = self.default_filtered.clone();
        } else {
            self.filtered = (0..self.all_items.len())
                .filter(|&i| self.all_items[i].label.to_lowercase().contains(&q))
                .collect();
        }
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
        self.recompute_scroll();
    }

    fn recompute_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + PALETTE_MAX_VISIBLE {
            self.scroll_offset = self.selected + 1 - PALETTE_MAX_VISIBLE;
        }
    }

    pub(crate) fn move_up(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.filtered.len() - 1;
        } else {
            self.selected -= 1;
        }
        self.recompute_scroll();
    }

    pub(crate) fn move_down(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.filtered.len();
        self.recompute_scroll();
    }
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            last_resize: None,
            pending_pty_resize: None,
            initial_resize_done: false,
            pty_status: None,
            last_cmd_duration: None,
            context_menu: None,
            pending_update: None,
            bell_flash_until: None,
            cursor_blink_last: Instant::now(),
            cursor_blink_phase: true,
            toasts: VecDeque::new(),
            last_search_query: None,
            active_modal: None,
        }
    }
}

/// Theme and font catalogues discovered at startup plus the index of the
/// currently active preset (if any).
#[derive(Default)]
pub(crate) struct ThemeFontState {
    /// All theme files discovered at startup (sorted by name).
    pub(crate) available_themes: Vec<crate::theme::ThemeFile>,
    /// Index into `available_themes` of the currently active preset, or `None`
    /// when the user is using custom colors.
    pub(crate) active_theme_idx: Option<usize>,
    /// All font families discovered at startup (index 0 = "(default)").
    pub(crate) available_fonts: Vec<FontEntry>,
    /// Index into `available_fonts` of the currently selected font.
    /// 0 means "(default)", i.e. no font family override.
    pub(crate) active_font_idx: usize,
}
