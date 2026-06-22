use std::io;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use editor_core::EditorBuffer;
use terminal_core::{DamageRegion, StyledChars, TerminalError, TerminalSession};
use terminal_pty::PtyBackend;

// ============================================================================
// History sub-struct
// ============================================================================

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub cmd: String,
    pub count: u32,
    pub last_used_secs: u64,
}

/// Stores command history with frecency-based ranking.
pub struct AppHistory {
    entries: Vec<HistoryEntry>,
}

impl AppHistory {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn record(&mut self, cmd: &str) {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return;
        }

        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(entry) = self.entries.iter_mut().find(|e| e.cmd.trim() == trimmed) {
            entry.count = entry.count.saturating_add(1);
            entry.last_used_secs = now_secs;
            return;
        }

        self.entries.push(HistoryEntry {
            cmd: trimmed.to_owned(),
            count: 1,
            last_used_secs: now_secs,
        });
    }

    pub fn frecency_suggestions(&self, prefix: &str, limit: usize) -> Vec<String> {
        let p = prefix.trim().to_lowercase();
        if p.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut matches: Vec<&HistoryEntry> = self
            .entries
            .iter()
            .filter(|entry| entry.cmd.to_lowercase().starts_with(&p))
            .collect();

        matches.sort_by(|a, b| {
            let a_score = (a.count as u64)
                .saturating_mul(1_000_000)
                .saturating_add(a.last_used_secs);
            let b_score = (b.count as u64)
                .saturating_mul(1_000_000)
                .saturating_add(b.last_used_secs);
            b_score.cmp(&a_score).then_with(|| a.cmd.cmp(&b.cmd))
        });

        matches
            .into_iter()
            .take(limit)
            .map(|e| e.cmd.clone())
            .collect()
    }

    pub fn at(&self, idx: usize) -> Option<&str> {
        self.entries.get(idx).map(|e| e.cmd.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }
}

// ============================================================================
// Editor sub-struct
// ============================================================================

/// Wraps the editor buffer (text input, cursor, undo/redo, selection).
pub struct AppEditor {
    buffer: EditorBuffer,
}

impl AppEditor {
    fn new() -> Self {
        Self {
            buffer: EditorBuffer::new(),
        }
    }

    pub fn insert_str(&mut self, text: &str) {
        self.buffer.insert_str(text);
    }

    pub fn backspace(&mut self) {
        self.buffer.backspace();
    }

    pub fn delete_forward(&mut self) {
        self.buffer.delete_forward();
    }

    /// Returns the selected byte range, or `None` when nothing is selected.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let sel = self.buffer.selection();
        let (start, end) = sel.normalized();
        if start == end {
            None
        } else {
            Some((start, end))
        }
    }

    pub fn can_undo(&self) -> bool {
        self.buffer.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.buffer.can_redo()
    }

    pub fn undo(&mut self) {
        self.buffer.undo();
    }

    pub fn redo(&mut self) {
        self.buffer.redo();
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn cursor_offset(&self) -> usize {
        self.buffer.cursor().offset
    }

    pub fn set_cursor(&mut self, offset: usize, extend_selection: bool) {
        self.buffer.set_cursor(offset, extend_selection);
    }

    pub fn move_cursor_left(&mut self, extend_selection: bool) {
        self.buffer.move_cursor_left(extend_selection);
    }

    pub fn move_cursor_right(&mut self, extend_selection: bool) {
        self.buffer.move_cursor_right(extend_selection);
    }

    pub fn delete_to_line_start(&mut self) {
        self.buffer.delete_to_line_start();
    }

    pub fn delete_to_line_end(&mut self) {
        self.buffer.delete_to_line_end();
    }

    pub fn delete_word_backward(&mut self) {
        self.buffer.delete_word_backward();
    }

    pub fn snapshot(&self) -> String {
        self.buffer.text().to_string()
    }

    /// Returns the payload to send when the user executes the current command,
    /// respecting `prefer_selection`.  Returns `None` when there is nothing to run.
    pub(crate) fn command_payload(&self, prefer_selection: bool) -> Option<String> {
        self.buffer
            .command_payload(prefer_selection)
            .map(|s| s.to_string())
    }
}

// ============================================================================
// Terminal sub-struct
// ============================================================================

/// Wraps the terminal session: screen state, VT parsing, PTY I/O helpers.
pub struct AppTerminal {
    session: TerminalSession,
}

impl AppTerminal {
    fn new(rows: usize, cols: usize) -> Result<Self, TerminalError> {
        Ok(Self {
            session: TerminalSession::new(rows, cols)?,
        })
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.session.feed(bytes);
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.session.resize(rows, cols);
    }

