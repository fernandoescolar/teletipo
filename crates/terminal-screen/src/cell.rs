use crate::color::AnsiColor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellStyle {
    pub(crate) fg: AnsiColor,
    pub(crate) bg: AnsiColor,
    pub(crate) bold: bool,
    pub(crate) dim: bool,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
    pub(crate) reverse: bool,
    pub(crate) strikethrough: bool,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: AnsiColor::Default,
            bg: AnsiColor::Default,
            bold: false,
            dim: false,
            italic: false,
            underline: false,
            reverse: false,
            strikethrough: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub(crate) ch: char,
    pub(crate) style: CellStyle,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: CellStyle::default(),
        }
    }
}
