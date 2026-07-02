use crate::color::AnsiColor;

const BOLD: u8 = 0x01;
const DIM: u8 = 0x02;
const ITALIC: u8 = 0x04;
const UNDERLINE: u8 = 0x08;
const REVERSE: u8 = 0x10;
const STRIKETHROUGH: u8 = 0x20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellStyle {
    pub(crate) fg: AnsiColor,
    pub(crate) bg: AnsiColor,
    flags: u8,
}

impl CellStyle {
    pub(crate) fn bold(&self) -> bool {
        self.flags & BOLD != 0
    }
    pub(crate) fn set_bold(&mut self, v: bool) {
        if v {
            self.flags |= BOLD;
        } else {
            self.flags &= !BOLD;
        }
    }

    pub(crate) fn dim(&self) -> bool {
        self.flags & DIM != 0
    }
    pub(crate) fn set_dim(&mut self, v: bool) {
        if v {
            self.flags |= DIM;
        } else {
            self.flags &= !DIM;
        }
    }

    pub(crate) fn italic(&self) -> bool {
        self.flags & ITALIC != 0
    }
    pub(crate) fn set_italic(&mut self, v: bool) {
        if v {
            self.flags |= ITALIC;
        } else {
            self.flags &= !ITALIC;
        }
    }

    pub(crate) fn underline(&self) -> bool {
        self.flags & UNDERLINE != 0
    }
    pub(crate) fn set_underline(&mut self, v: bool) {
        if v {
            self.flags |= UNDERLINE;
        } else {
            self.flags &= !UNDERLINE;
        }
    }

    pub(crate) fn reverse(&self) -> bool {
        self.flags & REVERSE != 0
    }
    pub(crate) fn set_reverse(&mut self, v: bool) {
        if v {
            self.flags |= REVERSE;
        } else {
            self.flags &= !REVERSE;
        }
    }

    pub(crate) fn strikethrough(&self) -> bool {
        self.flags & STRIKETHROUGH != 0
    }
    pub(crate) fn set_strikethrough(&mut self, v: bool) {
        if v {
            self.flags |= STRIKETHROUGH;
        } else {
            self.flags &= !STRIKETHROUGH;
        }
    }
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: AnsiColor::Default,
            bg: AnsiColor::Default,
            flags: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub(crate) ch: char,
    pub(crate) style: CellStyle,
    /// OSC 8 hyperlink ID: 0 = no link, 1-65535 = index into the screen's
    /// [`crate::hyperlink::HyperlinkInterner`]. Kept as a plain `u16` so
    /// `Cell` remains `Copy` and memory-layout-friendly.
    pub(crate) hyperlink_id: u16,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: CellStyle::default(),
            hyperlink_id: 0,
        }
    }
}