    pub fn snapshot_text(&self) -> String {
        self.session.snapshot_text()
    }

    pub fn snapshot_text_with_scrollback(&self) -> String {
        self.session.snapshot_text_with_scrollback()
    }

    pub fn snapshot_ansi(&self) -> std::sync::Arc<String> {
        self.session.snapshot_ansi()
    }

    pub fn snapshot_styled(&self) -> StyledChars {
        self.session.snapshot_styled()
    }

    pub fn snapshot_styled_at_offset(&self, scroll_offset: usize) -> StyledChars {
        self.session.snapshot_styled_at_offset(scroll_offset)
    }

    pub fn snapshot_styled_at_offset_with_palette(
        &self,
        scroll_offset: usize,
        palette: Option<&[[f32; 3]; 16]>,
    ) -> StyledChars {
        self.session
            .snapshot_styled_at_offset_with_palette(scroll_offset, palette)
    }

    pub fn scrollback_len(&self) -> usize {
        self.session.scrollback_len()
    }

    pub fn screen_version(&self) -> u64 {
        self.session.screen_version()
    }

    pub fn take_last_exit_code(&mut self) -> Option<i32> {
        self.session.take_last_exit_code()
    }

    pub fn mouse_mode(&self) -> u16 {
        self.session.mouse_mode()
    }

    pub fn mouse_sgr(&self) -> bool {
        self.session.mouse_sgr()
    }

    pub fn bracketed_paste(&self) -> bool {
        self.session.bracketed_paste()
    }

    pub fn kitty_keyboard_flags(&self) -> u32 {
        self.session.kitty_keyboard_flags()
    }

    pub fn drain_pending_responses(&mut self) -> Vec<String> {
        self.session.drain_pending_responses()
    }

    pub fn cursor_shape(&self) -> u16 {
        self.session.cursor_shape()
    }

    pub fn window_title(&self) -> Option<&str> {
        self.session.window_title()
    }

    pub fn take_bell(&mut self) -> bool {
        self.session.take_bell()
    }

    pub fn cursor_pos(&self) -> (usize, usize) {
        self.session.cursor_pos()
    }

    pub fn take_damage(&mut self) -> DamageRegion {
        self.session.take_damage()
    }

    pub fn is_alternate_screen(&self) -> bool {
        self.session.is_alternate_screen()
    }

    pub fn application_cursor_keys(&self) -> bool {
        self.session.application_cursor_keys()
    }

    pub fn prompt_marks(&self) -> Vec<usize> {
        self.session.prompt_marks()
    }

    /// All completed command zones (OSC 133 A–D lifecycle), oldest first.
    pub fn command_zones(&self) -> &[terminal_core::CommandZone] {
        self.session.command_zones()
    }

    /// Absolute row where the current (in-progress) prompt started, if any.
    /// Used to bound the output of the last completed command zone.
    pub fn current_zone_prompt_row(&self) -> Option<usize> {
        self.session.current_zone().map(|z| z.prompt_start_row)
    }

    /// Returns all OSC 8 hyperlink spans visible at the given scroll offset.
    /// See [`terminal_core::GenericTerminalSession::hyperlink_spans`].
    pub fn hyperlink_spans(&self, scroll_offset: usize) -> Vec<(usize, usize, usize, u16)> {
        self.session.hyperlink_spans(scroll_offset)
    }

    /// Resolves a hyperlink ID to its URI string. Returns `None` for ID 0.
    pub fn hyperlink_uri(&self, id: u16) -> Option<&str> {
        self.session.hyperlink_uri(id)
    }

    /// Working directory last reported by the shell via OSC 7.
    pub fn osc7_cwd(&self) -> Option<&std::path::Path> {
        self.session.osc7_cwd()
    }
}

// ============================================================================
// App facade — delegates to sub-structs and owns cross-cutting PTY logic
// ============================================================================

/// Top-level application state.  Holds three sub-structs for terminal I/O,
/// editor input, and command history.  PTY plumbing lives here because it
/// crosses the terminal/editor boundary.
pub struct App {
    pub terminal: AppTerminal,
    pub editor: AppEditor,
    pub history: AppHistory,
}

impl App {
    pub fn new(rows: usize, cols: usize) -> Result<Self, TerminalError> {
        Ok(Self {
            terminal: AppTerminal::new(rows, cols)?,
            editor: AppEditor::new(),
            history: AppHistory::new(),
        })
    }

    // --- History delegation (preserves existing public API) ---

