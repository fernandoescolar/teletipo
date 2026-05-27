use terminal_ansi::{Action, Parser};
use terminal_screen::{DamageRegion, Screen, ScreenSnapshot};

use crate::error::TerminalError;

#[derive(Debug)]
pub struct TerminalSession {
    parser: Parser,
    screen: Screen,
}

impl TerminalSession {
    pub fn new(rows: usize, cols: usize) -> Result<Self, TerminalError> {
        if rows == 0 || cols == 0 {
            return Err(TerminalError::InvalidSize { rows, cols });
        }

        Ok(Self {
            parser: Parser::new(),
            screen: Screen::new(rows, cols),
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
                    if mode == 1049 {
                        self.screen.set_alternate_screen(true);
                    }
                }
                Action::DecPrivateModeReset(mode) => {
                    if mode == 1049 {
                        self.screen.set_alternate_screen(false);
                    }
                }
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
    pub fn snapshot_styled(&self) -> Vec<(char, Option<[f32; 3]>, Option<[f32; 3]>)> {
        self.screen.dump_styled()
    }

    /// Like `snapshot_styled` but scrolled back by `scroll_offset` rows.
    pub fn snapshot_styled_at_offset(
        &self,
        scroll_offset: usize,
    ) -> Vec<(char, Option<[f32; 3]>, Option<[f32; 3]>)> {
        self.screen.dump_styled_at_offset(scroll_offset)
    }

    /// Like `snapshot_styled_at_offset` but overrides ANSI indexed colors 0-15
    /// using `palette` when provided.
    pub fn snapshot_styled_at_offset_with_palette(
        &self,
        scroll_offset: usize,
        palette: Option<&[[f32; 3]; 16]>,
    ) -> Vec<(char, Option<[f32; 3]>, Option<[f32; 3]>)> {
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
    fn exposes_damage_tracking() {
        let mut session = TerminalSession::new(2, 8).expect("session");
        let d0 = session.take_damage();
        assert!(d0.full_redraw);

        session.feed(b"x");
        let d1 = session.take_damage();
        assert!(!d1.dirty_rows.is_empty());
    }
}
