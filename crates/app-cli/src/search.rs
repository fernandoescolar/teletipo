use crate::tab::TabState;

/// Panel geometry tuning expressed in grid units.
pub(crate) const PANEL_WIDTH_CELLS: f32 = 40.0;
pub(crate) const PANEL_HEIGHT_CELLS: f32 = 1.6;
pub(crate) const PANEL_BUTTON_CELLS: f32 = 2.0;

/// Character offset (in cells from panel left) where the query text begins (after the label).
pub(crate) const QUERY_TEXT_OFFSET_CELLS: f32 = 6.6;
/// Maximum number of query characters visible at once in the input area.
pub(crate) const QUERY_VISIBLE_CHARS: usize = 13;

#[derive(Clone, Debug, Default)]
pub(crate) struct SearchMatch {
    /// Absolute row index in full terminal text (oldest row = 0).
    pub(crate) abs_row: usize,
    /// Match start column (character-based).
    pub(crate) col_start: usize,
    /// Match end column (exclusive).
    pub(crate) col_end: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SearchState {
    pub(crate) active: bool,
    pub(crate) query: String,
    pub(crate) matches: Vec<SearchMatch>,
    pub(crate) current: usize,
    /// Number of rows in the full terminal snapshot used for this match set.
    pub(crate) total_rows: usize,
    /// When `true`, treat the query as a regular expression.
    pub(crate) regex_mode: bool,
    /// When `true`, the search is case-sensitive.
    pub(crate) case_sensitive: bool,
    /// Set when `regex_mode` is on but the query fails to compile.
    pub(crate) error: Option<String>,
    /// Byte offset of the text cursor inside `query`.
    pub(crate) cursor_byte: usize,
    /// Byte offset of the selection anchor.  `None` = no active selection.
    /// The selection spans `min(sel_anchor_byte, cursor_byte)..max(...)` in bytes.
    pub(crate) sel_anchor_byte: Option<usize>,
}

impl SearchState {
    /// Return the character index of the cursor (0-based).
    pub(crate) fn cursor_char_index(&self) -> usize {
        let clamped = self.cursor_byte.min(self.query.len());
        self.query[..clamped].chars().count()
    }

