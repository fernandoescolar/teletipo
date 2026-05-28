// Scrollbar width in physical pixels — must stay in sync with render-wgpu's
// `geometry::SCROLLBAR_W_PX` so that hit-testing is consistent with rendering.
const SCROLLBAR_W_PX: f32 = 10.0;

pub fn clamp_editor_scroll_offset(
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

pub fn editor_row_col_to_offset(text: &str, row: usize, col: usize) -> usize {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.is_empty() {
        return 0;
    }
    let clamped_row = row.min(lines.len() - 1);
    let line_start: usize = lines[..clamped_row].iter().map(|line| line.len() + 1).sum();
    let line = lines[clamped_row];
    let char_byte = line
        .char_indices()
        .nth(col)
        .map(|(idx, _)| idx)
        .unwrap_or(line.len());
    line_start + char_byte
}

pub fn editor_cursor_row_col(text: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(text.len());
    let before = &text[..clamped];
    let row = before.chars().filter(|ch| *ch == '\n').count();
    let col = match before.rfind('\n') {
        Some(pos) => before[pos + 1..].chars().count(),
        None => before.chars().count(),
    };
    (row, col)
}

#[allow(clippy::too_many_arguments)]
pub fn cursor_to_terminal_cell(
    cx: f64,
    cy: f64,
    window_width: u32,
    window_height: u32,
    split_ratio: f32,
    cell_w_px: f32,
    cell_h_px: f32,
    term_row_count: usize,
    tab_bar_h: f32,
    pad_h: f32,
    pad_v: f32,
) -> Option<(usize, usize)> {
    let cell_w = cell_w_px as f64;
    let cell_h = cell_h_px as f64;
    let tbh = tab_bar_h as f64;
    let available_h = window_height as f64 - tbh;
    let terminal_h = available_h * split_ratio as f64;
    let effective_term_h = (terminal_h - 2.0 * pad_v as f64).max(0.0);
    let content_h = (term_row_count as f64 * cell_h).min(effective_term_h);
    let term_top_y = tbh + (effective_term_h - content_h).max(0.0) + pad_v as f64;
    let term_bottom_y = tbh + terminal_h;
    let scrollbar_x = window_width as f64 - SCROLLBAR_W_PX as f64;

    if cy < term_top_y || cy >= term_bottom_y || cx < 0.0 || cx >= scrollbar_x {
        return None;
    }

    let col = ((cx - pad_h as f64) / cell_w).max(0.0) as usize;
    let row = ((cy - term_top_y) / cell_h).max(0.0) as usize;
    Some((row, col))
}

pub fn extract_selection(text: &str, anchor: (usize, usize), end: (usize, usize)) -> String {
    let (start_row, start_col, end_row, end_col) = if anchor <= end {
        (anchor.0, anchor.1, end.0, end.1)
    } else {
        (end.0, end.1, anchor.0, anchor.1)
    };

    let lines: Vec<&str> = text.split('\n').collect();
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
        let segment: String = chars[from..to].iter().collect();
        result.push_str(segment.trim_end());
        if row < end_row {
            result.push('\n');
        }
    }
    result
}

pub fn current_line_prefix(text: &str, cursor: usize) -> &str {
    let cursor = cursor.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    text[line_start..cursor].trim()
}

pub fn line_leading_spaces(text: &str, cursor: usize) -> &str {
    let cursor = cursor.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
    let raw = &text[line_start..cursor];
    let trimmed = raw.trim_start();
    &raw[..raw.len() - trimmed.len()]
}

pub fn cursor_at_line_end(text: &str, cursor: usize) -> bool {
    cursor == text.len() || text.as_bytes().get(cursor) == Some(&b'\n')
}

pub fn detect_terminal_links(text: &str) -> Vec<(usize, usize, usize, String)> {
    let mut out = Vec::new();
    for (row, line) in text.split('\n').enumerate() {
        for token in line.split_whitespace() {
            if (token.starts_with("http://")
                || token.starts_with("https://")
                || token.starts_with("ftp://"))
                && let Some(start) = line.find(token)
            {
                let end = start + token.chars().count();
                out.push((row, start, end, token.to_owned()));
            }
        }
    }
    out
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
        // visible rows = 10, cursor on row 15 → offset must be 6 so 6..15 inclusive
        // (6, 7, 8, 9, 10, 11, 12, 13, 14, 15 = 10 rows).
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
        let text = "a\nb\nc";
        assert_eq!(editor_row_col_to_offset(text, 99, 1), 5); // "a\nb\nc" -> last line "c" pos
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
        let text = "ab";
        assert_eq!(editor_cursor_row_col(text, 999), (0, 2));
    }

    #[test]
    fn cursor_row_col_counts_characters_not_bytes() {
        // "ñ" (U+00F1) = 2 bytes; column count is in chars.
        assert_eq!(editor_cursor_row_col("ñx", 3), (0, 2));
    }

    // ── cursor_to_terminal_cell ──────────────────────────────────────────

    #[test]
    fn cursor_outside_pane_returns_none() {
        // cy above tab bar → None
        assert!(
            cursor_to_terminal_cell(100.0, 5.0, 800, 600, 0.5, 8.0, 16.0, 24, 30.0, 4.0, 4.0)
                .is_none()
        );
    }

    #[test]
    fn cursor_in_scrollbar_returns_none() {
        // cx in the rightmost SCROLLBAR_W_PX pixels → None
        assert!(
            cursor_to_terminal_cell(795.0, 100.0, 800, 600, 0.5, 8.0, 16.0, 24, 30.0, 4.0, 4.0)
                .is_none()
        );
    }

    #[test]
    fn cursor_in_pane_returns_row_col() {
        let res =
            cursor_to_terminal_cell(50.0, 100.0, 800, 600, 0.5, 8.0, 16.0, 24, 30.0, 0.0, 0.0);
        assert!(res.is_some());
    }

    // ── extract_selection ─────────────────────────────────────────────────

    #[test]
    fn extract_selection_single_line() {
        let text = "hello world";
        // Select "ello" — char positions (0, 1) to (0, 4)
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
        // anchor after end → still normalises.
        let s1 = extract_selection(text, (1, 1), (0, 0));
        let s2 = extract_selection(text, (0, 0), (1, 1));
        assert_eq!(s1, s2);
    }

    // ── current_line_prefix ──────────────────────────────────────────────

    #[test]
    fn current_line_prefix_strips_whitespace() {
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
        let links = detect_terminal_links("visit https://example.com today");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].3, "https://example.com");
    }

    #[test]
    fn detects_multiple_link_schemes() {
        let text = "http://a.org\nftp://b.org";
        let links = detect_terminal_links(text);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].0, 0); // row 0
        assert_eq!(links[1].0, 1); // row 1
    }

    #[test]
    fn ignores_non_url_tokens() {
        let links = detect_terminal_links("plain text here");
        assert!(links.is_empty());
    }
}
