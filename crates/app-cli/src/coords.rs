use platform_abstraction::{ProcessInfo, current_process_info};
use render_glow::SCROLLBAR_W_PX;

use crate::GpuRuntimeState;

/// Keep the editor caret column inside the horizontal viewport.
pub(crate) fn clamp_editor_horizontal_scroll_offset(
    cursor_col: usize,
    visible_cols: usize,
    current_offset: usize,
) -> usize {
    if cursor_col < current_offset {
        cursor_col
    } else if cursor_col >= current_offset + visible_cols {
        cursor_col + 1 - visible_cols
    } else {
        current_offset
    }
}

/// Keep the editor caret row inside the vertical viewport.
pub(crate) fn clamp_editor_scroll_offset(
    cursor_row: usize,
    visible_rows: usize,
    current_offset: usize,
) -> usize {
    if cursor_row < current_offset {
        cursor_row
    } else if cursor_row >= current_offset + visible_rows {
        cursor_row + 1 - visible_rows
    } else {
        current_offset
    }
}

/// Adjusts `state.editor_scroll_offset` to keep the caret row visible.
pub(crate) fn clamp_editor_scroll(state: &mut GpuRuntimeState) {
    let tab_bar_h = state.tab_bar_h();
    let window_height = state.layout.window_height;
    let cell_h = state.layout.cell_h;
    let pad_v = state.user_config.padding.vertical as f32;
    let visible_cols = if cell_h > 0.0 && state.layout.cell_w > 0.0 {
        ((state.layout.window_width as f32
            - 2.0 * state.user_config.padding.horizontal as f32
            - SCROLLBAR_W_PX)
            / state.layout.cell_w)
            .floor()
            .max(1.0) as usize
    } else {
        1
    };
    let active = state.active_tab;
    let tab = &mut state.tabs[active];
    let text = tab.app.editor_snapshot();
    let offset = tab.app.editor_cursor_offset();
    let before = &text[..offset.min(text.len())];
    let caret_row = before.chars().filter(|&c| c == '\n').count();
    let caret_col = before
        .rsplit_once('\n')
        .map_or(before, |(_, line)| line)
        .chars()
        .count();
    let editor_h_px = (1.0 - tab.split_ratio) * (window_height as f32 - tab_bar_h);
    // Text starts pad_v pixels below the pane top and ends pad_v above the
    // bottom, so subtract both top and bottom padding from the usable area.
    let visible_rows = if cell_h > 0.0 {
        ((editor_h_px - 2.0 * pad_v) / cell_h).floor().max(1.0) as usize
    } else {
        1
    };
    tab.editor_scroll_offset =
        clamp_editor_scroll_offset(caret_row, visible_rows, tab.editor_scroll_offset);
    tab.editor_horizontal_scroll_offset = clamp_editor_horizontal_scroll_offset(
        caret_col,
        visible_cols,
        tab.editor_horizontal_scroll_offset,
    );
}

/// Return the byte range of the code-like word at `offset`.
///
/// Identifiers include Unicode alphanumeric characters and underscores. When
/// the click lands on punctuation or whitespace, that single character is
/// selected, matching the behavior of common code editors.
pub(crate) fn editor_word_bounds(text: &str, offset: usize) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }
    let mut at = offset.min(text.len());
    if at == text.len() {
        at = text.char_indices().next_back().map_or(0, |(idx, _)| idx);
    } else {
        while at > 0 && !text.is_char_boundary(at) {
            at -= 1;
        }
    }
    let ch = text[at..].chars().next().expect("valid character boundary");
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    if !is_word(ch) {
        return (at, at + ch.len_utf8());
    }
    let mut start = at;
    for (idx, candidate) in text[..at].char_indices().rev() {
        if !is_word(candidate) {
            break;
        }
        start = idx;
    }
    let mut end = at + ch.len_utf8();
    for candidate in text[end..].chars() {
        if !is_word(candidate) {
            break;
        }
        end += candidate.len_utf8();
    }
    (start, end)
}

