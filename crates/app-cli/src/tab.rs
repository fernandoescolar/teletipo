use app_orchestrator::{App, CommandBlock};
use terminal_pty::PortablePtySession;

use crate::search::SearchState;

/// Copy mode state for scrollback-driven selection (Ctrl+Shift+[).
#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
pub(crate) struct CopyModeState {
    /// Whether copy mode is currently active.
    pub(crate) active: bool,
    /// Cursor row (signed offset from bottom of scrollback/grid; 0 = current grid top).
    pub(crate) cursor_row: isize,
    /// Cursor column.
    pub(crate) cursor_col: usize,
    /// Selection anchor (row, col), if `v` was pressed to start selection.
    pub(crate) anchor: Option<(isize, usize)>,
}

/// Per-command frecency tracking data persisted across sessions.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct HistoryEntry {
    pub(crate) cmd: String,
    /// Number of times this command has been executed.
    pub(crate) count: u32,
    /// Unix timestamp (seconds) of the most recent execution.
    pub(crate) last_used_secs: u64,
}

/// Keep retained per-tab command state bounded across long-running sessions.
pub(crate) const COMMAND_HISTORY_LIMIT: usize = 10_000;
pub(crate) const HISTORY_ENTRIES_LIMIT: usize = 10_000;
pub(crate) const COMMAND_BLOCK_LIMIT: usize = 500;
pub(crate) const RESTORED_TERMINAL_OUTPUT_LINE_LIMIT: usize = 10_000;

pub(crate) fn cap_command_history(history: &mut Vec<String>) {
    let excess = history.len().saturating_sub(COMMAND_HISTORY_LIMIT);
    if excess > 0 {
        history.drain(0..excess);
    }
}

pub(crate) fn cap_history_entries(entries: &mut Vec<HistoryEntry>) {
    if entries.len() <= HISTORY_ENTRIES_LIMIT {
        return;
    }
    entries.sort_by(|a, b| {
        b.last_used_secs
            .cmp(&a.last_used_secs)
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| a.cmd.cmp(&b.cmd))
    });
    entries.truncate(HISTORY_ENTRIES_LIMIT);
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
    /// Unified command blocks, populated in parallel with `pending_cmd`.
    /// Represents all completed command execution blocks for this tab.
    pub(crate) command_blocks: Vec<CommandBlock>,
    /// The in-progress command block opened by `run_editor_command` and closed
    /// by `finalize_pending_cmd`. `None` when no command is in-flight or when
    /// shell integration is inactive.
    pub(crate) current_block: Option<CommandBlock>,
    /// Monotonic counter for generating unique `CommandBlock` IDs. Starts at 1
    /// (never 0) and never wraps.
    pub(crate) next_block_id: u64,
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
    /// `true` when the user has pressed Ctrl+N while a command is running,
    /// unlocking the editor to prepare the next command.  Automatically
    /// reset to `false` when `command_running` transitions to `false`.
    pub(crate) editor_unlocked: bool,
    /// Wall-clock time when the current (or last) command was submitted.
    /// Used to compute execution duration shown in the status overlay.
    pub(crate) command_start_time: Option<std::time::Instant>,
    /// `true` while the tab is in the background and has received new PTY
    /// output that the user has not yet seen.  Cleared when the tab becomes active.
    pub(crate) unread_output: bool,
    /// `true` when the tab has received BEL while in background and the user
    /// has not yet visited it. Cleared when the tab becomes active.
    pub(crate) bell_pending: bool,
    /// Copy mode state (keyboard-driven scrollback selection).
    #[allow(dead_code)]
    pub(crate) copy_mode: CopyModeState,
    /// Screen version seen at the last accessibility-tree push.
    /// Used to skip `update_accessibility_tree` when nothing changed.
    pub(crate) a11y_screen_version: u64,
    /// While `Some`, PTY output is drained without being fed to the screen and
    /// user input is dropped. Set after SIGWINCH so the shell's prompt-redraw
    /// is invisible. Cleared once the instant elapses.
    pub(crate) suppress_until: Option<std::time::Instant>,
    /// Time this tab was spawned. SIGWINCH suppress is skipped while the tab is
    /// younger than this threshold so the shell's initial prompt is never eaten.
    pub(crate) spawned_at: std::time::Instant,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_command_history_keeps_newest_commands() {
        let mut history: Vec<String> = (0..COMMAND_HISTORY_LIMIT + 3)
            .map(|i| format!("cmd-{i}"))
            .collect();

        cap_command_history(&mut history);

        assert_eq!(history.len(), COMMAND_HISTORY_LIMIT);
        assert_eq!(history.first().map(String::as_str), Some("cmd-3"));
        assert_eq!(
            history.last().map(String::as_str),
            Some(format!("cmd-{}", COMMAND_HISTORY_LIMIT + 2).as_str())
        );
    }

    #[test]
    fn cap_history_entries_keeps_recent_entries() {
        let mut entries: Vec<HistoryEntry> = (0..HISTORY_ENTRIES_LIMIT + 3)
            .map(|i| HistoryEntry {
                cmd: format!("cmd-{i}"),
                count: 1,
                last_used_secs: i as u64,
            })
            .collect();

        cap_history_entries(&mut entries);

        assert_eq!(entries.len(), HISTORY_ENTRIES_LIMIT);
        assert_eq!(
            entries.first().map(|entry| entry.cmd.as_str()),
            Some("cmd-10002")
        );
        assert!(entries.iter().all(|entry| entry.cmd != "cmd-0"));
    }
}
