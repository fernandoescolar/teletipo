use std::collections::HashSet;

use app_orchestrator::App;
use terminal_pty::PortablePtySession;

use crate::search::SearchState;

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
    /// Horizontal editor viewport offset in character cells.
    pub(crate) editor_horizontal_scroll_offset: usize,
    /// Command history for this tab (oldest first).
    pub(crate) history: Vec<String>,
    /// Index into `history` while navigating; `None` = current (unsaved) input.
    pub(crate) history_index: Option<usize>,
    /// Saved editor content for restoring after history navigation ends.
    pub(crate) saved_input: String,
    /// Fraction of window height used by the terminal pane (0 < r < 1).
    pub(crate) split_ratio: f32,
    /// Whether this tab is currently presenting terminal fullscreen mode.
    pub(crate) was_terminal_fullscreen: bool,
    /// Split ratio from before entering fullscreen, restored when exiting.
    pub(crate) pre_fullscreen_split_ratio: f32,
    /// Terminal text selection anchor (row, col) and the scroll_offset at the
    /// time the selection point was recorded.  Rows must be adjusted by the
    /// scroll delta when rendering or copying.
    pub(crate) selection_anchor: Option<(usize, usize)>,
    pub(crate) selection_anchor_scroll: usize,
    pub(crate) selection_end: Option<(usize, usize)>,
    pub(crate) selection_end_scroll: usize,
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
    /// Command that has been sent to the PTY but whose exit code has not yet
    /// been received.  `None` when no command is in-flight or when shell
    /// integration is inactive.
    pub(crate) pending_cmd: Option<String>,
    /// `true` when the shell was spawned with OSC 133 exit-code integration.
    /// When `false`, commands are saved to history immediately on Enter.
    pub(crate) shell_integration: bool,
    /// Inline terminal find panel state (Cmd+F).
    pub(crate) search: SearchState,
    /// `true` when the shell is currently waiting on a foreground child
    /// command (vim, sudo, ssh, scripts, …) as reported by `tcgetpgrp` on
    /// the PTY slave.  Refreshed each frame in `pump_all_ptys`.  When set
    /// the UI bypasses the command editor and routes keystrokes directly to
    /// the PTY, so the running program sees them verbatim.
    pub(crate) command_running: bool,
    /// `true` while the tab is in the background and has received new PTY
    /// output that the user has not yet seen.  Cleared when the tab becomes active.
    pub(crate) unread_output: bool,
    /// `true` when the tab has received BEL while in background and the user
    /// has not yet visited it. Cleared when the tab becomes active.
    pub(crate) bell_pending: bool,
    /// Screen version seen at the last accessibility-tree push.
    /// Used to skip `update_accessibility_tree` when nothing changed.
    pub(crate) a11y_screen_version: u64,
    /// Selected structured command block, if any.
    pub(crate) selected_block: Option<app_orchestrator::BlockId>,
    /// Command blocks whose long output is visually collapsed.
    pub(crate) collapsed_blocks: HashSet<app_orchestrator::BlockId>,
    /// Effective scrollback line count after subtracting lines hidden by
    /// collapsed blocks.  Updated every frame by `build_snapshot`.  Input
    /// handlers use this instead of `app.scrollback_len()` so that scroll
    /// bounds and the scrollbar drag scale respect the virtual content size.
    pub(crate) virtual_scrollback_lines: usize,
    /// Rows hidden by collapsed blocks, in absolute row coordinates: sorted,
    /// non-overlapping `(start_abs, len)` entries.  Updated every frame by
    /// `build_snapshot`; used to map viewport rows back to absolute rows
    /// (block selection, quick actions) while blocks are folded.
    pub(crate) collapsed_hidden_ranges: Vec<(usize, usize)>,
    /// First virtual row shown in the viewport in the last rendered frame.
    /// Cached so click handlers map view_row → abs_row using the exact same
    /// geometry that placed the pixels, instead of recomputing with a
    /// potentially-stale scrollback length.
    pub(crate) last_frame_v_start: usize,
    /// Restored session content not yet replayed into the terminal.  Held
    /// until the terminal reaches its real on-screen size so the content is
    /// laid out only once: feeding at the startup placeholder size and then
    /// resizing would reflow the grid and invalidate the absolute row numbers
    /// the restored command blocks point at.  Flushed by
    /// [`crate::runtime::GpuRuntimeState::flush_pending_restore`].
    pub(crate) pending_restore: Option<PendingRestore>,
    /// Command blocks captured as text the moment they completed, in arrival
    /// order.  Persisted to the session file and replayed on next launch.
    pub(crate) saved_blocks: Vec<SavedBlock>,
    /// Number of `execution_blocks()` already captured into `saved_blocks`,
    /// so each completed block is captured exactly once.
    pub(crate) captured_block_count: usize,
}

/// Saved command blocks awaiting replay into a freshly sized terminal.
/// `terminal_output` is only used as a fallback when a tab has no captured
/// blocks (e.g. plain scrollback).  See [`TabState::pending_restore`].
pub(crate) struct PendingRestore {
    pub(crate) terminal_output: String,
    pub(crate) blocks: Vec<SavedBlock>,
}

/// A finished command block persisted across sessions.
///
/// Stored as the captured ANSI text of each region rather than absolute row
/// numbers: rows drift whenever the window is resized mid-session (the grid
/// reflows but block rows are not remapped), so on restore the blocks are
/// replayed through a synthetic OSC 133 stream and their positions recomputed
/// from scratch.  The text is captured the moment a block completes, while its
/// rows are still valid.
#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
pub(crate) struct SavedBlock {
    /// Prompt region (everything from the prompt start up to the command).
    #[serde(default)]
    pub(crate) prompt_ansi: String,
    /// The echoed command line(s) shown between the prompt and the output.
    #[serde(default)]
    pub(crate) command_ansi: String,
    /// Command output region.
    #[serde(default)]
    pub(crate) output_ansi: String,
    /// Logical command text (drives copy / re-run / edit).
    #[serde(default)]
    pub(crate) command: Option<String>,
    #[serde(default)]
    pub(crate) exit_code: Option<i32>,
    #[serde(default)]
    pub(crate) duration_ms: Option<u64>,
    /// Unix seconds (UTC) when the command started — `None` for blocks loaded
    /// from older session files that did not record this field.
    #[serde(default)]
    pub(crate) started_at_secs: Option<u64>,
    #[serde(default)]
    pub(crate) cwd: Option<String>,
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
    /// Finished command blocks (empty in old session files).
    #[serde(default)]
    pub(crate) blocks: Vec<SavedBlock>,
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
