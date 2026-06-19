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
    fn cursor_next_line(&mut self, n: u16);
    fn cursor_previous_line(&mut self, n: u16);
    fn cursor_horizontal_absolute(&mut self, col: u16);
    fn cursor_vertical_absolute(&mut self, row: u16);
    fn cursor_position(&mut self, row: u16, col: u16);
    fn reverse_index(&mut self);
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
    /// Activate or deactivate the current OSC 8 hyperlink.
    /// `None` or empty URI → deactivate. `Some(uri)` → activate.
    fn set_active_hyperlink(&mut self, uri: Option<&str>);
    /// Resolve a hyperlink ID to its URI, or `None` if ID is 0 / unknown.
    fn hyperlink_uri(&self, id: u16) -> Option<&str>;
    /// Collect all hyperlink spans in the visible viewport at the given scroll
    /// offset. Returns `(vis_row, col_start, col_end_exclusive, id)` tuples.
    fn dump_hyperlink_spans(&self, scroll_offset: usize) -> Vec<(usize, usize, usize, u16)>;
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

    fn cursor_next_line(&mut self, n: u16) {
        Screen::cursor_next_line(self, n)
    }

    fn cursor_previous_line(&mut self, n: u16) {
        Screen::cursor_previous_line(self, n)
    }

    fn cursor_horizontal_absolute(&mut self, col: u16) {
        Screen::cursor_horizontal_absolute(self, col)
    }

    fn cursor_vertical_absolute(&mut self, row: u16) {
        Screen::cursor_vertical_absolute(self, row)
    }

    fn cursor_position(&mut self, row: u16, col: u16) {
        Screen::cursor_position(self, row, col)
    }

    fn reverse_index(&mut self) {
        Screen::reverse_index(self)
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

    fn set_active_hyperlink(&mut self, uri: Option<&str>) {
        Screen::set_active_hyperlink(self, uri)
    }

    fn hyperlink_uri(&self, id: u16) -> Option<&str> {
        Screen::hyperlink_uri(self, id)
    }

    fn dump_hyperlink_spans(&self, scroll_offset: usize) -> Vec<(usize, usize, usize, u16)> {
        Screen::dump_hyperlink_spans(self, scroll_offset)
    }
}

/// Generic terminal session: pairs a [`TerminalParser`] with a [`TerminalDisplay`].
///
/// Semantic zone tracking a single shell command lifecycle from prompt
/// display through execution and exit.
///
/// Row numbers are absolute (scrollback_len + grid_row at the time of the
/// event) so they stay valid as lines are pushed into scrollback.
#[derive(Debug, Clone)]
pub struct CommandZone {
    /// Absolute row where the prompt started (`OSC 133;A`).
    pub prompt_start_row: usize,
    /// Absolute row where the user finished typing and pressed Enter
    /// (`OSC 133;B`).  `None` if the B marker has not been received yet.
    pub command_start_row: Option<usize>,
    /// Absolute row where command output began (`OSC 133;C`).
    /// `None` if the C marker has not been received yet.
    pub output_start_row: Option<usize>,
    /// Exit code reported by the shell (`OSC 133;D;N`).
    pub exit_code: Option<i32>,
}

