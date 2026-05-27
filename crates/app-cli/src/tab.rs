use app_orchestrator::App;
use terminal_pty::PortablePtySession;

/// Per-command frecency tracking data persisted across sessions.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct HistoryEntry {
    pub(crate) cmd: String,
    /// Number of times this command has been executed.
    pub(crate) count: u32,
    /// Unix timestamp (seconds) of the most recent execution.
    pub(crate) last_used_secs: u64,
}

/// All state that belongs to a single terminal+editor tab.
pub(crate) struct TabState {
    pub(crate) app: App,
    pub(crate) pty: Option<PortablePtySession>,
    /// Scrollback offset for the terminal view (0 = bottom/newest).
    pub(crate) scroll_offset: usize,
    /// How many lines the command editor is scrolled (0 = top visible).
    pub(crate) editor_scroll_offset: usize,
    /// Command history for this tab (oldest first).
    pub(crate) history: Vec<String>,
    /// Index into `history` while navigating; `None` = current (unsaved) input.
    pub(crate) history_index: Option<usize>,
    /// Saved editor content for restoring after history navigation ends.
    pub(crate) saved_input: String,
    /// Fraction of window height used by the terminal pane (0 < r < 1).
    pub(crate) split_ratio: f32,
    /// Terminal text selection anchor (row, col).
    pub(crate) selection_anchor: Option<(usize, usize)>,
    pub(crate) selection_end: Option<(usize, usize)>,
    pub(crate) is_selecting: bool,
    pub(crate) is_selecting_editor: bool,
    /// Terminal text from the most recent snapshot (used for Cmd+C copy).
    pub(crate) last_terminal_text: String,
    /// Number of terminal rows in the most recent snapshot.
    pub(crate) term_row_count: usize,
    /// Cached working directory label shown in the tab bar.
    pub(crate) cwd: String,
    /// The editor text that was active when Tab-cycling began; `None` when not
    /// cycling. Preserved across multiple Tab presses so cycling always searches
    /// from the original typed prefix.
    pub(crate) suggestion_prefix: Option<String>,
    /// Index into the cycling suggestion list corresponding to `suggestion_prefix`.
    /// `None` means "show the top match as ghost text but do not fill the editor".
    pub(crate) suggestion_index: Option<usize>,
    /// Per-command frecency metadata (parallel lookup table, not ordered).
    pub(crate) history_entries: Vec<HistoryEntry>,
}

/// Persistent state for a single tab.
#[derive(serde::Serialize, serde::Deserialize, Default)]
pub(crate) struct TabSession {
    #[serde(default)]
    pub(crate) terminal_output: String,
    #[serde(default)]
    pub(crate) history: Vec<String>,
    #[serde(default = "default_split_ratio")]
    pub(crate) split_ratio: f32,
    /// Last known working directory for this tab. Empty string means unknown.
    #[serde(default)]
    pub(crate) cwd: String,
    /// Frecency data for history entries (empty in old session files — seeded on load).
    #[serde(default)]
    pub(crate) history_entries: Vec<HistoryEntry>,
}

/// Session data persisted to disk across program runs.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct PersistentSession {
    #[serde(default = "default_window_width")]
    pub(crate) window_width: u32,
    #[serde(default = "default_window_height")]
    pub(crate) window_height: u32,
    /// Last known window position in physical pixels. `None` = not yet recorded.
    #[serde(default)]
    pub(crate) window_x: Option<i32>,
    #[serde(default)]
    pub(crate) window_y: Option<i32>,
    /// Per-tab sessions. Empty in old single-tab session files.
    #[serde(default)]
    pub(crate) tabs: Vec<TabSession>,
    // Legacy single-tab fields kept for backward compatibility.
    #[serde(default = "default_split_ratio")]
    pub(crate) split_ratio: f32,
    #[serde(default)]
    pub(crate) history: Vec<String>,
    #[serde(default)]
    pub(crate) terminal_output: String,
}

fn default_window_width() -> u32 {
    1280
}

fn default_window_height() -> u32 {
    720
}

fn default_split_ratio() -> f32 {
    0.7
}

impl Default for PersistentSession {
    fn default() -> Self {
        Self {
            window_width: default_window_width(),
            window_height: default_window_height(),
            window_x: None,
            window_y: None,
            tabs: Vec::new(),
            split_ratio: default_split_ratio(),
            history: Vec::new(),
            terminal_output: String::new(),
        }
    }
}
