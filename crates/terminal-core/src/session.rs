use terminal_ansi::{Action, Parser};
use terminal_screen::{DamageRegion, Screen, ScreenSnapshot, StyledChars};

use crate::error::TerminalError;

#[derive(Debug)]
pub struct TerminalSession {
    parser: Parser,
    screen: Screen,
    last_exit_code: Option<i32>,
    mouse_mode: u16,
    bracketed_paste: bool,
    pending_responses: Vec<String>,
    cursor_shape: u16,
    window_title: Option<String>,
    bell_pending: bool,
    application_cursor_keys: bool,
}

impl TerminalSession {
    pub fn new(rows: usize, cols: usize) -> Result<Self, TerminalError> {
        if rows == 0 || cols == 0 {
            return Err(TerminalError::InvalidSize { rows, cols });
        }

        Ok(Self {
            parser: Parser::new(),
            screen: Screen::new(rows, cols),
            last_exit_code: None,
            mouse_mode: 0,
            bracketed_paste: false,
            pending_responses: Vec::new(),
            cursor_shape: 0,
            window_title: None,
            bell_pending: false,
            application_cursor_keys: false,
        })
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        let actions = self.parser.advance(bytes);
        for action in actions {
            match action {
                Action::Print(ch) => self.screen.put_char(ch),
                Action::Linefeed => self.screen.linefeed(),
                Action::CarriageReturn => self.screen.carriage_return(),
                Action::Backspace => self.screen.backspace(),
                Action::HorizontalTab => self.screen.horizontal_tab(),
                Action::CursorUp(n) => self.screen.cursor_up(n),
                Action::CursorDown(n) => self.screen.cursor_down(n),
                Action::CursorForward(n) => self.screen.cursor_forward(n),
                Action::CursorBackward(n) => self.screen.cursor_backward(n),
                Action::CursorPosition { row, col } => self.screen.cursor_position(row, col),
                Action::SaveCursor => self.screen.save_cursor(),
                Action::RestoreCursor => self.screen.restore_cursor(),
                Action::SetScrollRegion { top, bottom } => self.screen.set_scroll_region(top, bottom),
                Action::InsertChars(n) => self.screen.insert_chars(n),
                Action::DeleteChars(n) => self.screen.delete_chars(n),
                Action::InsertLines(n) => self.screen.insert_lines(n),
                Action::DeleteLines(n) => self.screen.delete_lines(n),
                Action::EraseInDisplay(mode) => self.screen.erase_in_display(mode),
                Action::EraseInLine(mode) => self.screen.erase_in_line(mode),
                Action::SetGraphicsRendition(params) => self.screen.set_sgr(&params),
                Action::DecPrivateModeSet(mode) => {
                    match mode {
                        1 => self.application_cursor_keys = true,
                        1049 => self.screen.set_alternate_screen(true),
                        1000 | 1002 | 1003 | 1006 => self.mouse_mode = mode,
                        2004 => self.bracketed_paste = true,
                        _ => {}
                    }
                }
                Action::DecPrivateModeReset(mode) => {
                    match mode {
                        1 => self.application_cursor_keys = false,
                        1049 => self.screen.set_alternate_screen(false),
                        1000 | 1002 | 1003 | 1006 if self.mouse_mode == mode => {
                            self.mouse_mode = 0;
                        }
                        2004 => self.bracketed_paste = false,
                        _ => {}
                    }
                }
                Action::Osc(s) => {
                    // OSC 133;D;N — shell integration exit-code report.
                    if let Some(rest) = s.strip_prefix("133;D;")
                        && let Ok(code) = rest.parse::<i32>() {
                        self.last_exit_code = Some(code);
                    } else if let Some(title) = s.strip_prefix("0;").or_else(|| s.strip_prefix("2;")) {
                        self.window_title = Some(title.to_owned());
                    }
                }
                Action::Bell => self.bell_pending = true,
                Action::DeviceStatusReport => {
                    let row = self.screen.cursor_row() + 1;
                    let col = self.screen.cursor_col() + 1;
                    self.pending_responses.push(format!("\x1b[{row};{col}R"));
                }
                Action::SetCursorShape(n) => self.cursor_shape = n,
            }
        }
    }

