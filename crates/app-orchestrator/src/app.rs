use std::io;
use std::thread;
use std::time::Duration;

use editor_core::EditorBuffer;
use terminal_core::{TerminalError, TerminalSession, StyledChars};
use terminal_pty::PtyBackend;

pub struct App {
    terminal: TerminalSession,
    editor: EditorBuffer,
}

impl App {
    pub fn new(rows: usize, cols: usize) -> Result<Self, TerminalError> {
        Ok(Self {
            terminal: TerminalSession::new(rows, cols)?,
            editor: EditorBuffer::new(),
        })
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
        let sel = self.editor.selection();
        let (start, end) = sel.normalized();
        if start == end { None } else { Some((start, end)) }
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

    pub fn editor_cursor_offset(&self) -> usize {
        self.editor.cursor().offset
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

    pub fn send_pty_input<B: PtyBackend>(
        &mut self,
        pty: &mut B,
        bytes: &[u8],
    ) -> io::Result<()> {
        pty.write_input(bytes)
    }

    pub fn run_editor_command<B: PtyBackend>(
        &mut self,
        pty: &mut B,
        prefer_selection: bool,
    ) -> io::Result<bool> {
        if let Some(payload) = self.editor.command_payload(prefer_selection) {
            pty.write_input(payload.as_bytes())?;
            pty.write_input(b"\n")?;
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

    pub fn terminal_ansi_snapshot(&self) -> String {
        self.terminal.snapshot_ansi()
    }

    pub fn terminal_styled_snapshot(&self) -> StyledChars {
        self.terminal.snapshot_styled()
    }

    pub fn terminal_styled_snapshot_at_offset(
        &self,
        scroll_offset: usize,
    ) -> StyledChars {
        self.terminal.snapshot_styled_at_offset(scroll_offset)
    }

    /// Like `terminal_styled_snapshot_at_offset` but overrides ANSI indexed
    /// colors 0-15 using `palette` when provided.
    pub fn terminal_styled_snapshot_at_offset_with_palette(
        &self,
        scroll_offset: usize,
        palette: Option<&[[f32; 3]; 16]>,
    ) -> StyledChars {
        self.terminal.snapshot_styled_at_offset_with_palette(scroll_offset, palette)
    }

    pub fn scrollback_len(&self) -> usize {
        self.terminal.scrollback_len()
    }

    pub fn editor_snapshot(&self) -> String {
        self.editor.text().to_string()
    }

    /// Consumes and returns the exit code from the most recent OSC 133;D shell
    /// integration sequence, or `None` if no new report has arrived.
    pub fn take_last_exit_code(&mut self) -> Option<i32> {
        self.terminal.take_last_exit_code()
    }

    /// Active mouse reporting mode (0 = off, 1000/1002/1003/1006).
    pub fn mouse_mode(&self) -> u16 {
        self.terminal.mouse_mode()
    }

    /// Whether bracketed paste mode (DEC 2004) is active.
    pub fn bracketed_paste(&self) -> bool {
        self.terminal.bracketed_paste()
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

    /// Returns whether the terminal currently uses the alternate screen buffer.
    pub fn is_alternate_screen(&self) -> bool {
        self.terminal.is_alternate_screen()
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    use crate::runtime::{AppEvent, AppRuntime, RuntimeConfig};
    use terminal_pty::MockPty;
    use std::time::Duration;

    #[test]
    fn app_wires_terminal_and_editor() {
        let mut app = App::new(4, 12).expect("app");
        app.feed_terminal(b"ready");
        app.insert_editor_input("ls -la");

        assert!(app.terminal_snapshot().contains("ready"));
        assert_eq!(app.editor_snapshot(), "ls -la");
    }

    #[test]
    fn app_pumps_pty_output() {
        let mut app = App::new(4, 16).expect("app");
        let mut pty = MockPty::default();
        pty.push_output(b"from-pty\n");

        let pumped = app.pump_pty_once(&mut pty).expect("pump");

        assert!(pumped > 0);
        assert!(app.terminal_snapshot().contains("from-pty"));
    }

    #[test]
    fn app_pumps_until_quiet() {
        let mut app = App::new(4, 16).expect("app");
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
        let mut app = App::new(4, 16).expect("app");
        let mut pty = MockPty::default();

        app.insert_editor_input("echo run-buffer");
        let sent = app.run_editor_command(&mut pty, false).expect("run cmd");

        assert!(sent);
        assert_eq!(pty.input_log(), b"echo run-buffer\n");
    }

    #[test]
    fn editor_clears_after_run_command() {
        let mut app = App::new(4, 16).expect("app");
        let mut pty = MockPty::default();

        app.insert_editor_input("ls -la");
        app.run_editor_command(&mut pty, false).expect("run");

        assert_eq!(app.editor_snapshot(), "", "editor must be empty after execution");
    }

    #[test]
    fn editor_cursor_offset_tracks_inserts() {
        let mut app = App::new(4, 16).expect("app");
        assert_eq!(app.editor_cursor_offset(), 0);
        app.insert_editor_input("abc");
        assert_eq!(app.editor_cursor_offset(), 3);
        app.editor_backspace();
        assert_eq!(app.editor_cursor_offset(), 2);
    }

    #[test]
    fn runs_editor_selection_command_to_pty() {
        let mut app = App::new(4, 32).expect("app");
        let mut pty = MockPty::default();

        app.insert_editor_input("echo selected words");
        app.set_editor_cursor(5, false);
        app.set_editor_cursor(13, true);
        let sent = app.run_editor_command(&mut pty, true).expect("run selection");

        assert!(sent);
        assert_eq!(pty.input_log(), b"selected\n");
    }

    #[test]
    fn runtime_routes_events() {
        let mut app = App::new(4, 16).expect("app");
        let rt = AppRuntime::new(RuntimeConfig::default());
        let tx = rt.sender();

        tx.send(AppEvent::PtyOutput(b"evt\n".to_vec())).expect("send");
        assert!(rt.step(&mut app));
        assert!(app.terminal_snapshot().contains("evt"));

        tx.send(AppEvent::Shutdown).expect("send");
        assert!(!rt.step(&mut app));
    }
}