/// Owns the bytes-to-actions decoder and the screen model and threads parsed
/// actions through to it. Instantiating with custom `P` / `D` parameters lets
/// tests substitute fake parsers or screens.
#[derive(Debug)]
pub struct GenericTerminalSession<P = Parser, D = Screen> {
    parser: P,
    screen: D,
    last_exit_code: Option<i32>,
    mouse_tracking: u16,
    mouse_sgr: bool,
    bracketed_paste: bool,
    pending_responses: Vec<String>,
    cursor_shape: u16,
    window_title: Option<String>,
    bell_pending: bool,
    application_cursor_keys: bool,
    /// Finished command zones (prompt_start → exit) kept in arrival order.
    command_zones: Vec<CommandZone>,
    /// The in-progress zone opened by the most recent `OSC 133;A` and not yet
    /// closed by `OSC 133;D`.
    current_zone: Option<CommandZone>,
    /// Working directory reported by the shell via OSC 7 (`file://host/path`).
    /// Falls back to OS-level CWD inspection when `None`.
    cwd: Option<std::path::PathBuf>,
    /// Kitty keyboard protocol flags stack. Top of stack (last element) is the
    /// active flags word. Each `\x1b[=<flags>u` pushes; `\x1b[<n>u` pops n.
    kitty_keyboard_flags: Vec<u32>,
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
            mouse_tracking: 0,
            mouse_sgr: false,
            bracketed_paste: false,
            pending_responses: Vec::new(),
            cursor_shape: 0,
            window_title: None,
            bell_pending: false,
            application_cursor_keys: false,
            command_zones: Vec::new(),
            current_zone: None,
            cwd: None,
            kitty_keyboard_flags: Vec::new(),
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
            mouse_tracking: 0,
            mouse_sgr: false,
            bracketed_paste: false,
            pending_responses: Vec::new(),
            cursor_shape: 0,
            window_title: None,
            bell_pending: false,
            application_cursor_keys: false,
            command_zones: Vec::new(),
            current_zone: None,
            cwd: None,
            kitty_keyboard_flags: Vec::new(),
        }
    }

    fn abs_cursor_row(&self) -> usize {
        self.screen
            .scrollback_len()
            .saturating_add(self.screen.cursor_row())
    }

    /// Handle `OSC 133;A` — prompt start.
    /// Opens a new [`CommandZone`], preserving any existing incomplete zone so
    /// all prompt positions remain available for navigation.
    fn on_osc133_a(&mut self) {
        let abs_row = self.abs_cursor_row();
        // If there is already an in-progress zone (e.g. user pressed Enter
        // without running a command, or this is the very first prompt), push
        // it into the finished list so its prompt row is preserved.
        if let Some(zone) = self.current_zone.take() {
            self.command_zones.push(zone);
            if self.command_zones.len() > 500 {
                self.command_zones.remove(0);
            }
        }
        self.current_zone = Some(CommandZone {
            prompt_start_row: abs_row,
            command_start_row: None,
            output_start_row: None,
            exit_code: None,
        });
    }

    /// Handle `OSC 133;B` — end of prompt / user pressed Enter.
    fn on_osc133_b(&mut self) {
        let abs_row = self.abs_cursor_row();
        if let Some(zone) = &mut self.current_zone {
            zone.command_start_row = Some(abs_row);
        }
    }

    /// Handle `OSC 133;C` — command output is about to start.
    fn on_osc133_c(&mut self) {
        let abs_row = self.abs_cursor_row();
        if let Some(zone) = &mut self.current_zone {
            zone.output_start_row = Some(abs_row);
        }
    }

    /// Handle `OSC 133;D;N` — command finished with exit code `code`.
    fn on_osc133_d(&mut self, code: i32) {
        self.last_exit_code = Some(code);
        if let Some(mut zone) = self.current_zone.take() {
            zone.exit_code = Some(code);
            self.command_zones.push(zone);
            // Cap history to avoid unbounded growth in long-running sessions.
            if self.command_zones.len() > 500 {
                self.command_zones.remove(0);
            }
        }
    }

    #[allow(clippy::too_many_lines)]
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
                Action::CursorNextLine(n) => self.screen.cursor_next_line(n),
                Action::CursorPreviousLine(n) => self.screen.cursor_previous_line(n),
                Action::CursorHorizontalAbsolute(col) => {
                    self.screen.cursor_horizontal_absolute(col)
                }
                Action::CursorVerticalAbsolute(row) => self.screen.cursor_vertical_absolute(row),
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
                    1000 | 1002 | 1003 => self.mouse_tracking = mode,
                    1006 => self.mouse_sgr = true,
                    2004 => self.bracketed_paste = true,
                    _ => {}
                },
                Action::DecPrivateModeReset(mode) => match mode {
                    1 => self.application_cursor_keys = false,
                    1049 => self.screen.set_alternate_screen(false),
                    1000 | 1002 | 1003 if self.mouse_tracking == mode => {
                        self.mouse_tracking = 0;
                    }
                    1006 => self.mouse_sgr = false,
                    2004 => self.bracketed_paste = false,
                    _ => {}
                },
                Action::Osc(s) => {
                    if s == "133;A" {
                        self.on_osc133_a();
                    } else if s == "133;B" {
                        self.on_osc133_b();
                    } else if s == "133;C" {
                        self.on_osc133_c();
                    } else if let Some(rest) = s.strip_prefix("133;D;")
                        && let Ok(code) = rest.parse::<i32>()
                    {
                        self.on_osc133_d(code);
                    } else if let Some(title) =
                        s.strip_prefix("0;").or_else(|| s.strip_prefix("2;"))
                    {
                        self.window_title = Some(title.to_owned());
                    } else if let Some(uri) = s.strip_prefix("7;") {
                        // OSC 7 — shell reports its working directory as
                        // `file://[host]/path`. Strip the authority component
                        // (everything up to the third slash after "file://").
                        if let Some(path_str) = uri
                            .strip_prefix("file://")
                            .and_then(|rest| rest.find('/').map(|i| &rest[i..]))
                            .or_else(|| uri.strip_prefix("file:///"))
                        {
                            // URL-decode percent-encoded characters (e.g. %20 → ' ')
                            let decoded = percent_decode(path_str);
                            self.cwd = Some(std::path::PathBuf::from(decoded));
                        }
                    }
                }
                Action::SetHyperlink(uri_opt) => {
                    self.screen.set_active_hyperlink(uri_opt.as_deref());
                }
                Action::Bell => self.bell_pending = true,
                Action::DeviceStatusReport => {
                    let row = self.screen.cursor_row() + 1;
                    let col = self.screen.cursor_col() + 1;
                    self.pending_responses.push(format!("\x1b[{row};{col}R"));
                }
                Action::SetCursorShape(n) => self.cursor_shape = n,
                Action::KittyKeyboardPush(flags) => {
                    self.kitty_keyboard_flags.push(flags);
                }
                Action::KittyKeyboardPop(n) => {
                    let len = self.kitty_keyboard_flags.len();
                    self.kitty_keyboard_flags
                        .truncate(len.saturating_sub(n as usize));
                }
                Action::KittyKeyboardQuery => {
                    let flags = self.kitty_keyboard_flags.last().copied().unwrap_or(0);
                    self.pending_responses.push(format!("\x1b[?{flags}u"));
                }
                Action::ReverseIndex => self.screen.reverse_index(),
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

    /// Active mouse tracking mode (0 = off, 1000/1002/1003 = reporting level).
    /// Use `mouse_sgr` to check whether SGR (1006) extended encoding is active.
    pub fn mouse_mode(&self) -> u16 {
        self.mouse_tracking
    }

    /// Whether SGR extended mouse encoding (DEC 1006) is active.
    pub fn mouse_sgr(&self) -> bool {
        self.mouse_sgr
    }

    /// Whether bracketed paste mode (DEC 2004) is currently active.
    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    /// Current kitty keyboard protocol flags (top of stack, 0 = disabled).
    pub fn kitty_keyboard_flags(&self) -> u32 {
        self.kitty_keyboard_flags.last().copied().unwrap_or(0)
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
    /// Derived from `command_zones` for backwards compatibility.
    pub fn prompt_marks(&self) -> Vec<usize> {
        let mut marks: Vec<usize> = self
            .command_zones
            .iter()
            .map(|z| z.prompt_start_row)
            .collect();
        if let Some(current) = &self.current_zone {
            marks.push(current.prompt_start_row);
        }
        marks
    }

    /// All completed command zones, oldest first.
    pub fn command_zones(&self) -> &[CommandZone] {
        &self.command_zones
    }

    /// The in-progress zone (prompt seen but exit code not yet received), if any.
    pub fn current_zone(&self) -> Option<&CommandZone> {
        self.current_zone.as_ref()
    }

    /// Working directory last reported by the shell via OSC 7.
    /// Returns `None` if the shell has not yet sent an OSC 7 sequence.
    pub fn osc7_cwd(&self) -> Option<&std::path::Path> {
        self.cwd.as_deref()
    }

    /// Collect all OSC 8 hyperlink spans visible at the given scroll offset.
    /// Delegates to the underlying screen model; see
    /// [`terminal_screen::Screen::dump_hyperlink_spans`] for the return format.
    pub fn hyperlink_spans(&self, scroll_offset: usize) -> Vec<(usize, usize, usize, u16)> {
        self.screen.dump_hyperlink_spans(scroll_offset)
    }

    /// Resolve a hyperlink ID to its URI string. `0` always returns `None`.
    pub fn hyperlink_uri(&self, id: u16) -> Option<&str> {
        self.screen.hyperlink_uri(id)
    }
}

