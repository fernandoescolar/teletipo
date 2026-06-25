/// A semantic event decoded by the ANSI parser.
///
/// `Action` is the intermediate representation between raw PTY bytes and the
/// terminal screen model; the parser emits a stream of these and the screen
/// applies them to its grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Print(char),
    Linefeed,
    CarriageReturn,
    Backspace,
    HorizontalTab,
    CursorUp(u16),
    CursorDown(u16),
    CursorForward(u16),
    CursorBackward(u16),
    CursorNextLine(u16),
    CursorPreviousLine(u16),
    CursorHorizontalAbsolute(u16),
    CursorVerticalAbsolute(u16),
    CursorPosition {
        row: u16,
        col: u16,
    },
    SaveCursor,
    RestoreCursor,
    SetScrollRegion {
        top: u16,
        bottom: u16,
    },
    InsertChars(u16),
    DeleteChars(u16),
    InsertLines(u16),
    DeleteLines(u16),
    EraseInDisplay(u16),
    EraseInLine(u16),
    SetGraphicsRendition(Vec<u16>),
    DecPrivateModeSet(u16),
    DecPrivateModeReset(u16),
    /// Raw OSC (Operating System Command) sequence payload, e.g. "133;D;0"
    /// for shell integration exit-code reporting.
    Osc(String),
    /// OSC 8 hyperlink: `Some(uri)` activates the link, `None` ends it.
    ///
    /// Emitted from `\e]8;[params];[uri]\a` or `\e]8;[params];[uri]\e\\`.
    /// The optional `params` field is deliberately ignored (it is rarely used
    /// and carries application-specific metadata we don't need).
    SetHyperlink(Option<String>),
    /// BEL character (0x07).
    Bell,
    /// Request cursor position report (`\x1b[6n`).
    DeviceStatusReport,
    /// Request primary device attributes (`\x1b[c` or `\x1b[0c`).
    PrimaryDeviceAttributes,
    /// DECSCUSR — set cursor shape.
    /// 0/1 = blinking block, 2 = steady block, 3 = blinking underline,
    /// 4 = steady underline, 5 = blinking bar, 6 = steady bar.
    SetCursorShape(u16),
    /// Kitty keyboard protocol: push flags onto the stack (`\x1b[=<flags>u`).
    KittyKeyboardPush(u32),
    /// Kitty keyboard protocol: pop N entries from the stack (`\x1b[<n>u`).
    KittyKeyboardPop(u32),
    /// Kitty keyboard protocol: query current flags (`\x1b[?u`).
    KittyKeyboardQuery,
    /// ESC M — reverse index (RI): move cursor up one line, scroll down if at top margin.
    ReverseIndex,
    /// OSC 1337;ReportCellSize — the application requests the terminal's cell
    /// pixel dimensions. The terminal should respond with
    /// `\x1b]1337;ReportCellSize=HxW\a` where H and W are the cell height and
    /// width in physical pixels.
    ReportCellSize,
    /// OSC 1337;CurrentDir=`path` — iTerm2 working-directory report.
    /// Semantically equivalent to OSC 7; both update the shell CWD label.
    SetCwdIterm(String),
}
