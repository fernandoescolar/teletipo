use app_orchestrator::App;
use terminal_pty::PortablePtySession;
use tracing::{debug, warn};
use ui::TabBackend;

/// Concrete [`TabBackend`] implementation for `app-cli`.
///
/// Owns both the terminal/editor [`App`] and the optional PTY session,
/// fulfilling all [`TabBackend`] operations without exposing either type
/// to the `ui` crate.
pub(crate) struct TerminalBackend {
    pub app: App,
    pub pty: Option<PortablePtySession>,
}

impl TerminalBackend {
    pub(crate) fn new(app: App, pty: Option<PortablePtySession>) -> Self {
        Self { app, pty }
    }
}

impl TabBackend for TerminalBackend {
    fn insert_text(&mut self, text: &str) {
        self.app.insert_editor_input(text);
    }

    fn backspace(&mut self) {
        self.app.editor_backspace();
    }

    fn delete_forward(&mut self) {
        self.app.editor_delete_forward();
    }

    fn move_cursor_left(&mut self, extend: bool) {
        self.app.editor_move_cursor_left(extend);
    }

    fn move_cursor_right(&mut self, extend: bool) {
        self.app.editor_move_cursor_right(extend);
    }

    fn set_cursor(&mut self, offset: usize, extend: bool) {
        self.app.set_editor_cursor(offset, extend);
    }

    fn undo(&mut self) {
        self.app.editor_undo();
    }

    fn redo(&mut self) {
        self.app.editor_redo();
    }

    fn clear_editor(&mut self) {
        self.app.editor_clear();
    }

    fn editor_snapshot(&self) -> String {
        self.app.editor_snapshot()
    }

    fn record_history(&mut self, cmd: &str) {
        self.app.record_history(cmd);
    }

    fn run_command(&mut self, pipe_selection: bool) {
        if let Some(mut pty) = self.pty.take() {
            if let Err(err) = self.app.run_editor_command(&mut pty, pipe_selection) {
                warn!(error = %err, pipe_selection, "editor command failed");
            }
            self.pty = Some(pty);
        }
    }

    fn send_bytes(&mut self, bytes: &[u8]) {
        if let Some(mut pty) = self.pty.take() {
            if let Err(err) = self.app.send_pty_input(&mut pty, bytes) {
                warn!(error = %err, byte_len = bytes.len(), "failed to send bytes to pty");
            }
            self.pty = Some(pty);
        }
    }

    fn pump(&mut self) -> Option<usize> {
        let mut pty = self.pty.take()?;
        let result = match self.app.pump_pty_once(&mut pty) {
            Ok(bytes) => Some(bytes),
            Err(err) => {
                warn!(error = %err, "failed to pump pty output");
                None
            }
        };
        self.pty = Some(pty);
        result
    }

    fn drain_responses(&mut self) -> Vec<String> {
        self.app.drain_pending_responses()
    }

    fn take_bell(&mut self) -> bool {
        self.app.take_bell()
    }

    fn is_alternate_screen(&self) -> bool {
        self.app.is_alternate_screen()
    }

    fn is_dead(&mut self) -> bool {
        match self.pty.as_mut().and_then(|p| match p.try_wait() {
            Ok(status) => Some(status),
            Err(err) => {
                debug!(error = %err, "failed to query pty child status");
                None
            }
        }) {
            Some(Some(_)) => true,
            Some(None) | None => false,
        }
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        if let Some(pty) = self.pty.as_mut() {
            pty.resize(rows, cols);
        }
        self.app.resize_terminal(rows as usize, cols as usize);
    }

    fn has_pty(&self) -> bool {
        self.pty.is_some()
    }
}
