use render_wgpu::SCROLLBAR_W_PX;

use crate::GpuRuntimeState;

/// Adjusts `state.editor_scroll_offset` to keep the caret row visible.
pub(crate) fn clamp_editor_scroll(state: &mut GpuRuntimeState) {
    let tab_bar_h = state.tab_bar_h();
    let window_height = state.window_height;
    let cell_h = state.cell_h;
    let active = state.active_tab;
    let tab = &mut state.tabs[active];
    let text = tab.app.editor_snapshot();
    let offset = tab.app.editor_cursor_offset();
    let caret_row = text[..offset.min(text.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count();
    let editor_h_px = (1.0 - tab.split_ratio) * (window_height as f32 - tab_bar_h);
    let visible_rows = if cell_h > 0.0 {
        (editor_h_px / cell_h).floor().max(1.0) as usize
    } else {
        1
    };
    if caret_row < tab.editor_scroll_offset {
        tab.editor_scroll_offset = caret_row;
    } else if caret_row >= tab.editor_scroll_offset + visible_rows {
        tab.editor_scroll_offset = caret_row + 1 - visible_rows;
    }
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

/// Converts a cursor physical-pixel position to a terminal grid (row, col).
/// Returns `None` if the cursor is outside the terminal pane or in the scrollbar area.
/// Uses `term_row_count` to compute the bottom-alignment offset so the mapping
/// matches what the renderer draws.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cursor_to_terminal_cell(
    cx: f64,
    cy: f64,
    window_width: u32,
    window_height: u32,
    split_ratio: f32,
    cell_w_px: f32,
    cell_h_px: f32,
    term_row_count: usize,
    tab_bar_h: f32,
) -> Option<(usize, usize)> {
    let cell_w = cell_w_px as f64;
    let cell_h = cell_h_px as f64;
    let tbh = tab_bar_h as f64;
    let available_h = window_height as f64 - tbh;
    let terminal_h = available_h * split_ratio as f64;
    let content_h = (term_row_count as f64 * cell_h).min(terminal_h);
    let term_top_y = tbh + (terminal_h - content_h).max(0.0);
    let term_bottom_y = tbh + terminal_h;
    let scrollbar_x = window_width as f64 - SCROLLBAR_W_PX as f64;
    if cy < term_top_y || cy >= term_bottom_y || cx < 0.0 || cx >= scrollbar_x {
        return None;
    }
    Some((((cy - term_top_y) / cell_h) as usize, (cx / cell_w) as usize))
}

/// Read the current working directory of an OS process.
/// Returns `None` if the pid is no longer alive or the OS call fails.
#[cfg(target_os = "macos")]
pub(crate) fn read_child_cwd(pid: u32) -> Option<String> {
    // Use macOS proc_pidinfo(PROC_PIDVNODEPATHINFO) to get the cwd path.
    // Struct sizes (verified against <sys/proc_info.h>):
    //   vinfo_stat       = 136 bytes
    //   vnode_info       = vinfo_stat(136) + vi_type(4) + vi_pad(4) + vi_fsid(8) = 152 bytes
    //   vnode_info_path  = vnode_info(152) + path[MAXPATHLEN=1024] = 1176 bytes
    //   proc_vnodepathinfo = pvi_cdir(1176) + pvi_rdir(1176) = 2352 bytes
    const PROC_PIDVNODEPATHINFO: i32 = 9;
    const BUF_SIZE: usize = 2352;
    const PATH_OFFSET: usize = 152; // sizeof(vnode_info)
    const MAXPATHLEN: usize = 1024;
    unsafe extern "C" {
        unsafe fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut u8,
            buffersize: i32,
        ) -> i32;
    }
    let mut buf = vec![0u8; BUF_SIZE];
    let ret = unsafe {
        proc_pidinfo(
            pid as i32,
            PROC_PIDVNODEPATHINFO,
            0,
            buf.as_mut_ptr(),
            BUF_SIZE as i32,
        )
    };
    if ret <= 0 {
        return None;
    }
    let path_bytes = &buf[PATH_OFFSET..PATH_OFFSET + MAXPATHLEN];
    let end = path_bytes.iter().position(|&b| b == 0).unwrap_or(MAXPATHLEN);
    String::from_utf8(path_bytes[..end].to_vec())
        .ok()
        .filter(|s| !s.is_empty())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn read_child_cwd(pid: u32) -> Option<String> {
    // On Linux read the cwd symlink from procfs.
    std::fs::read_link(format!("/proc/{}/cwd", pid))
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
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
        let segment: String = chars[from..to].iter().collect();
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
    &text[line_start..cursor]
}

/// Returns `true` when `cursor` sits at the end of its line — i.e. the next
/// byte is `'\n'` or `cursor` is at the very end of `text`.
pub(crate) fn cursor_at_line_end(text: &str, cursor: usize) -> bool {
    cursor == text.len() || text.as_bytes().get(cursor) == Some(&b'\n')
}

/// Replaces the content of the line containing `cursor` with `new_line`.
/// Returns `(new_full_text, new_cursor)` where `new_cursor` points just after
/// the end of the replaced content (ready for the next edit or confirmation).
pub(crate) fn replace_cursor_line(text: &str, cursor: usize, new_line: &str) -> (String, usize) {
    let cursor = cursor.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[cursor..].find('\n').map(|i| cursor + i).unwrap_or(text.len());
    let new_text = format!("{}{}{}", &text[..line_start], new_line, &text[line_end..]);
    let new_cursor = line_start + new_line.len();
    (new_text, new_cursor)
}
