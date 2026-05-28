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
    /// BEL character (0x07).
    Bell,
    /// Request cursor position report (`\x1b[6n`).
    DeviceStatusReport,
    /// DECSCUSR — set cursor shape.
    /// 0/1 = blinking block, 2 = steady block, 3 = blinking underline,
    /// 4 = steady underline, 5 = blinking bar, 6 = steady bar.
    SetCursorShape(u16),
}
