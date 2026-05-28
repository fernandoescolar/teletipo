#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BufferEngineKind {
    #[default]
    GapBuffer,
    Rope,
    PieceTable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    pub offset: usize,
    pub preferred_column: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub active: usize,
}

impl Selection {
    pub fn normalized(self) -> (usize, usize) {
        (self.anchor.min(self.active), self.anchor.max(self.active))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticCommand {
    pub raw: String,
    pub command: String,
    pub args: Vec<String>,
}