    /// Return the selected char-index range `(start, end)` where `start < end`,
    /// or `None` if there is no selection (anchor == cursor, or no anchor).
    pub(crate) fn sel_char_range(&self) -> Option<(usize, usize)> {
        let anchor_byte = self.sel_anchor_byte?;
        let anchor_byte = anchor_byte.min(self.query.len());
        let cursor_byte = self.cursor_byte.min(self.query.len());
        if anchor_byte == cursor_byte {
            return None;
        }
        let a = self.query[..anchor_byte].chars().count();
        let c = self.query[..cursor_byte].chars().count();
        if a <= c { Some((a, c)) } else { Some((c, a)) }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SearchPanelHitbox {
    pub(crate) panel_x: f64,
    pub(crate) panel_y: f64,
    pub(crate) panel_w: f64,
    pub(crate) panel_h: f64,
    pub(crate) prev_x: f64,
    pub(crate) next_x: f64,
    pub(crate) close_x: f64,
    pub(crate) button_w: f64,
}

/// Recompute search matches against the full terminal snapshot.
pub(crate) fn refresh_search(tab: &mut TabState) {
    let full_text = tab.app.terminal_snapshot_with_scrollback();
    let total_rows = full_text.lines().count().max(1);
    tab.search.total_rows = total_rows;
    let (matches, error) =
        compute_matches(&full_text, &tab.search.query, tab.search.regex_mode, tab.search.case_sensitive);
    tab.search.error = error;
    tab.search.matches = matches;
    // Keep cursor and anchor inside the (possibly modified) query.
    let q_len = tab.search.query.len();
    tab.search.cursor_byte = clamp_to_char_boundary(&tab.search.query, tab.search.cursor_byte.min(q_len));
    if let Some(a) = tab.search.sel_anchor_byte.as_mut() {
        *a = clamp_to_char_boundary(&tab.search.query, (*a).min(q_len));
    }
    if tab.search.matches.is_empty() {
        tab.search.current = 0;
        clear_terminal_selection(tab);
        return;
    }
    if tab.search.current >= tab.search.matches.len() {
        tab.search.current = 0;
    }
    jump_to_current(tab);
}

/// Move focus to the previous match, wrapping at the beginning.
pub(crate) fn prev_match(tab: &mut TabState) {
    if tab.search.matches.is_empty() {
        return;
    }
    tab.search.current = if tab.search.current == 0 {
        tab.search.matches.len() - 1
    } else {
        tab.search.current - 1
    };
    jump_to_current(tab);
}

/// Move focus to the next match, wrapping at the end.
pub(crate) fn next_match(tab: &mut TabState) {
    if tab.search.matches.is_empty() {
        return;
    }
    tab.search.current = (tab.search.current + 1) % tab.search.matches.len();
    jump_to_current(tab);
}

pub(crate) fn close_search(tab: &mut TabState) {
    tab.search.active = false;
    tab.search.query.clear();
    tab.search.matches.clear();
    tab.search.current = 0;
    tab.search.cursor_byte = 0;
    tab.search.sel_anchor_byte = None;
    clear_terminal_selection(tab);
}

pub(crate) fn search_panel_hitbox(
    window_width: u32,
    tab_bar_h: f32,
    cell_w: f32,
    cell_h: f32,
    pad_h: f32,
    pad_v: f32,
) -> Option<SearchPanelHitbox> {
    if window_width == 0 || cell_w <= 0.0 || cell_h <= 0.0 {
        return None;
    }
    let panel_w = (cell_w * PANEL_WIDTH_CELLS) as f64;
    let panel_h = (cell_h * PANEL_HEIGHT_CELLS) as f64;
    let panel_x = (window_width as f32 - pad_h - panel_w as f32).max(0.0) as f64;
    let panel_y = (tab_bar_h + pad_v) as f64;
    let button_w = (cell_w * PANEL_BUTTON_CELLS) as f64;
    let close_x = panel_x + panel_w - button_w;
    let next_x = close_x - button_w;
    let prev_x = next_x - button_w;
    Some(SearchPanelHitbox {
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        prev_x,
        next_x,
        close_x,
        button_w,
    })
}

pub(crate) fn hit_prev(hitbox: &SearchPanelHitbox, x: f64, y: f64) -> bool {
    in_button(hitbox, hitbox.prev_x, x, y)
}

pub(crate) fn hit_next(hitbox: &SearchPanelHitbox, x: f64, y: f64) -> bool {
    in_button(hitbox, hitbox.next_x, x, y)
}

pub(crate) fn hit_close(hitbox: &SearchPanelHitbox, x: f64, y: f64) -> bool {
    in_button(hitbox, hitbox.close_x, x, y)
}

pub(crate) fn in_panel(hitbox: &SearchPanelHitbox, x: f64, y: f64) -> bool {
    x >= hitbox.panel_x
        && x <= hitbox.panel_x + hitbox.panel_w
        && y >= hitbox.panel_y
        && y <= hitbox.panel_y + hitbox.panel_h
}

fn in_button(hitbox: &SearchPanelHitbox, button_x: f64, x: f64, y: f64) -> bool {
    x >= button_x
        && x <= button_x + hitbox.button_w
        && y >= hitbox.panel_y
        && y <= hitbox.panel_y + hitbox.panel_h
}

fn clear_terminal_selection(tab: &mut TabState) {
    tab.selection_anchor = None;
    tab.selection_end = None;
    tab.is_selecting = false;
}

fn jump_to_current(tab: &mut TabState) {
    let Some(current) = tab.search.matches.get(tab.search.current).cloned() else {
        clear_terminal_selection(tab);
        return;
    };

    let visible_rows = tab.term_row_count.max(1);
    let total_rows = tab.search.total_rows.max(visible_rows);
    let scrollback = tab.app.scrollback_len();

    // Place the match approximately in the middle of the viewport.
    let center_target = current.abs_row.saturating_sub(visible_rows / 2);
    let max_start = total_rows.saturating_sub(visible_rows);
    let clamped_start = center_target.min(max_start);
    let desired_scroll = total_rows
        .saturating_sub(visible_rows)
        .saturating_sub(clamped_start)
        .min(scrollback);
    tab.scroll_offset = desired_scroll;

    // Recompute visible window after scroll and map absolute row to viewport row.
    let window_start = total_rows
        .saturating_sub(visible_rows)
        .saturating_sub(tab.scroll_offset.min(scrollback));
    let row_in_view = current.abs_row.saturating_sub(window_start);

    tab.selection_anchor = Some((row_in_view, current.col_start));
    tab.selection_anchor_scroll = tab.scroll_offset;
    tab.selection_end = Some((row_in_view, current.col_end.saturating_sub(1)));
    tab.selection_end_scroll = tab.scroll_offset;
    tab.is_selecting = false;
}

fn compute_matches(
    text: &str,
    query: &str,
    regex_mode: bool,
    case_sensitive: bool,
) -> (Vec<SearchMatch>, Option<String>) {
    let needle = query.trim();
    if needle.is_empty() {
        return (Vec::new(), None);
    }

    if regex_mode {
        let pattern = if case_sensitive {
            needle.to_owned()
        } else {
            format!("(?i){needle}")
        };
        let re = match regex::Regex::new(&pattern) {
            Ok(r) => r,
            Err(err) => return (Vec::new(), Some(err.to_string())),
        };
        let mut out = Vec::new();
        for (row, line) in text.lines().enumerate() {
            for m in re.find_iter(line) {
                let col_start = line[..m.start()].chars().count();
                let col_end = line[..m.end()].chars().count();
                out.push(SearchMatch { abs_row: row, col_start, col_end });
            }
        }
        (out, None)
    } else if case_sensitive {
        let needle_chars = needle.chars().count();
        let mut out = Vec::new();
        for (row, line) in text.lines().enumerate() {
            for (byte_start, _) in line.match_indices(needle) {
                let col_start = line[..byte_start].chars().count();
                out.push(SearchMatch {
                    abs_row: row,
                    col_start,
                    col_end: col_start + needle_chars,
                });
            }
        }
        (out, None)
    } else {
        let needle_fold = needle.to_ascii_lowercase();
        let needle_chars = needle.chars().count();
        let mut out = Vec::new();
        for (row, line) in text.lines().enumerate() {
            let hay = line.to_ascii_lowercase();
            for (byte_start, _) in hay.match_indices(&needle_fold) {
                let col_start = line[..byte_start].chars().count();
                out.push(SearchMatch {
                    abs_row: row,
                    col_start,
                    col_end: col_start + needle_chars,
                });
            }
        }
        (out, None)
    }
}

// ─── Cursor / text-editing helpers ───────────────────────────────────────────

/// Clamp `byte_pos` to the nearest valid char boundary (≤ pos).
fn clamp_to_char_boundary(s: &str, byte_pos: usize) -> usize {
    let mut p = byte_pos.min(s.len());
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// Byte offset of the char boundary one char *before* `byte_pos` (or 0).
fn prev_char_boundary(s: &str, byte_pos: usize) -> usize {
    let clamped = byte_pos.min(s.len());
    if clamped == 0 {
        return 0;
    }
    s[..clamped].char_indices().rev().next().map(|(i, _)| i).unwrap_or(0)
}

/// Byte offset one past the char that *starts* at or after `byte_pos`.
fn next_char_boundary(s: &str, byte_pos: usize) -> usize {
    let clamped = byte_pos.min(s.len());
    s[clamped..].chars().next().map(|c| clamped + c.len_utf8()).unwrap_or(clamped)
}

/// Byte offset of the start of the previous word (for Option/Alt+Backspace / Option+Left).
fn prev_word_boundary(s: &str, byte_pos: usize) -> usize {
    let before = &s[..byte_pos.min(s.len())];
    // skip non-word chars, then skip word chars
    let no_space = before.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_');
    no_space.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_').len()
}

/// Byte offset past the end of the next word (for Option/Alt+Right).
fn next_word_boundary(s: &str, byte_pos: usize) -> usize {
    let clamped = byte_pos.min(s.len());
    let after = &s[clamped..];
    // skip word chars, then skip non-word chars
    let after_word = after.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
    let after_space =
        after_word.trim_start_matches(|c: char| !c.is_alphanumeric() && c != '_');
    clamped + (after.len() - after_space.len())
}

/// Convert a char index to a byte offset.
fn char_idx_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(s.len())
}

// ─── Public text-editing operations ──────────────────────────────────────────

/// Delete the active selection and move the cursor to the deletion point.
/// Returns `true` if anything was deleted.
fn delete_selection(tab: &mut TabState) -> bool {
    if let Some((sc, ec)) = tab.search.sel_char_range() {
        let sb = char_idx_to_byte(&tab.search.query, sc);
        let eb = char_idx_to_byte(&tab.search.query, ec);
        tab.search.query.drain(sb..eb);
        tab.search.cursor_byte = sb;
        tab.search.sel_anchor_byte = None;
        return true;
    }
    false
}

/// Insert `text` at the cursor position (replacing any selection first).
pub(crate) fn search_insert(tab: &mut TabState, text: &str) {
    delete_selection(tab);
    let pos = tab.search.cursor_byte;
    tab.search.query.insert_str(pos, text);
    tab.search.cursor_byte = pos + text.len();
    tab.search.sel_anchor_byte = None;
    refresh_search(tab);
}

/// Delete the character *before* the cursor (or the active selection).
pub(crate) fn search_delete_backward(tab: &mut TabState) {
    if delete_selection(tab) {
        refresh_search(tab);
        return;
    }
    let pos = tab.search.cursor_byte;
    if pos > 0 {
        let new_pos = prev_char_boundary(&tab.search.query, pos);
        tab.search.query.drain(new_pos..pos);
        tab.search.cursor_byte = new_pos;
    }
    tab.search.sel_anchor_byte = None;
    refresh_search(tab);
}

/// Delete the character *after* the cursor (or the active selection).
pub(crate) fn search_delete_forward(tab: &mut TabState) {
    if delete_selection(tab) {
        refresh_search(tab);
        return;
    }
    let pos = tab.search.cursor_byte;
    let len = tab.search.query.len();
    if pos < len {
        let end = next_char_boundary(&tab.search.query, pos);
        tab.search.query.drain(pos..end);
    }
    tab.search.sel_anchor_byte = None;
    refresh_search(tab);
}

/// Delete the word immediately before the cursor (Option/Alt+Backspace).
pub(crate) fn search_delete_word_backward(tab: &mut TabState) {
    if delete_selection(tab) {
        refresh_search(tab);
        return;
    }
    let pos = tab.search.cursor_byte;
    let new_pos = prev_word_boundary(&tab.search.query, pos);
    if new_pos < pos {
        tab.search.query.drain(new_pos..pos);
        tab.search.cursor_byte = new_pos;
    }
    tab.search.sel_anchor_byte = None;
    refresh_search(tab);
}

/// Move cursor one character to the left. With `extend_sel` it grows the selection.
pub(crate) fn search_move_left(tab: &mut TabState, extend_sel: bool) {
    let cur = tab.search.cursor_byte;
    if extend_sel {
        tab.search.sel_anchor_byte.get_or_insert(cur);
    } else if let Some((sc, _)) = tab.search.sel_char_range() {
        // Collapse to selection start.
        tab.search.cursor_byte = char_idx_to_byte(&tab.search.query, sc);
        tab.search.sel_anchor_byte = None;
        return;
    } else {
        tab.search.sel_anchor_byte = None;
    }
    tab.search.cursor_byte = prev_char_boundary(&tab.search.query, cur);
}

/// Move cursor one character to the right. With `extend_sel` it grows the selection.
pub(crate) fn search_move_right(tab: &mut TabState, extend_sel: bool) {
    let cur = tab.search.cursor_byte;
    if extend_sel {
        tab.search.sel_anchor_byte.get_or_insert(cur);
    } else if let Some((_, ec)) = tab.search.sel_char_range() {
        // Collapse to selection end.
        tab.search.cursor_byte = char_idx_to_byte(&tab.search.query, ec);
        tab.search.sel_anchor_byte = None;
        return;
    } else {
        tab.search.sel_anchor_byte = None;
    }
    tab.search.cursor_byte = next_char_boundary(&tab.search.query, cur);
}

/// Move cursor to the start of the query.
pub(crate) fn search_move_home(tab: &mut TabState, extend_sel: bool) {
    let cur = tab.search.cursor_byte;
    if extend_sel {
        tab.search.sel_anchor_byte.get_or_insert(cur);
    } else {
        tab.search.sel_anchor_byte = None;
    }
    tab.search.cursor_byte = 0;
}

/// Move cursor to the end of the query.
pub(crate) fn search_move_end(tab: &mut TabState, extend_sel: bool) {
    let cur = tab.search.cursor_byte;
    if extend_sel {
        tab.search.sel_anchor_byte.get_or_insert(cur);
    } else {
        tab.search.sel_anchor_byte = None;
    }
    tab.search.cursor_byte = tab.search.query.len();
}

/// Move cursor one word to the left (Option/Alt+Left).
pub(crate) fn search_move_word_left(tab: &mut TabState, extend_sel: bool) {
    let cur = tab.search.cursor_byte;
    if extend_sel {
        tab.search.sel_anchor_byte.get_or_insert(cur);
    } else {
        tab.search.sel_anchor_byte = None;
    }
    tab.search.cursor_byte = prev_word_boundary(&tab.search.query, cur);
}

/// Move cursor one word to the right (Option/Alt+Right).
pub(crate) fn search_move_word_right(tab: &mut TabState, extend_sel: bool) {
    let cur = tab.search.cursor_byte;
    if extend_sel {
        tab.search.sel_anchor_byte.get_or_insert(cur);
    } else {
        tab.search.sel_anchor_byte = None;
    }
    tab.search.cursor_byte = next_word_boundary(&tab.search.query, cur);
}

/// Select all query text (Cmd+A).
pub(crate) fn search_select_all(tab: &mut TabState) {
    tab.search.sel_anchor_byte = Some(0);
    tab.search.cursor_byte = tab.search.query.len();
}

/// Return the currently-selected text slice (empty if no selection).
pub(crate) fn search_selected_text(tab: &TabState) -> &str {
    if let Some((sc, ec)) = tab.search.sel_char_range() {
        let sb = char_idx_to_byte(&tab.search.query, sc);
        let eb = char_idx_to_byte(&tab.search.query, ec);
        &tab.search.query[sb..eb]
    } else {
        ""
    }
}

/// Set the cursor to `char_idx` (clamped) and clear any selection.
pub(crate) fn search_set_cursor(tab: &mut TabState, char_idx: usize) {
    let total_chars = tab.search.query.chars().count();
    let clamped = char_idx.min(total_chars);
    tab.search.cursor_byte = char_idx_to_byte(&tab.search.query, clamped);
    tab.search.sel_anchor_byte = None;
}
