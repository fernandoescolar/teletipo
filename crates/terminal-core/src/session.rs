use std::sync::Arc;

use terminal_ansi::{Action, Parser};
use terminal_screen::{DamageRegion, Screen, ScreenSnapshot, StyledChars};

use crate::error::TerminalError;

/// Abstracts the byte-to-action parser used by a terminal session.
pub trait TerminalParser {
    fn advance(&mut self, bytes: &[u8]) -> Vec<Action>;
}

impl TerminalParser for Parser {
    fn advance(&mut self, bytes: &[u8]) -> Vec<Action> {
        Parser::advance(self, bytes)
    }
}

/// Abstracts the screen/grid backend used by a terminal session.
pub trait TerminalDisplay {
    fn put_char(&mut self, ch: char);
    fn linefeed(&mut self);
    fn carriage_return(&mut self);
    fn backspace(&mut self);
    fn horizontal_tab(&mut self);
    fn cursor_up(&mut self, n: u16);
    fn cursor_down(&mut self, n: u16);
    fn cursor_forward(&mut self, n: u16);
    fn cursor_backward(&mut self, n: u16);
    fn cursor_position(&mut self, row: u16, col: u16);
    fn save_cursor(&mut self);
    fn restore_cursor(&mut self);
    fn set_scroll_region(&mut self, top: u16, bottom: u16);
    fn insert_chars(&mut self, n: u16);
    fn delete_chars(&mut self, n: u16);
    fn insert_lines(&mut self, n: u16);
    fn delete_lines(&mut self, n: u16);
    fn erase_in_display(&mut self, mode: u16);
    fn erase_in_line(&mut self, mode: u16);
    fn set_sgr(&mut self, params: &[u16]);
    fn set_alternate_screen(&mut self, enabled: bool);
    fn cursor_row(&self) -> usize;
    fn cursor_col(&self) -> usize;
    fn dump_text(&self) -> String;
    fn dump_text_with_scrollback(&self) -> String;
    fn dump_ansi(&self) -> Arc<String>;
    fn dump_styled(&self) -> StyledChars;
    fn dump_styled_at_offset(&self, scroll_offset: usize) -> StyledChars;
    fn dump_styled_at_offset_with_palette(
        &self,
        scroll_offset: usize,
        palette: Option<&[[f32; 3]; 16]>,
    ) -> StyledChars;
    fn scrollback_len(&self) -> usize;
    fn version(&self) -> u64;
    fn resize(&mut self, rows: usize, cols: usize);
    fn snapshot(&self) -> ScreenSnapshot;
    fn take_damage(&mut self) -> DamageRegion;
    fn is_alternate_screen(&self) -> bool;
}

impl TerminalDisplay for Screen {
    fn put_char(&mut self, ch: char) {
        Screen::put_char(self, ch)
    }

    fn linefeed(&mut self) {
        Screen::linefeed(self)
    }

    fn carriage_return(&mut self) {
        Screen::carriage_return(self)
    }

    fn backspace(&mut self) {
        Screen::backspace(self)
    }

    fn horizontal_tab(&mut self) {
        Screen::horizontal_tab(self)
    }

    fn cursor_up(&mut self, n: u16) {
        Screen::cursor_up(self, n)
    }

    fn cursor_down(&mut self, n: u16) {
        Screen::cursor_down(self, n)
    }

    fn cursor_forward(&mut self, n: u16) {
        Screen::cursor_forward(self, n)
    }

    fn cursor_backward(&mut self, n: u16) {
        Screen::cursor_backward(self, n)
    }

    fn cursor_position(&mut self, row: u16, col: u16) {
        Screen::cursor_position(self, row, col)
    }

    fn save_cursor(&mut self) {
        Screen::save_cursor(self)
    }

    fn restore_cursor(&mut self) {
        Screen::restore_cursor(self)
    }

    fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        Screen::set_scroll_region(self, top, bottom)
    }

    fn insert_chars(&mut self, n: u16) {
        Screen::insert_chars(self, n)
    }

    fn delete_chars(&mut self, n: u16) {
        Screen::delete_chars(self, n)
    }

    fn insert_lines(&mut self, n: u16) {
        Screen::insert_lines(self, n)
    }

    fn delete_lines(&mut self, n: u16) {
        Screen::delete_lines(self, n)
    }

    fn erase_in_display(&mut self, mode: u16) {
        Screen::erase_in_display(self, mode)
    }

    fn erase_in_line(&mut self, mode: u16) {
        Screen::erase_in_line(self, mode)
    }

    fn set_sgr(&mut self, params: &[u16]) {
        Screen::set_sgr(self, params)
    }

    fn set_alternate_screen(&mut self, enabled: bool) {
        Screen::set_alternate_screen(self, enabled)
    }

    fn cursor_row(&self) -> usize {
        Screen::cursor_row(self)
    }

    fn cursor_col(&self) -> usize {
        Screen::cursor_col(self)
    }

    fn dump_text(&self) -> String {
        Screen::dump_text(self)
    }

    fn dump_text_with_scrollback(&self) -> String {
        Screen::dump_text_with_scrollback(self)
    }

    fn dump_ansi(&self) -> Arc<String> {
        Screen::dump_ansi(self)
    }

    fn dump_styled(&self) -> StyledChars {
        Screen::dump_styled(self)
    }

    fn dump_styled_at_offset(&self, scroll_offset: usize) -> StyledChars {
        Screen::dump_styled_at_offset(self, scroll_offset)
    }

    fn dump_styled_at_offset_with_palette(
        &self,
        scroll_offset: usize,
        palette: Option<&[[f32; 3]; 16]>,
    ) -> StyledChars {
        Screen::dump_styled_at_offset_with_palette(self, scroll_offset, palette)
    }

    fn scrollback_len(&self) -> usize {
        Screen::scrollback_len(self)
    }

    fn version(&self) -> u64 {
        Screen::version(self)
    }

    fn resize(&mut self, rows: usize, cols: usize) {
        Screen::resize(self, rows, cols)
    }

    fn snapshot(&self) -> ScreenSnapshot {
        Screen::snapshot(self)
    }

    fn take_damage(&mut self) -> DamageRegion {
        Screen::take_damage(self)
    }

    fn is_alternate_screen(&self) -> bool {
        Screen::is_alternate_screen(self)
    }
}

/// Generic terminal session: pairs a [`TerminalParser`] with a [`TerminalDisplay`].
///
/// Owns the bytes-to-actions decoder and the screen model and threads parsed
/// actions through to it. Instantiating with custom `P` / `D` parameters lets
/// tests substitute fake parsers or screens.
#[derive(Debug)]
pub struct GenericTerminalSession<P = Parser, D = Screen> {
    parser: P,
    screen: D,
    last_exit_code: Option<i32>,
    mouse_mode: u16,
    bracketed_paste: bool,
    pending_responses: Vec<String>,
    cursor_shape: u16,
    window_title: Option<String>,
    bell_pending: bool,
    application_cursor_keys: bool,
    prompt_marks: Vec<usize>,
}

/// Default `GenericTerminalSession` specialised with the production parser
/// (`terminal_ansi::Parser`) and screen (`terminal_screen::Screen`).
pub type TerminalSession = GenericTerminalSession<Parser, Screen>;

impl GenericTerminalSession<Parser, Screen> {
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
            prompt_marks: Vec::new(),
        })
    }
}