    pub fn record_history(&mut self, cmd: &str) {
        self.history.record(cmd);
    }

    pub fn frecency_suggestions(&self, prefix: &str, limit: usize) -> Vec<String> {
        self.history.frecency_suggestions(prefix, limit)
    }

    pub fn history_at(&self, idx: usize) -> Option<&str> {
        self.history.at(idx)
    }

    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn history_entries(&self) -> &[HistoryEntry] {
        self.history.entries()
    }

    pub fn feed_terminal(&mut self, bytes: &[u8]) {
        self.terminal.feed(bytes);
    }

    pub fn resize_terminal(&mut self, rows: usize, cols: usize) {
        self.terminal.resize(rows, cols);
    }

    pub fn insert_editor_input(&mut self, text: &str) {
        self.editor.insert_str(text);
    }

    pub fn editor_backspace(&mut self) {
        self.editor.backspace();
    }

    pub fn editor_delete_forward(&mut self) {
        self.editor.delete_forward();
    }

    /// Returns the selected byte range in the editor, or `None` if nothing is selected.
    pub fn editor_selection(&self) -> Option<(usize, usize)> {
        self.editor.selection()
    }

    pub fn editor_can_undo(&self) -> bool {
        self.editor.can_undo()
    }

    pub fn editor_can_redo(&self) -> bool {
        self.editor.can_redo()
    }

    pub fn editor_undo(&mut self) {
        self.editor.undo();
    }

    pub fn editor_redo(&mut self) {
        self.editor.redo();
    }

    pub fn editor_clear(&mut self) {
        self.editor.clear();
    }

    pub fn editor_delete_to_line_start(&mut self) {
        self.editor.delete_to_line_start();
    }

    pub fn editor_delete_to_line_end(&mut self) {
        self.editor.delete_to_line_end();
    }

    pub fn editor_delete_word_backward(&mut self) {
        self.editor.delete_word_backward();
    }

    pub fn editor_cursor_offset(&self) -> usize {
        self.editor.cursor_offset()
    }

    pub fn set_editor_cursor(&mut self, offset: usize, extend_selection: bool) {
        self.editor.set_cursor(offset, extend_selection);
    }

    pub fn editor_move_cursor_left(&mut self, extend_selection: bool) {
        self.editor.move_cursor_left(extend_selection);
    }

    pub fn editor_move_cursor_right(&mut self, extend_selection: bool) {
        self.editor.move_cursor_right(extend_selection);
    }

    pub fn send_pty_input<B: PtyBackend>(&mut self, pty: &mut B, bytes: &[u8]) -> io::Result<()> {
        pty.write_input(bytes)
    }

