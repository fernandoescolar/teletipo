use crate::color::AnsiColor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellStyle {
    pub fg: AnsiColor,
    pub bg: AnsiColor,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: AnsiColor::Default,
            bg: AnsiColor::Default,
            bold: false,
            italic: false,
            underline: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: CellStyle,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: CellStyle::default(),
        }
    }
}