/// Convert an editor (row, col) grid position to a byte offset in `text`.
/// Both are clamped to the actual text contents.
pub(crate) fn editor_row_col_to_offset(text: &str, row: usize, col: usize) -> usize {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.is_empty() {
        return 0;
    }
    let row = row.min(lines.len() - 1);
    // Byte offset of the start of `row` (sum of lengths of preceding lines + their '\n').
    let line_start: usize = lines[..row].iter().map(|l| l.len() + 1).sum();
    let line = lines[row];
    // Walk `col` chars into the line, stopping at the end.
    let char_byte = line
        .char_indices()
        .nth(col)
        .map(|(i, _)| i)
        .unwrap_or(line.len());
    line_start + char_byte
}

/// Return the byte offset at the start of the line containing `offset`.
pub(crate) fn editor_line_start_offset(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text[..offset].rfind('\n').map_or(0, |index| index + 1)
}

/// Return the byte offset at the end of the line containing `offset`.
pub(crate) fn editor_line_end_offset(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    text[offset..]
        .find('\n')
        .map_or(text.len(), |index| offset + index)
}

/// Returns the (row, col) grid position of the cursor in `text`.
/// row is 0-indexed; col is the character count from the start of the current line.
pub(crate) fn editor_cursor_row_col(text: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(text.len());
    let before = &text[..clamped];
    let row = before.chars().filter(|&c| c == '\n').count();
    let col = match before.rfind('\n') {
        Some(pos) => before[pos + 1..].chars().count(),
        None => before.chars().count(),
    };
    (row, col)
}

/// Layout parameters for the terminal pane, used when mapping pixel coordinates
/// to terminal grid cells.
pub(crate) struct TerminalLayout {
    pub split_ratio: f32,
    pub cell_w_px: f32,
    pub cell_h_px: f32,
    pub term_row_count: usize,
    pub tab_bar_h: f32,
    pub pad_h: f32,
    pub pad_v: f32,
}

/// Converts a cursor physical-pixel position to a terminal grid (row, col).
/// Returns `None` if the cursor is outside the terminal pane or in the scrollbar area.
/// Uses `term_row_count` to compute the bottom-alignment offset so the mapping
/// matches what the renderer draws.  `pad_h` and `pad_v` are the horizontal and
/// vertical padding (pixels) applied to the terminal content area; clicks inside
/// the padding region are clamped to the first row/column.
pub(crate) fn cursor_to_terminal_cell(
    cx: f64,
    cy: f64,
    window_width: u32,
    window_height: u32,
    layout: &TerminalLayout,
) -> Option<(usize, usize)> {
    let cell_w = layout.cell_w_px as f64;
    let cell_h = layout.cell_h_px as f64;
    let tbh = layout.tab_bar_h as f64;
    let available_h = window_height as f64 - tbh;
    let terminal_h = available_h * layout.split_ratio as f64;
    // Mirror the renderer: pad_v is reserved at both the top and bottom, so
    // the usable content area is narrower.  Text for row r starts at:
    //   term_top_offset_px + pad_v + r * cell_h
    // where term_top_offset_px = tbh + (effective_term_h - content_h).
    let effective_term_h = (terminal_h - 2.0 * layout.pad_v as f64).max(0.0);
    let content_h = (layout.term_row_count as f64 * cell_h).min(effective_term_h);
    let term_top_y = tbh + (effective_term_h - content_h).max(0.0) + layout.pad_v as f64;
    let term_bottom_y = tbh + terminal_h;
    let scrollbar_x = window_width as f64 - SCROLLBAR_W_PX as f64;
    if cy < term_top_y || cy >= term_bottom_y || cx < 0.0 || cx >= scrollbar_x {
        return None;
    }
    let col = ((cx - layout.pad_h as f64) / cell_w).max(0.0) as usize;
    let row = ((cy - term_top_y) / cell_h).max(0.0) as usize;
    Some((row, col))
}

/// Read the current working directory of an OS process.
/// Returns `None` if the pid is no longer alive or the OS call fails.
pub(crate) fn read_child_cwd(pid: u32) -> Option<String> {
    current_process_info().read_child_cwd(pid)
}

