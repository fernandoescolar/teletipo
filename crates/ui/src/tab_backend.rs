/// Abstraction over the terminal/editor backend held inside each `TabPane`.
///
/// Implementing this trait allows `ui` to drive tabs without depending on
/// concrete `app-orchestrator` or `terminal-pty` types.
pub trait TabBackend {
    // ── Editor ─────────────────────────────────────────────────────────────
    fn insert_text(&mut self, text: &str);
    fn backspace(&mut self);
    fn delete_forward(&mut self);
    fn move_cursor_left(&mut self, extend: bool);
    fn move_cursor_right(&mut self, extend: bool);
    fn set_cursor(&mut self, offset: usize, extend: bool);
    fn undo(&mut self);
    fn redo(&mut self);
    fn clear_editor(&mut self);
    fn editor_snapshot(&self) -> String;
    fn record_history(&mut self, cmd: &str);
    /// Execute the current editor content (or selection when `pipe_selection` is true).
    fn run_command(&mut self, pipe_selection: bool);

    // ── Terminal / PTY ──────────────────────────────────────────────────────
    /// Send raw bytes to the PTY (e.g. keyboard input or OSC responses).
    fn send_bytes(&mut self, bytes: &[u8]);
    /// Read one chunk of PTY output into the terminal screen.
    /// Returns the number of bytes consumed, or `None` if no PTY is attached.
    fn pump(&mut self) -> Option<usize>;
    /// Drain any pending PTY responses queued by the terminal state machine.
    fn drain_responses(&mut self) -> Vec<String>;
    fn take_bell(&mut self) -> bool;
    fn is_alternate_screen(&self) -> bool;
    /// Returns `true` if the backing process has exited.
    fn is_dead(&mut self) -> bool;
    fn resize(&mut self, rows: u16, cols: u16);
    fn has_pty(&self) -> bool;
}