    pub fn snapshot_text(&self) -> String {
        self.screen.dump_text()
    }

    pub fn snapshot_ansi(&self) -> String {
        self.screen.dump_ansi()
    }

    /// Returns per-character styled data for the visible grid.
    /// `None` fg/bg means the cell uses the renderer's default color.
    /// Matches the character layout of `snapshot_text()`.
    pub fn snapshot_styled(&self) -> StyledChars {
        self.screen.dump_styled()
    }

    /// Like `snapshot_styled` but scrolled back by `scroll_offset` rows.
    pub fn snapshot_styled_at_offset(
        &self,
        scroll_offset: usize,
    ) -> StyledChars {
        self.screen.dump_styled_at_offset(scroll_offset)
    }

    /// Like `snapshot_styled_at_offset` but overrides ANSI indexed colors 0-15
    /// using `palette` when provided.
    pub fn snapshot_styled_at_offset_with_palette(
        &self,
        scroll_offset: usize,
        palette: Option<&[[f32; 3]; 16]>,
    ) -> StyledChars {
        self.screen.dump_styled_at_offset_with_palette(scroll_offset, palette)
    }

    pub fn scrollback_len(&self) -> usize {
        self.screen.scrollback_len()
    }

    /// Resize the terminal grid to the given dimensions.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        if rows > 0 && cols > 0 {
            self.screen.resize(rows, cols);
        }
    }

    pub fn snapshot(&self) -> ScreenSnapshot {
        self.screen.snapshot()
    }

    pub fn take_damage(&mut self) -> DamageRegion {
        self.screen.take_damage()
    }

    /// Consumes and returns the exit code reported by the most recent OSC 133;D
    /// shell-integration sequence, or `None` if no new code has arrived.
    pub fn take_last_exit_code(&mut self) -> Option<i32> {
        self.last_exit_code.take()
    }

    /// Active mouse reporting mode (0 = off, 1000/1002/1003/1006 = various protocols).
    pub fn mouse_mode(&self) -> u16 {
        self.mouse_mode
    }

    /// Whether bracketed paste mode (DEC 2004) is currently active.
    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    /// Drains and returns any pending responses (e.g. cursor-position reports)
    /// that should be written back to the PTY.
    pub fn drain_pending_responses(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_responses)
    }

    /// Current cursor shape as set by DECSCUSR.
    /// 0/1 = block, 3/4 = underline, 5/6 = bar.
    pub fn cursor_shape(&self) -> u16 {
        self.cursor_shape
    }

    /// Window title set by OSC 0 or OSC 2, if any.
    pub fn window_title(&self) -> Option<&str> {
        self.window_title.as_deref()
    }

    /// Returns `true` and clears the flag if a BEL was received since the last call.
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.bell_pending)
    }

    /// Returns the current terminal cursor position as `(row, col)`, 0-based.
    pub fn cursor_pos(&self) -> (usize, usize) {
        (self.screen.cursor_row(), self.screen.cursor_col())
    }

    /// Returns whether the terminal is currently using the alternate screen.
    pub fn is_alternate_screen(&self) -> bool {
        self.screen.is_alternate_screen()
    }

    /// Returns whether application cursor keys mode (DECCKM, DEC private mode 1) is active.
    pub fn application_cursor_keys(&self) -> bool {
        self.application_cursor_keys
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalSession;

    #[test]
    fn applies_ansi_actions_to_grid() {
        let mut session = TerminalSession::new(3, 12).expect("session");
        session.feed(b"hello\n\rworld");

        let snapshot = session.snapshot_text();
        assert!(snapshot.contains("hello"));
        assert!(snapshot.contains("world"));
    }

    #[test]
    fn applies_cursor_and_erase_sequences() {
        let mut session = TerminalSession::new(2, 8).expect("session");
        session.feed(b"hello");
        session.feed(b"\x1b[1;1H\x1b[2K");

        let snapshot = session.snapshot_text();
        assert!(!snapshot.contains("hello"));
    }

    #[test]
    fn switches_to_alternate_buffer() {
        let mut session = TerminalSession::new(2, 8).expect("session");
        session.feed(b"main");
        session.feed(b"\x1b[?1049h");
        session.feed(b"alt");
        assert!(session.snapshot_text().contains("alt"));

        session.feed(b"\x1b[?1049l");
        assert!(session.snapshot_text().contains("main"));
    }

    #[test]
    fn alternate_screen_accessor_toggles() {
        let mut session = TerminalSession::new(2, 8).expect("session");
        assert!(!session.is_alternate_screen());
        session.feed(b"\x1b[?1049h");
        assert!(session.is_alternate_screen());
        session.feed(b"\x1b[?1049l");
        assert!(!session.is_alternate_screen());
    }

    #[test]
    fn exposes_damage_tracking() {
        let mut session = TerminalSession::new(2, 8).expect("session");
        let d0 = session.take_damage();
        assert!(d0.full_redraw);

        session.feed(b"x");
        let d1 = session.take_damage();
        assert!(!d1.dirty_rows.is_empty());
    }

    #[test]
    fn bell_sets_and_clears_flag() {
        let mut session = TerminalSession::new(2, 8).expect("session");
        assert!(!session.take_bell(), "bell should start as false");
        session.feed(b"\x07");
        assert!(session.take_bell(), "bell should be true after BEL byte");
        assert!(!session.take_bell(), "take_bell should clear the flag");
    }

    #[test]
    fn dsr_response_contains_cursor_position() {
        let mut session = TerminalSession::new(5, 20).expect("session");
        // Cursor is at top-left (1;1) after a fresh session.
        session.feed(b"\x1b[6n");
        let responses = session.drain_pending_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0], "\x1b[1;1R");
    }

    #[test]
    fn dsr_response_reflects_moved_cursor() {
        let mut session = TerminalSession::new(5, 20).expect("session");
        // Move cursor to row 3, col 5 (ESC[3;5H) then query.
        session.feed(b"\x1b[3;5H\x1b[6n");
        let responses = session.drain_pending_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0], "\x1b[3;5R");
    }

    #[test]
    fn bracketed_paste_toggle() {
        let mut session = TerminalSession::new(2, 8).expect("session");
        assert!(!session.bracketed_paste());
        session.feed(b"\x1b[?2004h");
        assert!(session.bracketed_paste());
        session.feed(b"\x1b[?2004l");
        assert!(!session.bracketed_paste());
    }

    #[test]
    fn cursor_shape_sequence() {
        let mut session = TerminalSession::new(2, 8).expect("session");
        assert_eq!(session.cursor_shape(), 0);
        session.feed(b"\x1b[4 q"); // steady underline
        assert_eq!(session.cursor_shape(), 4);
        session.feed(b"\x1b[6 q"); // steady bar
        assert_eq!(session.cursor_shape(), 6);
    }

    #[test]
    fn window_title_from_osc() {
        let mut session = TerminalSession::new(2, 8).expect("session");
        assert!(session.window_title().is_none());
        // OSC 0 ; title BEL
        session.feed(b"\x1b]0;My Title\x07");
        assert_eq!(session.window_title(), Some("My Title"));
        // OSC 2 ; title ST
        session.feed(b"\x1b]2;Other\x1b\\");
        assert_eq!(session.window_title(), Some("Other"));
    }

    #[test]
    fn mouse_mode_toggle() {
        let mut session = TerminalSession::new(2, 8).expect("session");
        assert_eq!(session.mouse_mode(), 0);
        session.feed(b"\x1b[?1000h");
        assert_eq!(session.mouse_mode(), 1000);
        session.feed(b"\x1b[?1000l");
        assert_eq!(session.mouse_mode(), 0);
        session.feed(b"\x1b[?1006h");
        assert_eq!(session.mouse_mode(), 1006);
        session.feed(b"\x1b[?1006l");
        assert_eq!(session.mouse_mode(), 0);
    }
}