/// Shorten a cwd string for display in a tab.
/// Replaces the home directory with `~`, then trims to the last `max_chars` characters.
pub(crate) fn shorten_cwd_label(cwd: &str, max_chars: usize) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let s = if !home.is_empty() && cwd.starts_with(&home) {
        format!("~{}", &cwd[home.len()..])
    } else {
        cwd.to_owned()
    };
    // Show only the last component if it doesn't fit.
    if s.chars().count() <= max_chars {
        s
    } else {
        // Take up to max_chars chars from the end, prefixed with "…".
        let chars: Vec<char> = s.chars().collect();
        let start = chars.len().saturating_sub(max_chars - 1);
        format!("…{}", chars[start..].iter().collect::<String>())
    }
}

/// Extracts the text within a selection from `terminal_text` (rows separated by `\n`).
/// `anchor` and `end` are (row, col) in display coordinates; order does not matter.
pub(crate) fn extract_selection(
    terminal_text: &str,
    anchor: (usize, usize),
    end: (usize, usize),
) -> String {
    let (start_row, start_col, end_row, end_col) = if anchor <= end {
        (anchor.0, anchor.1, end.0, end.1)
    } else {
        (end.0, end.1, anchor.0, anchor.1)
    };
    let lines: Vec<&str> = terminal_text.split('\n').collect();
    let mut result = String::new();
    for row in start_row..=end_row {
        let Some(line) = lines.get(row) else {
            break;
        };
        let chars: Vec<char> = line.chars().collect();
        let from = if row == start_row {
            start_col.min(chars.len())
        } else {
            0
        };
        let to = if row == end_row {
            (end_col + 1).min(chars.len())
        } else {
            chars.len()
        };
        let segment: String = chars[from..to].iter().filter(|&&c| c != '\0').collect();
        result.push_str(segment.trim_end());
        if row < end_row {
            result.push('\n');
        }
    }
    result
}

/// Returns the text of the line that contains `cursor`, from the line's start
/// up to `cursor` (exclusive). Does not include the `'\n'`.
pub(crate) fn current_line_prefix(text: &str, cursor: usize) -> &str {
    let cursor = cursor.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    // Strip only leading whitespace (editor indent). Trailing whitespace is
    // semantically meaningful — e.g. "cd " signals that the user is about to
    // type a path argument and suggestion lookup needs the trailing space.
    text[line_start..cursor].trim_start()
}

/// Returns the leading whitespace of the line containing `cursor`.
/// Used when confirming a suggestion so that indentation is preserved.
pub(crate) fn line_leading_spaces(text: &str, cursor: usize) -> &str {
    let cursor = cursor.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let raw = &text[line_start..cursor];
    let trimmed = raw.trim_start();
    &raw[..raw.len() - trimmed.len()]
}

/// Returns `true` when `cursor` sits at the end of its line — i.e. the next
/// byte is `'\n'` or `cursor` is at the very end of `text`.
pub(crate) fn cursor_at_line_end(text: &str, cursor: usize) -> bool {
    cursor == text.len() || text.as_bytes().get(cursor) == Some(&b'\n')
}

// ── Terminal link detection ───────────────────────────────────────────────────

/// Scans `terminal_text` (rows separated by `\n`) and returns all link-like
/// spans as `(row, col_start, col_end, target)` where the column indices are
/// char-based (consistent with the rest of the selection system).
///
/// Detected patterns: `https://`, `http://`, `ftp://` URLs; `~/…` tilde paths;
/// `./…` and `../…` relative paths; `/absolute/…` paths at a word boundary.
pub(crate) fn detect_terminal_links(
    text: &str,
    term_cols: usize,
) -> Vec<(usize, usize, usize, String)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut result = Vec::new();

    // First pass: detect links within each line independently (original behavior).
    for (row, line) in lines.iter().enumerate() {
        scan_links_in_line(line, row, &mut result);
    }

    // Second pass: stitch links that were cut at a line boundary due to terminal
    // wrapping. The terminal grid pads every row to exactly `term_cols` cells with
    // spaces, so a URL that fills the row to its last non-space character may
    // continue on the next row starting at column 0. URLs can wrap across more
    // than two rows, so we follow the chain until the continuation stops.
    if term_cols == 0 {
        return result;
    }
    let mut extra: Vec<(usize, usize, usize, String)> = Vec::new();
    for entry in result.iter_mut() {
        let (row, _start, end, url) = entry;
        let mut cur_row = *row;
        let mut cur_end = *end;
        let mut accumulated = url.clone();
        // Collect continuation segments before we know the final URL.
        let mut segments: Vec<(usize, usize, usize)> = Vec::new();

        loop {
            let trimmed_len = lines[cur_row].trim_end().chars().count();
            if cur_end < trimmed_len || cur_row + 1 >= lines.len() {
                break;
            }
            let next_chars: Vec<char> = lines[cur_row + 1].chars().collect();
            let cont_end = scan_url_end(&next_chars, 0);
            if cont_end == 0 {
                break;
            }
            let suffix: String = next_chars[..cont_end].iter().collect();
            accumulated.push_str(&suffix);
            segments.push((cur_row + 1, 0, cont_end));
            cur_row += 1;
            cur_end = cont_end;
        }

        if !segments.is_empty() {
            // Now that we know the full URL, stamp it on all segments uniformly.
            *url = accumulated.clone();
            for (r, cs, ce) in segments {
                extra.push((r, cs, ce, accumulated.clone()));
            }
        }
    }
    result.extend(extra);
    result
}