/// Decode percent-encoded characters in a URI path component (e.g. `%20` → `' '`).
/// Only handles ASCII-range encodings; non-ASCII byte sequences are passed through.
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            )
        {
            out.push((hi * 16 + lo) as u8 as char);
            i += 3;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
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

        fn cursor_next_line(&mut self, _n: u16) {}

        fn cursor_previous_line(&mut self, _n: u16) {}

        fn cursor_horizontal_absolute(&mut self, _col: u16) {}

        fn cursor_vertical_absolute(&mut self, _row: u16) {}

        fn cursor_position(&mut self, _row: u16, _col: u16) {}

        fn reverse_index(&mut self) {}

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

        fn set_active_hyperlink(&mut self, _uri: Option<&str>) {}

        fn hyperlink_uri(&self, _id: u16) -> Option<&str> {
            None
        }

        fn dump_hyperlink_spans(&self, _scroll_offset: usize) -> Vec<(usize, usize, usize, u16)> {
            Vec::new()
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
    fn progress_updates_using_cursor_horizontal_absolute_rewrite_one_line() {
        let mut session = make_session(3, 20);

        session.feed(b"Downloading 10%\x1b[1G\x1b[2K");
        session.feed(b"Downloading 50%\x1b[1G\x1b[2K");
        session.feed(b"Downloading 100%");

        let snapshot = session.snapshot_text();
        assert!(snapshot.contains("Downloading 100%"));
        assert!(!snapshot.contains("Downloading 10%"));
        assert!(!snapshot.contains("Downloading 50%"));
        assert_eq!(session.scrollback_len(), 0);
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
        assert!(session.mouse_sgr());
        session.feed(b"\x1b[?1006l");
        assert!(!session.mouse_sgr());
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