    pub fn run_editor_command<B: PtyBackend>(
        &mut self,
        pty: &mut B,
        prefer_selection: bool,
    ) -> io::Result<bool> {
        if let Some(payload) = self.editor.command_payload(prefer_selection) {
            // Always submit through bracketed paste.  Modern shells (zsh, bash,
            // fish) all support it.  Sending as raw keypresses makes readline/ZLE
            // repeatedly redraw and reposition the prompt (wasteful and prone to
            // corrupting long lines), and causes visual inconsistency on the first
            // command if the shell hasn't yet emitted \e[?2004h when Enter is pressed.
            pty.write_input(b"\x1b[200~")?;
            pty.write_input(payload.as_bytes())?;
            pty.write_input(b"\x1b[201~")?;
            // PTYs expect Enter as CR; sending CRLF can be interpreted as two
            // submits by some shells (notably WSL/Git Bash via ConPTY).
            pty.write_input(b"\r")?;
            self.editor.clear();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn pump_pty_once<B: PtyBackend>(&mut self, pty: &mut B) -> io::Result<usize> {
        let mut out = Vec::new();
        let n = pty.try_read_output(&mut out)?;
        if n > 0 {
            self.feed_terminal(&out);
        }
        Ok(n)
    }

    /// Read PTY output and discard it without feeding it to the terminal screen.
    /// Used during the SIGWINCH suppress window to swallow the shell's
    /// prompt-redraw so it never appears in the terminal view.
    pub fn drain_pty_output<B: PtyBackend>(&mut self, pty: &mut B) -> io::Result<usize> {
        let mut out = Vec::new();
        pty.try_read_output(&mut out)
    }

    pub fn pump_until_quiet<B: PtyBackend>(
        &mut self,
        pty: &mut B,
        max_ticks: usize,
        tick_sleep: Duration,
    ) -> io::Result<usize> {
        let mut total = 0usize;
        let mut quiet_ticks = 0usize;

        for _ in 0..max_ticks {
            let n = self.pump_pty_once(pty)?;
            total += n;

            if n == 0 {
                quiet_ticks += 1;
                if quiet_ticks >= 3 {
                    break;
                }
            } else {
                quiet_ticks = 0;
            }

            thread::sleep(tick_sleep);
        }

        Ok(total)
    }

    pub fn terminal_snapshot(&self) -> String {
        self.terminal.snapshot_text()
    }

    pub fn terminal_snapshot_with_scrollback(&self) -> String {
        self.terminal.snapshot_text_with_scrollback()
    }

    /// Returns the screen's monotonic version counter — incremented on every
    /// write.  Callers can compare across frames to skip work when the
    /// terminal content has not changed.
    pub fn terminal_screen_version(&self) -> u64 {
        self.terminal.screen_version()
    }

    pub fn prompt_marks(&self) -> Vec<usize> {
        self.terminal.prompt_marks()
    }

    pub fn terminal_ansi_snapshot(&self) -> std::sync::Arc<String> {
        self.terminal.snapshot_ansi()
    }

    pub fn terminal_styled_snapshot(&self) -> StyledChars {
        self.terminal.snapshot_styled()
    }

    pub fn terminal_styled_snapshot_at_offset(&self, scroll_offset: usize) -> StyledChars {
        self.terminal.snapshot_styled_at_offset(scroll_offset)
    }

    /// Like `terminal_styled_snapshot_at_offset` but overrides ANSI indexed
    /// colors 0-15 using `palette` when provided.
    pub fn terminal_styled_snapshot_at_offset_with_palette(
        &self,
        scroll_offset: usize,
        palette: Option<&[[f32; 3]; 16]>,
    ) -> StyledChars {
        self.terminal
            .snapshot_styled_at_offset_with_palette(scroll_offset, palette)
    }

    pub fn scrollback_len(&self) -> usize {
        self.terminal.scrollback_len()
    }

    pub fn editor_snapshot(&self) -> String {
        self.editor.snapshot()
    }

    /// Consumes and returns the exit code from the most recent OSC 133;D shell
    /// integration sequence, or `None` if no new report has arrived.
    pub fn take_last_exit_code(&mut self) -> Option<i32> {
        self.terminal.take_last_exit_code()
    }

    /// Active mouse tracking mode (0 = off, 1000/1002/1003).
    pub fn mouse_mode(&self) -> u16 {
        self.terminal.mouse_mode()
    }

    /// Whether SGR extended mouse encoding (DEC 1006) is active.
    pub fn mouse_sgr(&self) -> bool {
        self.terminal.mouse_sgr()
    }

    /// Whether bracketed paste mode (DEC 2004) is active.
    pub fn bracketed_paste(&self) -> bool {
        self.terminal.bracketed_paste()
    }

    /// Current kitty keyboard protocol flags (top of stack, 0 = disabled).
    pub fn kitty_keyboard_flags(&self) -> u32 {
        self.terminal.kitty_keyboard_flags()
    }

    /// Drains pending PTY response strings (e.g. cursor-position reports).
    pub fn drain_pending_responses(&mut self) -> Vec<String> {
        self.terminal.drain_pending_responses()
    }

    /// Current cursor shape (DECSCUSR value).
    pub fn cursor_shape(&self) -> u16 {
        self.terminal.cursor_shape()
    }

    /// Window title set by OSC 0/2, if any.
    pub fn window_title(&self) -> Option<&str> {
        self.terminal.window_title()
    }

    /// Returns `true` and clears the flag if a BEL was received since the last call.
    pub fn take_bell(&mut self) -> bool {
        self.terminal.take_bell()
    }

    /// Returns the terminal cursor position as `(row, col)`, 0-based.
    pub fn terminal_cursor_pos(&self) -> (usize, usize) {
        self.terminal.cursor_pos()
    }

    pub fn terminal_take_damage(&mut self) -> DamageRegion {
        self.terminal.take_damage()
    }

    /// Returns whether the terminal currently uses the alternate screen buffer.
    pub fn is_alternate_screen(&self) -> bool {
        self.terminal.is_alternate_screen()
    }

    /// Returns whether application cursor keys mode (DECCKM) is active.
    pub fn application_cursor_keys(&self) -> bool {
        self.terminal.application_cursor_keys()
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    use crate::runtime::{AppEvent, AppRuntime, RuntimeConfig};
    use std::time::Duration;
    use terminal_pty::MockPty;

    const LINE_ENDING: &[u8] = b"\r";

    /// Construct an `App` with the given terminal dimensions for testing.
    /// Panics if construction fails (invalid size).
    pub(super) fn make_app(rows: usize, cols: usize) -> App {
        App::new(rows, cols).expect("make_app: valid size")
    }

    #[test]
    fn app_wires_terminal_and_editor() {
        let mut app = make_app(4, 12);
        app.feed_terminal(b"ready");
        app.insert_editor_input("ls -la");

        assert!(app.terminal_snapshot().contains("ready"));
        assert_eq!(app.editor_snapshot(), "ls -la");
    }

    #[test]
    fn app_pumps_pty_output() {
        let mut app = make_app(4, 16);
        let mut pty = MockPty::default();
        pty.push_output(b"from-pty\n");

        let pumped = app.pump_pty_once(&mut pty).expect("pump");

        assert!(pumped > 0);
        assert!(app.terminal_snapshot().contains("from-pty"));
    }

    #[test]
    fn app_pumps_until_quiet() {
        let mut app = make_app(4, 16);
        let mut pty = MockPty::default();
        pty.push_output(b"a");
        pty.push_output(b"b");

        let pumped = app
            .pump_until_quiet(&mut pty, 8, Duration::from_millis(1))
            .expect("pump");

        assert!(pumped >= 2);
    }

    #[test]
    fn runs_editor_buffer_command_to_pty() {
        let mut app = make_app(4, 16);
        let mut pty = MockPty::default();

        app.insert_editor_input("echo run-buffer");
        let sent = app.run_editor_command(&mut pty, false).expect("run cmd");

        assert!(sent);
        let expected = [
            b"\x1b[200~".as_ref(),
            b"echo run-buffer",
            b"\x1b[201~",
            LINE_ENDING,
        ]
        .concat();
        assert_eq!(pty.input_log(), expected.as_slice());
    }

    #[test]
    fn wraps_editor_command_in_bracketed_paste() {
        let mut app = make_app(4, 32);
        let mut pty = MockPty::default();
        app.insert_editor_input("openssl s_client -connect example.com:443 </dev/null");

        app.run_editor_command(&mut pty, false).expect("run");

        assert_eq!(
            pty.input_log(),
            b"\x1b[200~openssl s_client -connect example.com:443 </dev/null\x1b[201~\r"
        );
    }

    #[test]
    fn editor_clears_after_run_command() {
        let mut app = make_app(4, 16);
        let mut pty = MockPty::default();

        app.insert_editor_input("ls -la");
        app.run_editor_command(&mut pty, false).expect("run");

        assert_eq!(
            app.editor_snapshot(),
            "",
            "editor must be empty after execution"
        );
    }

    #[test]
    fn editor_cursor_offset_tracks_inserts() {
        let mut app = make_app(4, 16);
        assert_eq!(app.editor_cursor_offset(), 0);
        app.insert_editor_input("abc");
        assert_eq!(app.editor_cursor_offset(), 3);
        app.editor_backspace();
        assert_eq!(app.editor_cursor_offset(), 2);
    }

    #[test]
    fn runs_editor_selection_command_to_pty() {
        let mut app = make_app(4, 32);
        let mut pty = MockPty::default();

        app.insert_editor_input("echo selected words");
        app.set_editor_cursor(5, false);
        app.set_editor_cursor(13, true);
        let sent = app
            .run_editor_command(&mut pty, true)
            .expect("run selection");

        assert!(sent);
        let expected = [
            b"\x1b[200~".as_ref(),
            b"selected",
            b"\x1b[201~",
            LINE_ENDING,
        ]
        .concat();
        assert_eq!(pty.input_log(), expected.as_slice());
    }

    #[test]
    fn runtime_routes_events() {
        let mut app = make_app(4, 16);
        let rt = AppRuntime::new(RuntimeConfig::default());
        let tx = rt.sender();

        tx.send(AppEvent::PtyOutput(b"evt\n".to_vec()))
            .expect("send");
        assert!(rt.step(&mut app));
        assert!(app.terminal_snapshot().contains("evt"));

        tx.send(AppEvent::Shutdown).expect("send");
        assert!(!rt.step(&mut app));
    }

    #[test]
    fn records_and_suggests_history_by_frecency() {
        let mut app = make_app(4, 16);

        app.record_history("cargo test");
        app.record_history("cargo build");
        app.record_history("cargo test");

        assert_eq!(app.history_len(), 2);
        let suggestions = app.frecency_suggestions("cargo", 5);
        assert_eq!(suggestions.first().map(String::as_str), Some("cargo test"));
    }
}
