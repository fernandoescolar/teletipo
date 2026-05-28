use render_wgpu::SCROLLBAR_W_PX;

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
            if token.starts_with("http://") || token.starts_with("https://") || token.starts_with("ftp://") {
                if let Some(start) = line.find(token) {
                    let end = start + token.chars().count();
                    out.push((row, start, end, token.to_owned()));
                }
            }
        }
    }
    out
}
