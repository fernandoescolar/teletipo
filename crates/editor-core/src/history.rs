#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Edit {
    Insert { at: usize, text: String },
    Delete { at: usize, text: String },
}

#[derive(Debug, Default)]
pub(crate) struct EditHistory {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl EditHistory {
    pub(crate) fn push_undo(&mut self, edit: Edit) {
        self.undo.push(edit);
    }

    pub(crate) fn pop_undo(&mut self) -> Option<Edit> {
        self.undo.pop()
    }

    pub(crate) fn push_redo(&mut self, edit: Edit) {
        self.redo.push(edit);
    }

    pub(crate) fn pop_redo(&mut self) -> Option<Edit> {
        self.redo.pop()
    }

    pub(crate) fn clear_redo(&mut self) {
        self.redo.clear();
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