/// Strips a trailing `:line` or `:line:col` suffix so the bare file path can
/// be passed to `open` / `xdg-open`.  Returns a sub-slice of `path`.
pub(crate) fn strip_line_col(path: &str) -> &str {
    let bytes = path.as_bytes();
    let mut end = bytes.len();
    for _ in 0..2 {
        let mut i = end;
        while i > 0 && bytes[i - 1].is_ascii_digit() {
            i -= 1;
        }
        if i < end && i > 0 && bytes[i - 1] == b':' {
            end = i - 1;
        } else {
            break;
        }
    }
    &path[..end]
}

/// Expands a leading `~/` to `$HOME/` so `std::process::Command` (which does
/// not run through a shell) can handle tilde paths.
pub(crate) fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        if home.is_empty() {
            return path.to_owned();
        }
        format!("{}/{}", home.trim_end_matches('/'), rest)
    } else {
        path.to_owned()
    }
}

fn scan_links_in_line(line: &str, row: usize, out: &mut Vec<(usize, usize, usize, String)>) {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        if let Some((len, target)) = try_link_at(&chars, i) {
            out.push((row, i, i + len, target));
            i += len;
        } else {
            i += 1;
        }
    }
}

/// Returns `(length_in_chars, target_string)` if a link begins at `chars[i]`.
fn try_link_at(chars: &[char], i: usize) -> Option<(usize, String)> {
    let n = chars.len();

    // URL: https:// http:// ftp://
    for scheme in &["https://", "http://", "ftp://"] {
        let sc: Vec<char> = scheme.chars().collect();
        if char_starts_with(chars, i, &sc) && i + sc.len() < n {
            let end = scan_url_end(chars, i + sc.len());
            if end > i + sc.len() {
                return Some((end - i, chars[i..end].iter().collect()));
            }
        }
    }

    // ~/path
    if i + 1 < n && chars[i] == '~' && chars[i + 1] == '/' {
        let path_end = scan_path_end(chars, i + 2);
        let end = scan_line_col_suffix(chars, path_end);
        if end > i + 2 {
            return Some((end - i, chars[i..end].iter().collect()));
        }
    }

    // ../path
    if i + 2 < n && chars[i] == '.' && chars[i + 1] == '.' && chars[i + 2] == '/' {
        let path_end = scan_path_end(chars, i + 3);
        let end = scan_line_col_suffix(chars, path_end);
        if end > i + 3 {
            return Some((end - i, chars[i..end].iter().collect()));
        }
    }

    // ./path
    if i + 1 < n && chars[i] == '.' && chars[i + 1] == '/' {
        let path_end = scan_path_end(chars, i + 2);
        let end = scan_line_col_suffix(chars, path_end);
        if end > i + 2 {
            return Some((end - i, chars[i..end].iter().collect()));
        }
    }

    // /absolute/path — only at a word boundary and followed by alnum or '_'
    if chars[i] == '/' {
        let at_boundary = i == 0 || is_link_boundary(chars[i - 1]);
        if at_boundary && i + 1 < n {
            let next = chars[i + 1];
            if next.is_alphanumeric() || next == '_' {
                let path_end = scan_path_end(chars, i + 1);
                let end = scan_line_col_suffix(chars, path_end);
                if end > i + 1 {
                    return Some((end - i, chars[i..end].iter().collect()));
                }
            }
        }
    }

    // bare/relative/path — e.g. "crates/src/lib.rs" or "src/main.rs:42:5"
    // Must be at a word boundary, start with alnum/'_', and have content after a '/'.
    if chars[i].is_alphanumeric() || chars[i] == '_' {
        let at_boundary = i == 0 || is_link_boundary(chars[i - 1]);
        if at_boundary {
            // Find the end of the first segment (no '/' yet).
            let mut j = i;
            while j < n && (chars[j].is_alphanumeric() || matches!(chars[j], '_' | '-' | '.')) {
                j += 1;
            }
            // Require: first segment followed by '/', with content after it.
            if j < n && chars[j] == '/' && j > i {
                let path_end = scan_path_end(chars, i);
                let end = scan_line_col_suffix(chars, path_end);
                if end > j + 1 {
                    return Some((end - i, chars[i..end].iter().collect()));
                }
            }
        }
    }

    None
}

