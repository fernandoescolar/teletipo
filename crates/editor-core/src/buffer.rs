use crate::history::{Edit, EditHistory};
use crate::types::{BufferEngineKind, Cursor, SemanticCommand, Selection};

#[derive(Debug, Default)]
pub struct EditorBuffer {
    text: String,
    cursor: Cursor,
    selection: Selection,
    engine: BufferEngineKind,
    history: EditHistory,
}

impl EditorBuffer {
    pub fn new() -> Self {
        Self {
            engine: BufferEngineKind::GapBuffer,
            ..Self::default()
        }
    }

    pub fn with_engine(engine: BufferEngineKind) -> Self {
        Self {
            engine,
            ..Self::new()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn engine(&self) -> BufferEngineKind {
        self.engine
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    pub fn set_cursor(&mut self, offset: usize, extend_selection: bool) {
        let mut clamped = offset.min(self.text.len());
        // Snap to the nearest char boundary so callers never need to worry
        // about multi-byte character alignment.
        while clamped > 0 && !self.text.is_char_boundary(clamped) {
            clamped -= 1;
        }
        self.cursor.offset = clamped;
        if extend_selection {
            self.selection.active = clamped;
        } else {
            self.selection.anchor = clamped;
            self.selection.active = clamped;
        }
    }

    pub fn move_cursor_left(&mut self, extend_selection: bool) {
        let cur = self.cursor.offset;
        if !extend_selection {
            let (start, end) = self.selection.normalized();
            if start != end {
                self.set_cursor(start, false);
                return;
            }
        }
        if cur == 0 {
            return;
        }
        let mut new = cur - 1;
        while new > 0 && !self.text.is_char_boundary(new) {
            new -= 1;
        }
        self.set_cursor(new, extend_selection);
    }

    pub fn move_cursor_right(&mut self, extend_selection: bool) {
        let cur = self.cursor.offset;
        if !extend_selection {
            let (start, end) = self.selection.normalized();
            if start != end {
                self.set_cursor(end, false);
                return;
            }
        }
        if cur >= self.text.len() {
            return;
        }
        let step = self.text[cur..].chars().next().map_or(1, |c| c.len_utf8());
        self.set_cursor(cur + step, extend_selection);
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection.normalized();
        if start == end {
            return None;
        }
        self.text.get(start..end).map(|s| s.to_string())
    }

    pub fn semantic_command(&self) -> Option<SemanticCommand> {
        let raw = self.text.trim();
        if raw.is_empty() {
            return None;
        }

        let parts = tokenize_shell_words(raw);
        let command = parts.first()?.clone();
        let args = parts.into_iter().skip(1).collect();

        Some(SemanticCommand {
            raw: raw.to_string(),
            command,
            args,
        })
    }

    pub fn command_payload(&self, prefer_selection: bool) -> Option<String> {
        if prefer_selection
            && let Some(sel) = self.selected_text() {
                let trimmed = sel.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }

        let trimmed = self.text.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    pub fn insert_str(&mut self, s: &str) {
        self.delete_selection_inner(); // replace any active selection first
        let at = self.cursor.offset;
        self.text.insert_str(at, s);
        self.cursor.offset += s.len();
        self.selection.anchor = self.cursor.offset;
        self.selection.active = self.cursor.offset;
        self.history.push_undo(Edit::Insert {
            at,
            text: s.to_string(),
        });
        self.history.clear_redo();
    }

    pub fn backspace(&mut self) {
        if self.delete_selection_inner() {
            return;
        }
        if self.cursor.offset == 0 {
            return;
        }

        // Walk backward from the cursor to find the previous char boundary;
        // this is necessary for multi-byte characters (e.g. accented letters,
        // CJK, emoji) where a single character occupies more than 1 byte.
        let mut at = self.cursor.offset - 1;
        while at > 0 && !self.text.is_char_boundary(at) {
            at -= 1;
        }
        let removed = self.text.remove(at);
        self.cursor.offset = at;
        self.selection.anchor = at;
        self.selection.active = at;
        self.history.push_undo(Edit::Delete {
            at,
            text: removed.to_string(),
        });
        self.history.clear_redo();
    }

    /// Delete the character at (after) the cursor — "forward delete" / Delete key.
    pub fn delete_forward(&mut self) {
        if self.delete_selection_inner() {
            return;
        }
        let at = self.cursor.offset;
        if at >= self.text.len() {
            return;
        }
        let removed = self.text.remove(at);
        // Cursor stays at the same byte position.
        self.selection.anchor = self.cursor.offset;
        self.selection.active = self.cursor.offset;
        self.history.push_undo(Edit::Delete {
            at,
            text: removed.to_string(),
        });
        self.history.clear_redo();
    }

    /// Deletes the selected text (if any). Returns `true` if something was deleted.
    fn delete_selection_inner(&mut self) -> bool {
        let (start, end) = self.selection.normalized();
        if start == end {
            return false;
        }
        let removed: String = self.text[start..end].to_string();
        self.text.replace_range(start..end, "");
        self.cursor.offset = start;
        self.selection.anchor = start;
        self.selection.active = start;
        self.history.push_undo(Edit::Delete { at: start, text: removed });
        self.history.clear_redo();
        true
    }

    pub fn undo(&mut self) {
        if let Some(edit) = self.history.pop_undo() {
            match &edit {
                Edit::Insert { at, text } => {
                    let end = at + text.len();
                    self.text.replace_range(*at..end, "");
                    self.cursor.offset = *at;
                }
                Edit::Delete { at, text } => {
                    self.text.insert_str(*at, text);
                    self.cursor.offset = at + text.len();
                }
            }
            self.selection.anchor = self.cursor.offset;
            self.selection.active = self.cursor.offset;
            self.history.push_redo(edit);
        }
    }

    pub fn redo(&mut self) {
        if let Some(edit) = self.history.pop_redo() {
            match &edit {
                Edit::Insert { at, text } => {
                    self.text.insert_str(*at, text);
                    self.cursor.offset = at + text.len();
                }
                Edit::Delete { at, text } => {
                    let end = at + text.len();
                    self.text.replace_range(*at..end, "");
                    self.cursor.offset = *at;
                }
            }
            self.selection.anchor = self.cursor.offset;
            self.selection.active = self.cursor.offset;
            self.history.push_undo(edit);
        }
    }

    /// Clears all text and resets the cursor to the beginning.
    /// The cleared text is pushed onto the undo stack so it can be recovered.
    pub fn clear(&mut self) {
        if !self.text.is_empty() {
            let removed = std::mem::take(&mut self.text);
            self.history.push_undo(Edit::Delete { at: 0, text: removed });
            self.history.clear_redo();
        }
        self.cursor.offset = 0;
        self.selection = Selection::default();
    }
}

pub(crate) fn tokenize_shell_words(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else if ch == '\\' && q == '"' {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote = Some(ch),
            ' ' | '\t' => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{EditorBuffer};
    use crate::types::BufferEngineKind;

    #[test]
    fn insert_undo_redo_cycle() {
        let mut buf = EditorBuffer::new();
        buf.insert_str("echo hi");
        assert_eq!(buf.text(), "echo hi");

        buf.undo();
        assert_eq!(buf.text(), "");

        buf.redo();
        assert_eq!(buf.text(), "echo hi");
    }

    #[test]
    fn keeps_cursor_and_selection_consistent() {
        let mut buf = EditorBuffer::with_engine(BufferEngineKind::GapBuffer);
        buf.insert_str("hello");
        buf.set_cursor(1, false);
        buf.set_cursor(4, true);

        assert_eq!(buf.selected_text().as_deref(), Some("ell"));
        assert_eq!(buf.engine(), BufferEngineKind::GapBuffer);
    }

    #[test]
    fn extracts_semantic_command() {
        let mut buf = EditorBuffer::new();
        buf.insert_str("git status --short");
        let cmd = buf.semantic_command().expect("semantic command");
        assert_eq!(cmd.command, "git");
        assert_eq!(cmd.args, vec!["status".to_string(), "--short".to_string()]);
    }

    #[test]
    fn parses_quoted_args() {
        let mut buf = EditorBuffer::new();
        buf.insert_str("echo \"hello world\" 'x y'");
        let cmd = buf.semantic_command().expect("semantic command");

        assert_eq!(cmd.command, "echo");
        assert_eq!(cmd.args, vec!["hello world".to_string(), "x y".to_string()]);
    }

    #[test]
    fn clear_empties_buffer_and_is_undoable() {
        let mut buf = EditorBuffer::new();
        buf.insert_str("keep me");
        buf.clear();
        assert_eq!(buf.text(), "");
        assert_eq!(buf.cursor().offset, 0);
        buf.undo();
        assert_eq!(buf.text(), "keep me");
    }

    #[test]
    fn command_payload_prefers_selection() {
        let mut buf = EditorBuffer::new();
        buf.insert_str("echo from buffer");
        buf.set_cursor(5, false);
        buf.set_cursor(9, true);

        assert_eq!(buf.command_payload(true).as_deref(), Some("from"));
        assert_eq!(buf.command_payload(false).as_deref(), Some("echo from buffer"));
    }

    #[test]
    fn cursor_movement_respects_utf8_boundaries() {
        let mut buf = EditorBuffer::new();
        buf.insert_str("aéz");
        buf.set_cursor(buf.text().len(), false);

        buf.move_cursor_left(false);
        assert_eq!(buf.cursor().offset, 3);

        buf.move_cursor_left(false);
        assert_eq!(buf.cursor().offset, 1);

        buf.move_cursor_right(false);
        assert_eq!(buf.cursor().offset, 3);
    }
}