impl<P, D> GenericTerminalSession<P, D>
where
    P: TerminalParser,
    D: TerminalDisplay,
{
    pub fn with_components(parser: P, screen: D) -> Self {
        Self {
            parser,
            screen,
            last_exit_code: None,
            mouse_mode: 0,
            bracketed_paste: false,
            pending_responses: Vec::new(),
            cursor_shape: 0,
            window_title: None,
            bell_pending: false,
            application_cursor_keys: false,
            prompt_marks: Vec::new(),
        }
    }

    fn record_prompt_mark(&mut self) {
        let abs_row = self
            .screen
            .scrollback_len()
            .saturating_add(self.screen.cursor_row());
        if self.prompt_marks.last().copied() != Some(abs_row) {
            self.prompt_marks.push(abs_row);
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        let actions = self.parser.advance(bytes);
        metrics::histogram!("parse_actions").record(actions.len() as f64);
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
                Action::SetScrollRegion { top, bottom } => {
                    self.screen.set_scroll_region(top, bottom)
                }
                Action::InsertChars(n) => self.screen.insert_chars(n),
                Action::DeleteChars(n) => self.screen.delete_chars(n),
                Action::InsertLines(n) => self.screen.insert_lines(n),
                Action::DeleteLines(n) => self.screen.delete_lines(n),
                Action::EraseInDisplay(mode) => self.screen.erase_in_display(mode),
                Action::EraseInLine(mode) => self.screen.erase_in_line(mode),
                Action::SetGraphicsRendition(params) => self.screen.set_sgr(&params),
                Action::DecPrivateModeSet(mode) => match mode {
                    1 => self.application_cursor_keys = true,
                    1049 => self.screen.set_alternate_screen(true),
                    1000 | 1002 | 1003 | 1006 => self.mouse_mode = mode,
                    2004 => self.bracketed_paste = true,
                    _ => {}
                },
                Action::DecPrivateModeReset(mode) => match mode {
                    1 => self.application_cursor_keys = false,
                    1049 => self.screen.set_alternate_screen(false),
                    1000 | 1002 | 1003 | 1006 if self.mouse_mode == mode => {
                        self.mouse_mode = 0;
                    }
                    2004 => self.bracketed_paste = false,
                    _ => {}
                },
                Action::Osc(s) => {
                    if s == "133;A" {
                        self.record_prompt_mark();
                    }
                    // OSC 133;D;N — shell integration exit-code report.
                    if let Some(rest) = s.strip_prefix("133;D;")
                        && let Ok(code) = rest.parse::<i32>()
                    {
                        self.last_exit_code = Some(code);
                    } else if let Some(title) =
                        s.strip_prefix("0;").or_else(|| s.strip_prefix("2;"))
                    {
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

    pub fn snapshot_text_with_scrollback(&self) -> String {
        self.screen.dump_text_with_scrollback()
    }

    pub fn snapshot_ansi(&self) -> Arc<String> {
        self.screen.dump_ansi()
    }

    /// Returns per-character styled data for the visible grid.
    /// `None` fg/bg means the cell uses the renderer's default color.
    /// Matches the character layout of `snapshot_text()`.
    pub fn snapshot_styled(&self) -> StyledChars {
        self.screen.dump_styled()
    }

    /// Like `snapshot_styled` but scrolled back by `scroll_offset` rows.
    pub fn snapshot_styled_at_offset(&self, scroll_offset: usize) -> StyledChars {
        self.screen.dump_styled_at_offset(scroll_offset)
    }

    /// Like `snapshot_styled_at_offset` but overrides ANSI indexed colors 0-15
    /// using `palette` when provided.
    pub fn snapshot_styled_at_offset_with_palette(
        &self,
        scroll_offset: usize,
        palette: Option<&[[f32; 3]; 16]>,
    ) -> StyledChars {
        self.screen
            .dump_styled_at_offset_with_palette(scroll_offset, palette)
    }

    pub fn scrollback_len(&self) -> usize {
        self.screen.scrollback_len()
    }

    /// Returns the current screen version counter.  This value is incremented
    /// on every write to the screen and can be compared across frames to
    /// determine whether the terminal content has changed.
    pub fn screen_version(&self) -> u64 {
        self.screen.version()
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

    /// Absolute rows of prompts reported by OSC 133 hooks.
    pub fn prompt_marks(&self) -> &[usize] {
        &self.prompt_marks
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use terminal_ansi::Action;
    use terminal_screen::{DamageRegion, ScreenSnapshot, StyledChars};

    use super::{GenericTerminalSession, TerminalDisplay, TerminalParser, TerminalSession};

    /// Construct a `TerminalSession` with the given dimensions for testing.
    /// Panics if construction fails (invalid size).
    fn make_session(rows: usize, cols: usize) -> TerminalSession {
        TerminalSession::new(rows, cols).expect("make_session: valid size")
    }

    #[derive(Default)]
    struct FakeParser;

    impl TerminalParser for FakeParser {
        fn advance(&mut self, _bytes: &[u8]) -> Vec<Action> {
            vec![
                Action::Print('x'),
                Action::DecPrivateModeSet(1049),
                Action::DecPrivateModeReset(1049),
                Action::Bell,
            ]
        }
    }

    #[derive(Default)]
    struct FakeDisplay {
        text: String,
        alternate: bool,
    }

    impl TerminalDisplay for FakeDisplay {
        fn put_char(&mut self, ch: char) {
            self.text.push(ch);
        }

        fn linefeed(&mut self) {}

        fn carriage_return(&mut self) {}

        fn backspace(&mut self) {}

        fn horizontal_tab(&mut self) {}

        fn cursor_up(&mut self, _n: u16) {}

        fn cursor_down(&mut self, _n: u16) {}

        fn cursor_forward(&mut self, _n: u16) {}

        fn cursor_backward(&mut self, _n: u16) {}

        fn cursor_position(&mut self, _row: u16, _col: u16) {}

        fn save_cursor(&mut self) {}

        fn restore_cursor(&mut self) {}

        fn set_scroll_region(&mut self, _top: u16, _bottom: u16) {}

        fn insert_chars(&mut self, _n: u16) {}

        fn delete_chars(&mut self, _n: u16) {}

        fn insert_lines(&mut self, _n: u16) {}

        fn delete_lines(&mut self, _n: u16) {}

        fn erase_in_display(&mut self, _mode: u16) {}

        fn erase_in_line(&mut self, _mode: u16) {}

        fn set_sgr(&mut self, _params: &[u16]) {}

        fn set_alternate_screen(&mut self, enabled: bool) {
            self.alternate = enabled;
        }

        fn cursor_row(&self) -> usize {
            0
        }

        fn cursor_col(&self) -> usize {
            0
        }

        fn dump_text(&self) -> String {
            self.text.clone()
        }

        fn dump_text_with_scrollback(&self) -> String {
            self.text.clone()
        }

        fn dump_ansi(&self) -> Arc<String> {
            Arc::new(self.text.clone())
        }

        fn dump_styled(&self) -> StyledChars {
            self.text.chars().map(|ch| (ch, None, None, 0)).collect()
        }

        fn dump_styled_at_offset(&self, _scroll_offset: usize) -> StyledChars {
            self.dump_styled()
        }

        fn dump_styled_at_offset_with_palette(
            &self,
            _scroll_offset: usize,
            _palette: Option<&[[f32; 3]; 16]>,
        ) -> StyledChars {
            self.dump_styled()
        }

        fn scrollback_len(&self) -> usize {
            0
        }

        fn version(&self) -> u64 {
            1
        }

        fn resize(&mut self, _rows: usize, _cols: usize) {}

        fn snapshot(&self) -> ScreenSnapshot {
            ScreenSnapshot {
                text: Arc::new(self.text.clone()),
                version: 1,
                rows: 1,
                cols: 1,
            }
        }

        fn take_damage(&mut self) -> DamageRegion {
            DamageRegion {
                full_redraw: false,
                dirty_rows: Vec::new(),
                version: 1,
            }
        }

        fn is_alternate_screen(&self) -> bool {
            self.alternate
        }
    }

    #[test]
    fn generic_session_accepts_fake_components() {
        let mut session =
            GenericTerminalSession::with_components(FakeParser, FakeDisplay::default());

        session.feed(b"hello");

        assert_eq!(session.snapshot_text(), "x");
        assert!(!session.is_alternate_screen());
        assert!(session.take_bell());
    }

    #[test]
    fn applies_ansi_actions_to_grid() {
        let mut session = make_session(3, 12);
        session.feed(b"hello\n\rworld");

        let snapshot = session.snapshot_text();
        assert!(snapshot.contains("hello"));
        assert!(snapshot.contains("world"));
    }

    #[test]
    fn applies_cursor_and_erase_sequences() {
        let mut session = make_session(2, 8);
        session.feed(b"hello");
        session.feed(b"\x1b[1;1H\x1b[2K");

        let snapshot = session.snapshot_text();
        assert!(!snapshot.contains("hello"));
    }

    #[test]
    fn switches_to_alternate_buffer() {
        let mut session = make_session(2, 8);
        session.feed(b"main");
        session.feed(b"\x1b[?1049h");
        session.feed(b"alt");
        assert!(session.snapshot_text().contains("alt"));

        session.feed(b"\x1b[?1049l");
        assert!(session.snapshot_text().contains("main"));
    }

    #[test]
    fn alternate_screen_accessor_toggles() {
        let mut session = make_session(2, 8);
        assert!(!session.is_alternate_screen());
        session.feed(b"\x1b[?1049h");
        assert!(session.is_alternate_screen());
        session.feed(b"\x1b[?1049l");
        assert!(!session.is_alternate_screen());
    }

    #[test]
    fn exposes_damage_tracking() {
        let mut session = make_session(2, 8);
        let d0 = session.take_damage();
        assert!(d0.full_redraw);

        session.feed(b"x");
        let d1 = session.take_damage();
        assert!(!d1.dirty_rows.is_empty());
    }

    #[test]
    fn bell_sets_and_clears_flag() {
        let mut session = make_session(2, 8);
        assert!(!session.take_bell(), "bell should start as false");
        session.feed(b"\x07");
        assert!(session.take_bell(), "bell should be true after BEL byte");
        assert!(!session.take_bell(), "take_bell should clear the flag");
    }

    #[test]
    fn dsr_response_contains_cursor_position() {
        let mut session = make_session(5, 20);
        // Cursor is at top-left (1;1) after a fresh session.
        session.feed(b"\x1b[6n");
        let responses = session.drain_pending_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0], "\x1b[1;1R");
    }

    #[test]
    fn dsr_response_reflects_moved_cursor() {
        let mut session = make_session(5, 20);
        // Move cursor to row 3, col 5 (ESC[3;5H) then query.
        session.feed(b"\x1b[3;5H\x1b[6n");
        let responses = session.drain_pending_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0], "\x1b[3;5R");
    }

    #[test]
    fn bracketed_paste_toggle() {
        let mut session = make_session(2, 8);
        assert!(!session.bracketed_paste());
        session.feed(b"\x1b[?2004h");
        assert!(session.bracketed_paste());
        session.feed(b"\x1b[?2004l");
        assert!(!session.bracketed_paste());
    }

    #[test]
    fn cursor_shape_sequence() {
        let mut session = make_session(2, 8);
        assert_eq!(session.cursor_shape(), 0);
        session.feed(b"\x1b[4 q"); // steady underline
        assert_eq!(session.cursor_shape(), 4);
        session.feed(b"\x1b[6 q"); // steady bar
        assert_eq!(session.cursor_shape(), 6);
    }

    #[test]
    fn window_title_from_osc() {
        let mut session = make_session(2, 8);
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
        let mut session = make_session(2, 8);
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

    #[test]
    fn osc_133_prompt_marks_are_recorded_once_per_row() {
        let mut session = make_session(3, 10);

        session.feed(b"\x1b]133;A\x07");
        session.feed(b"prompt\n");
        session.feed(b"\x1b]133;B\x07");
        session.feed(b"cmd\n");
        session.feed(b"\x1b]133;A\x07");

        assert_eq!(session.prompt_marks(), &[0, 2]);
    }
}