fn char_starts_with(chars: &[char], pos: usize, prefix: &[char]) -> bool {
    let end = pos + prefix.len();
    end <= chars.len() && chars[pos..end] == *prefix
}

fn is_link_boundary(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t' | '(' | '[' | '<' | ':' | '=' | '"' | '\'' | ',' | ';' | '{' | '}'
    )
}

fn is_path_char(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            '/' | '.' | '_' | '-' | '+' | '~' | '@' | '%' | '&' | '=' | '?' | '#'
        )
}

fn scan_url_end(chars: &[char], start: usize) -> usize {
    let mut end = start;
    while end < chars.len() && !chars[end].is_whitespace() && chars[end] >= ' ' {
        end += 1;
    }
    // Trim trailing punctuation that typically trails the URL in prose.
    while end > start
        && matches!(
            chars[end - 1],
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '>' | '\'' | '"'
        )
    {
        end -= 1;
    }
    end
}

fn scan_path_end(chars: &[char], start: usize) -> usize {
    let mut end = start;
    while end < chars.len() && is_path_char(chars[end]) {
        end += 1;
    }
    // Trim trailing punctuation.
    while end > start
        && matches!(
            chars[end - 1],
            '.' | ',' | ':' | ';' | ')' | ']' | '\'' | '"'
        )
    {
        end -= 1;
    }
    end
}

