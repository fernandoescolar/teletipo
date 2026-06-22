use crate::gap_buffer::GapBuffer;
use crate::history::{Edit, EditHistory};
use crate::types::{BufferEngineKind, Cursor, Selection, SemanticCommand};

/// Top-level editable text buffer used by the command-line editor.
///
/// Wraps a [`GapBuffer`] for efficient near-cursor edits while keeping a
/// materialised [`String`] copy for cheap `&str` access and pairs it with
/// cursor / selection state and an undo history.
#[derive(Debug, Default)]
pub struct EditorBuffer {
    /// Gap-buffer engine: provides O(1) insert/delete near the cursor.
    gap: GapBuffer,
    /// Always-current string materialisation of `gap`.  Updated after every
    /// mutation so that `text()` can return a `&str` without any allocation.
    text_cache: String,
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

    /// Rebuild `text_cache` from the gap buffer.  Called after every mutation.
    fn sync_cache(&mut self) {
        self.text_cache = self.gap.to_owned_string();
    }

    pub fn text(&self) -> &str {
        &self.text_cache
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
        let len = self.gap.len();
        let mut clamped = offset.min(len);
        while clamped > 0 && !self.gap.is_char_boundary(clamped) {
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
        while new > 0 && !self.gap.is_char_boundary(new) {
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
        if cur >= self.gap.len() {
            return;
        }
        // Determine step width from the cached string (which is always in sync).
        let step = self.text_cache[cur..]
            .chars()
            .next()
            .map_or(1, |c| c.len_utf8());
        self.set_cursor(cur + step, extend_selection);
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection.normalized();
        if start == end {
            return None;
        }
        self.text_cache.get(start..end).map(|s| s.to_string())
    }

    pub fn semantic_command(&self) -> Option<SemanticCommand> {
        let raw = self.text_cache.trim();
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
        if prefer_selection && let Some(sel) = self.selected_text() {
            let trimmed = sel.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }

        let trimmed = self.text_cache.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    pub fn insert_str(&mut self, s: &str) {
        self.delete_selection_inner(); // replace any active selection first
        let at = self.cursor.offset;
        self.gap.insert_str(at, s);
        self.sync_cache();
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

        let mut at = self.cursor.offset - 1;
        while at > 0 && !self.gap.is_char_boundary(at) {
            at -= 1;
        }
        let removed = self.gap.remove_char(at);
        self.sync_cache();
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
        if at >= self.gap.len() {
            return;
        }
        let removed = self.gap.remove_char(at);
        self.sync_cache();
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
        let removed = self.gap.delete_range(start, end);
        self.sync_cache();
        self.cursor.offset = start;
        self.selection.anchor = start;
        self.selection.active = start;
        self.history.push_undo(Edit::Delete {
            at: start,
            text: removed,
        });
        self.history.clear_redo();
        true
    }

    /// Returns whether an edit is available to undo.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Returns whether an edit is available to redo.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) {
        if let Some(edit) = self.history.pop_undo() {
            match &edit {
                Edit::Insert { at, text } => {
                    let end = at + text.len();
                    self.gap.delete_range(*at, end);
                    self.sync_cache();
                    self.cursor.offset = *at;
                }
                Edit::Delete { at, text } => {
                    self.gap.insert_str(*at, text);
                    self.sync_cache();
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
                    self.gap.insert_str(*at, text);
                    self.sync_cache();
                    self.cursor.offset = at + text.len();
                }
                Edit::Delete { at, text } => {
                    let end = at + text.len();
                    self.gap.delete_range(*at, end);
                    self.sync_cache();
                    self.cursor.offset = *at;
                }
            }
            self.selection.anchor = self.cursor.offset;
            self.selection.active = self.cursor.offset;
            self.history.push_undo(edit);
        }
    }

    /// Delete from the cursor to the start of the current line (Ctrl+U).
    pub fn delete_to_line_start(&mut self) {
        let pos = self.cursor.offset;
        let full = self.gap.to_owned_string();
        let line_start = full[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0);
        if pos == line_start {
            return;
        }
        let removed = self.gap.delete_range(line_start, pos);
        self.sync_cache();
        self.cursor.offset = line_start;
        self.selection.anchor = line_start;
        self.selection.active = line_start;
        self.history.push_undo(Edit::Delete {
            at: line_start,
            text: removed,
        });
        self.history.clear_redo();
    }

    /// Delete from the cursor to the end of the current line (Ctrl+K).
    pub fn delete_to_line_end(&mut self) {
        let pos = self.cursor.offset;
        let full = self.gap.to_owned_string();
        let line_end = full[pos..]
            .find('\n')
            .map(|i| pos + i)
            .unwrap_or(full.len());
        if pos == line_end {
            return;
        }
        let removed = self.gap.delete_range(pos, line_end);
        self.sync_cache();
        self.selection.anchor = pos;
        self.selection.active = pos;
        self.history.push_undo(Edit::Delete {
            at: pos,
            text: removed,
        });
        self.history.clear_redo();
    }

    /// Delete the word immediately before the cursor (Ctrl+W).
    pub fn delete_word_backward(&mut self) {
        let pos = self.cursor.offset;
        if pos == 0 {
            return;
        }
        let full = self.gap.to_owned_string();
        let before = &full[..pos];
        // Walk backwards: skip trailing whitespace, then skip the word.
        let mut chars = before.char_indices().rev();
        let mut at = pos;
        // Phase 1: skip whitespace.
        let mut hit_word = false;
        for (i, c) in chars.by_ref() {
            if c.is_whitespace() {
                at = i;
            } else {
                at = i;
                hit_word = true;
                break;
            }
        }
        if !hit_word {
            // Only whitespace before cursor — delete it all.
            if at == pos {
                return;
            }
        } else {
            // Phase 2: skip the word characters.
            for (i, c) in chars {
                if c.is_whitespace() {
                    at = i + c.len_utf8();
                    break;
                }
                at = i;
            }
        }
        if at >= pos {
            return;
        }
        let removed = self.gap.delete_range(at, pos);
        self.sync_cache();
        self.cursor.offset = at;
        self.selection.anchor = at;
        self.selection.active = at;
        self.history.push_undo(Edit::Delete { at, text: removed });
        self.history.clear_redo();
    }

    /// Clears all text and resets the cursor to the beginning.
    /// The cleared text is pushed onto the undo stack so it can be recovered.
    pub fn clear(&mut self) {
        if !self.gap.is_empty() {
            let removed = self.gap.clear();
            self.text_cache.clear();
            self.history.push_undo(Edit::Delete {
                at: 0,
                text: removed,
            });
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
    use super::EditorBuffer;
    use crate::types::BufferEngineKind;

    #[test]
    fn insert_undo_redo_cycle() {
        let mut buf = EditorBuffer::new();
        assert!(!buf.can_undo());
        assert!(!buf.can_redo());
        buf.insert_str("echo hi");
        assert_eq!(buf.text(), "echo hi");

        assert!(buf.can_undo());
        assert!(!buf.can_redo());
        buf.undo();
        assert_eq!(buf.text(), "");
        assert!(!buf.can_undo());
        assert!(buf.can_redo());

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
        assert_eq!(
            buf.command_payload(false).as_deref(),
            Some("echo from buffer")
        );
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
