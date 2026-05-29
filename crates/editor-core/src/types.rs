#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BufferEngineKind {
    #[default]
    GapBuffer,
    Rope,
    PieceTable,
}

/// Insertion / navigation cursor inside an [`crate::EditorBuffer`].
///
/// `offset` is a byte offset into the buffer's text; `preferred_column` is
/// remembered across vertical moves so the cursor snaps back to the visual
/// column the user originally landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    /// Byte offset into the buffer text.
    pub offset: usize,
    /// Sticky visual column preserved across vertical movement.
    pub preferred_column: Option<usize>,
}

/// A possibly-empty text selection identified by two byte offsets.
///
/// `anchor` is where the selection started; `active` is where it currently
/// ends.  Either side may be the lesser offset, so use
/// [`Selection::normalized`] before slicing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    /// Byte offset where the selection was anchored.
    pub anchor: usize,
    /// Byte offset where the selection currently extends to.
    pub active: usize,
}

impl Selection {
    pub fn normalized(self) -> (usize, usize) {
        (self.anchor.min(self.active), self.anchor.max(self.active))
    }
}

/// A parsed shell-style command extracted from the editor buffer.
///
/// Used by the shell-token highlighter and history machinery; `raw` is the
/// original input, `command` is the first whitespace-separated token, and
/// `args` is everything after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCommand {
    /// The original, unparsed line.
    pub raw: String,
    /// The first whitespace-separated token (the executable / builtin).
    pub command: String,
    /// Remaining whitespace-separated tokens after `command`.
    pub args: Vec<String>,
}