/// After a path, consume an optional `:line` or `:line:col` suffix (e.g. `42` in `file.rs:42:5`).
fn scan_line_col_suffix(chars: &[char], start: usize) -> usize {
    let mut end = start;
    for _ in 0..2 {
        if end < chars.len() && chars[end] == ':' {
            let d_start = end + 1;
            let mut d_end = d_start;
            while d_end < chars.len() && chars[d_end].is_ascii_digit() {
                d_end += 1;
            }
            if d_end > d_start {
                end = d_end;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    end
}

/// Replaces the content of the line containing `cursor` with `new_line`.
/// Returns `(new_full_text, new_cursor)` where `new_cursor` points just after
/// the end of the replaced content (ready for the next edit or confirmation).
pub(crate) fn replace_cursor_line(text: &str, cursor: usize, new_line: &str) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[cursor..]
        .find('\n')
        .map(|i| cursor + i)
        .unwrap_or(text.len());
    let new_text = format!("{}{}{}", &text[..line_start], new_line, &text[line_end..]);
    let new_cursor = line_start + new_line.len();
    (new_text, new_cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── clamp_editor_scroll_offset ────────────────────────────────────────

    #[test]
    fn clamp_scroll_keeps_offset_when_cursor_in_view() {
        assert_eq!(clamp_editor_scroll_offset(5, 10, 0), 0);
    }

    #[test]
    fn clamp_scroll_jumps_up_when_cursor_above_view() {
        assert_eq!(clamp_editor_scroll_offset(2, 10, 5), 2);
    }

    #[test]
    fn clamp_scroll_scrolls_down_when_cursor_below_view() {
        // visible rows = 10, cursor on row 15 → offset must be 6 (rows 6..=15).
        assert_eq!(clamp_editor_scroll_offset(15, 10, 0), 6);
    }

    // ── editor_row_col_to_offset ─────────────────────────────────────────

    #[test]
    fn row_col_to_offset_first_line() {
        assert_eq!(editor_row_col_to_offset("hello\nworld", 0, 3), 3);
    }

    #[test]
    fn row_col_to_offset_second_line() {
        // "hello\n" = 6 bytes, "wo" = 2 → offset 8.
        assert_eq!(editor_row_col_to_offset("hello\nworld", 1, 2), 8);
    }

    #[test]
    fn row_col_to_offset_clamps_overshoot_column() {
        // col past end of line clamps to line end (5).
        assert_eq!(editor_row_col_to_offset("hello\nworld", 0, 100), 5);
    }

    #[test]
    fn row_col_to_offset_clamps_overshoot_row() {
        // row past last line clamps to last line.
        assert_eq!(editor_row_col_to_offset("a\nb\nc", 99, 1), 5);
    }

    #[test]
    fn row_col_to_offset_empty_text() {
        assert_eq!(editor_row_col_to_offset("", 0, 0), 0);
    }

    #[test]
    fn row_col_to_offset_unicode() {
        // "á" is 2 bytes; col=1 should land after it (byte 2).
        assert_eq!(editor_row_col_to_offset("áb", 0, 1), 2);
    }

    // ── editor_cursor_row_col ────────────────────────────────────────────

    #[test]
    fn cursor_row_col_at_start() {
        assert_eq!(editor_cursor_row_col("hello", 0), (0, 0));
    }

    #[test]
    fn cursor_row_col_on_third_line() {
        assert_eq!(editor_cursor_row_col("a\nb\nc", 4), (2, 0));
    }

    #[test]
    fn cursor_row_col_offset_past_end_is_clamped() {
        assert_eq!(editor_cursor_row_col("ab", 999), (0, 2));
    }

    #[test]
    fn cursor_row_col_counts_characters_not_bytes() {
        // "ñ" (U+00F1) = 2 bytes; column count is in chars.
        assert_eq!(editor_cursor_row_col("ñx", 3), (0, 2));
    }

    // ── editor horizontal scroll and word selection ──────────────────────

    #[test]
    fn horizontal_scroll_keeps_caret_visible() {
        assert_eq!(clamp_editor_horizontal_scroll_offset(20, 10, 0), 11);
        assert_eq!(clamp_editor_horizontal_scroll_offset(4, 10, 11), 4);
        assert_eq!(clamp_editor_horizontal_scroll_offset(8, 10, 4), 4);
    }

    #[test]
    fn editor_word_bounds_selects_code_identifier() {
        let text = "echo certificate_name.pem";
        assert_eq!(editor_word_bounds(text, 10), (5, 21));
        assert_eq!(
            &text[editor_word_bounds(text, 10).0..editor_word_bounds(text, 10).1],
            "certificate_name"
        );
    }

    #[test]
    fn editor_word_bounds_handles_unicode_and_punctuation() {
        let text = "echo café.pem";
        assert_eq!(
            &text[editor_word_bounds(text, 7).0..editor_word_bounds(text, 7).1],
            "café"
        );
        assert_eq!(
            &text[editor_word_bounds(text, 10).0..editor_word_bounds(text, 10).1],
            "."
        );
    }

    // ── cursor_to_terminal_cell ──────────────────────────────────────────

    #[test]
    fn cursor_outside_pane_returns_none() {
        // cy above the terminal pane → None
        assert!(
            cursor_to_terminal_cell(
                100.0,
                5.0,
                800,
                600,
                &TerminalLayout {
                    split_ratio: 0.5,
                    cell_w_px: 8.0,
                    cell_h_px: 16.0,
                    term_row_count: 24,
                    tab_bar_h: 30.0,
                    pad_h: 4.0,
                    pad_v: 4.0,
                }
            )
            .is_none()
        );
    }

    #[test]
    fn cursor_in_scrollbar_returns_none() {
        // cx in the rightmost SCROLLBAR_W_PX pixels → None
        assert!(
            cursor_to_terminal_cell(
                795.0,
                100.0,
                800,
                600,
                &TerminalLayout {
                    split_ratio: 0.5,
                    cell_w_px: 8.0,
                    cell_h_px: 16.0,
                    term_row_count: 24,
                    tab_bar_h: 30.0,
                    pad_h: 4.0,
                    pad_v: 4.0,
                }
            )
            .is_none()
        );
    }

    #[test]
    fn cursor_in_pane_returns_row_col() {
        let res = cursor_to_terminal_cell(
            50.0,
            100.0,
            800,
            600,
            &TerminalLayout {
                split_ratio: 0.5,
                cell_w_px: 8.0,
                cell_h_px: 16.0,
                term_row_count: 24,
                tab_bar_h: 30.0,
                pad_h: 0.0,
                pad_v: 0.0,
            },
        );
        assert!(res.is_some());
    }

    // ── extract_selection ─────────────────────────────────────────────────

    #[test]
    fn extract_selection_single_line() {
        let text = "hello world";
        // Select "ello" — char positions (0, 1) to (0, 4), inclusive
        let s = extract_selection(text, (0, 1), (0, 4));
        assert_eq!(s, "ello");
    }

    #[test]
    fn extract_selection_multi_line() {
        let text = "abc\ndef\nghi";
        // Select from (0, 1) to (2, 1) → "bc\ndef\ngh"
        let s = extract_selection(text, (0, 1), (2, 1));
        assert_eq!(s, "bc\ndef\ngh");
    }

    #[test]
    fn extract_selection_handles_reversed_anchor() {
        let text = "abc\ndef";
        // Anchor after end → still normalises; both orderings must agree.
        let s1 = extract_selection(text, (1, 1), (0, 0));
        let s2 = extract_selection(text, (0, 0), (1, 1));
        assert_eq!(s1, s2);
    }

    // ── current_line_prefix ──────────────────────────────────────────────

    #[test]
    fn current_line_prefix_strips_leading_whitespace() {
        assert_eq!(current_line_prefix("  git status", 12), "git status");
    }

    #[test]
    fn current_line_prefix_only_last_line() {
        let text = "first\nsecond cmd";
        assert_eq!(current_line_prefix(text, text.len()), "second cmd");
    }

    // ── line_leading_spaces ──────────────────────────────────────────────

    #[test]
    fn leading_spaces_returns_indent() {
        assert_eq!(line_leading_spaces("    code", 8), "    ");
    }

    #[test]
    fn leading_spaces_no_indent() {
        assert_eq!(line_leading_spaces("code", 4), "");
    }

    // ── editor line boundaries ───────────────────────────────────────────

    #[test]
    fn line_start_and_end_offsets_stay_on_current_line() {
        let text = "first\nsecond line\nthird";
        assert_eq!(editor_line_start_offset(text, 10), 6);
        assert_eq!(editor_line_end_offset(text, 10), 17);
    }

    #[test]
    fn line_boundaries_handle_first_last_and_unicode_lines() {
        let text = "héllo\nworld";
        assert_eq!(editor_line_start_offset(text, 3), 0);
        assert_eq!(editor_line_end_offset(text, 3), 6);
        assert_eq!(editor_line_start_offset(text, text.len()), 7);
        assert_eq!(editor_line_end_offset(text, text.len()), text.len());
    }

    // ── cursor_at_line_end ───────────────────────────────────────────────

    #[test]
    fn cursor_at_end_of_text_is_line_end() {
        assert!(cursor_at_line_end("abc", 3));
    }

    #[test]
    fn cursor_before_newline_is_line_end() {
        assert!(cursor_at_line_end("abc\ndef", 3));
    }

    #[test]
    fn cursor_mid_line_is_not_line_end() {
        assert!(!cursor_at_line_end("abc", 1));
    }

    // ── detect_terminal_links ────────────────────────────────────────────

    #[test]
    fn detects_https_link() {
        let links = detect_terminal_links("visit https://example.com today", 0);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].3, "https://example.com");
    }

    #[test]
    fn detects_multiple_link_schemes() {
        let text = "http://a.org\nftp://b.org";
        let links = detect_terminal_links(text, 0);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].0, 0); // row 0
        assert_eq!(links[1].0, 1); // row 1
    }

    #[test]
    fn ignores_non_url_tokens() {
        let links = detect_terminal_links("plain text here", 0);
        assert!(links.is_empty());
    }
}
