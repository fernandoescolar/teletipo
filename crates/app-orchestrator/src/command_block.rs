//! Unified representation of a command execution block.
//!
//! Represents a single command from prompt-through-output: the prompt row,
//! command text, working directory, timing, and exit code. This is the single
//! source of truth for extracting command output, showing sticky overlays,
//! building accessibility tree nodes, and enabling scoped search/rerun features.

use std::path::PathBuf;
use std::time::Instant;

/// A single command execution block with complete lifecycle metadata.
///
/// Created when the user presses Enter (`run_editor_command`) and closed when
/// the shell reports exit code (`finalize_pending_cmd`). The command block
/// bridges the shell integration (`OSC 133` markers) with the user-facing data
/// (command text, working directory, timing).
#[derive(Debug, Clone)]
pub struct CommandBlock {
    /// Monotonic ID: never 0, unique within the terminal session.
    /// Used as a correlation key instead of array index to avoid sync bugs when
    /// empty-Enter creates zones but no history entries (a known source of
    /// duplication between `command_zones` and `history`).
    pub id: u64,

    /// The shell command as typed by the user (after trimming whitespace).
    pub command: String,

    /// Working directory when the command was executed. Snapshotted from OSC 7
    /// at the time the command is entered, not re-read at exit.
    pub cwd: Option<PathBuf>,

    /// Absolute row where the shell showed the prompt (`OSC 133;A`).
    pub prompt_row: usize,

    /// Absolute row where the command output began (`OSC 133;C`). `None` if
    /// the command has not produced any output yet.
    pub output_start_row: Option<usize>,

    /// Absolute row where the command's output ended (i.e., where the next
    /// prompt begins, or current cursor row if this is the most recent block).
    /// Filled in when the *next* block's prompt opens (`OSC 133;A`), not when
    /// this block's exit code arrives (`OSC 133;D`).
    /// `None` while this block is still the active/latest block.
    pub output_end_row: Option<usize>,

    /// Exit code reported by the shell (`OSC 133;D;N`). `None` if the command
    /// is still running or shell integration is not active.
    pub exit_code: Option<i32>,

    /// Wall-clock time when the user pressed Enter. Set at `run_editor_command`.
    pub started_at: Option<Instant>,

    /// Wall-clock time when the shell reported exit code (`OSC 133;D`).
    /// `None` while the command is still running.
    pub finished_at: Option<Instant>,
}

impl CommandBlock {
    /// Create a new block when the user executes a command.
    ///
    /// Called from `run_editor_command` at the moment Enter is pressed.
    /// Returns `None` if the command text is empty.
    pub fn open(
        id: u64,
        command: String,
        cwd: Option<PathBuf>,
        prompt_row: usize,
    ) -> Option<Self> {
        if command.trim().is_empty() {
            return None;
        }
        Some(Self {
            id,
            command,
            cwd,
            prompt_row,
            output_start_row: None,
            output_end_row: None,
            exit_code: None,
            started_at: Some(Instant::now()),
            finished_at: None,
        })
    }

    /// Mark this block as finished with an exit code and timestamp.
    ///
    /// Called from `finalize_pending_cmd` when `OSC 133;D` arrives.
    pub fn close(&mut self, exit_code: i32) {
        self.exit_code = Some(exit_code);
        self.finished_at = Some(Instant::now());
    }

    /// Duration this command ran, if available.
    pub fn duration(&self) -> Option<std::time::Duration> {
        match (self.started_at, self.finished_at) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            _ => None,
        }
    }

    /// Whether this command is still running (started but not exited).
    pub fn is_running(&self) -> bool {
        self.started_at.is_some() && self.finished_at.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_returns_none_for_empty_command() {
        assert!(CommandBlock::open(1, String::new(), None, 0).is_none());
        assert!(CommandBlock::open(1, "   ".to_owned(), None, 0).is_none());
    }

    #[test]
    fn open_returns_some_for_nonempty_command() {
        let block = CommandBlock::open(
            1,
            "echo hello".to_owned(),
            Some(PathBuf::from("/home/user")),
            5,
        );
        assert!(block.is_some());
        assert_eq!(block.unwrap().command, "echo hello");
    }

    #[test]
    fn newly_opened_block_is_running() {
        let block = CommandBlock::open(1, "sleep 1".to_owned(), None, 0).unwrap();
        assert!(block.is_running());
    }

    #[test]
    fn closed_block_is_not_running() {
        let mut block = CommandBlock::open(1, "echo test".to_owned(), None, 0).unwrap();
        block.close(0);
        assert!(!block.is_running());
        assert_eq!(block.exit_code, Some(0));
        assert!(block.finished_at.is_some());
    }
}
