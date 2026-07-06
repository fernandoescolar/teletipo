//! Keyboard handling for copy mode (scrollback-driven selection).
//!
//! Copy mode allows users to navigate the terminal scrollback with keyboard
//! and select/copy text without using the mouse.

use crate::GpuRuntimeState;
use platform_abstraction::{KeyboardEvent, LogicalKey, NamedKey};

/// Handle a key event while in copy mode.
/// Returns true if the key was handled, false otherwise.
pub(super) fn handle_copy_mode_key(state: &mut GpuRuntimeState, key_event: &KeyboardEvent) -> bool {
    let logical_key = &key_event.logical_key;

    // Exit copy mode: Escape
    if matches!(logical_key, LogicalKey::Named(NamedKey::Escape)) {
        state.tab_mut().copy_mode.active = false;
        state.tab_mut().copy_mode.anchor = None;
        return true;
    }

    // Dispatch to sub-handlers by key category
    try_handle_movement(state, logical_key)
        || try_handle_selection(state, logical_key)
        || try_handle_copy_and_exit(state, logical_key)
        || try_handle_word_nav(state, logical_key)
        || try_handle_line_nav(state, logical_key)
        || try_handle_scrollback_nav(state, logical_key)
        || try_handle_page_nav(state, key_event)
}

fn try_handle_movement(state: &mut GpuRuntimeState, logical_key: &LogicalKey) -> bool {
    match logical_key {
        LogicalKey::Character(ch) if ch.as_str() == "h" || ch.as_str() == "H" => {
            cursor_move_left(state);
            true
        }
        LogicalKey::Character(ch) if ch.as_str() == "j" || ch.as_str() == "J" => {
            cursor_move_down(state);
            true
        }
        LogicalKey::Character(ch) if ch.as_str() == "k" || ch.as_str() == "K" => {
            cursor_move_up(state);
            true
        }
        LogicalKey::Character(ch) if ch.as_str() == "l" || ch.as_str() == "L" => {
            cursor_move_right(state);
            true
        }
        LogicalKey::Named(NamedKey::ArrowLeft) => {
            cursor_move_left(state);
            true
        }
        LogicalKey::Named(NamedKey::ArrowDown) => {
            cursor_move_down(state);
            true
        }
        LogicalKey::Named(NamedKey::ArrowUp) => {
            cursor_move_up(state);
            true
        }
        LogicalKey::Named(NamedKey::ArrowRight) => {
            cursor_move_right(state);
            true
        }
        _ => false,
    }
}

fn try_handle_selection(state: &mut GpuRuntimeState, logical_key: &LogicalKey) -> bool {
    if let LogicalKey::Character(ch) = logical_key {
        let ch_str = ch.as_str();
        if ch_str == "v" || ch_str == "V" {
            let cursor_row = state.tab().copy_mode.cursor_row;
            let cursor_col = state.tab().copy_mode.cursor_col;
            if state.tab().copy_mode.anchor.is_some() {
                state.tab_mut().copy_mode.anchor = None;
            } else {
                state.tab_mut().copy_mode.anchor = Some((cursor_row, cursor_col));
            }
            return true;
        }
    }
    false
}

fn try_handle_copy_and_exit(state: &mut GpuRuntimeState, logical_key: &LogicalKey) -> bool {
    if let LogicalKey::Character(ch) = logical_key {
        let ch_str = ch.as_str();
        if ch_str == "y" || ch_str == "Y" {
            copy_selection_and_exit(state);
            return true;
        }
    }
    if matches!(logical_key, LogicalKey::Named(NamedKey::Enter)) {
        copy_selection_and_exit(state);
        return true;
    }
    false
}

fn try_handle_word_nav(state: &mut GpuRuntimeState, logical_key: &LogicalKey) -> bool {
    if let LogicalKey::Character(ch) = logical_key {
        match ch.as_str() {
            "w" | "W" => {
                cursor_jump_word_forward(state);
                return true;
            }
            "b" | "B" => {
                cursor_jump_word_backward(state);
                return true;
            }
            _ => {}
        }
    }
    false
}

fn try_handle_line_nav(state: &mut GpuRuntimeState, logical_key: &LogicalKey) -> bool {
    if let LogicalKey::Character(ch) = logical_key {
        match ch.as_str() {
            "0" => {
                state.tab_mut().copy_mode.cursor_col = 0;
                return true;
            }
            "$" => {
                cursor_move_to_line_end(state);
                return true;
            }
            _ => {}
        }
    }
    false
}

fn try_handle_scrollback_nav(state: &mut GpuRuntimeState, logical_key: &LogicalKey) -> bool {
    if let LogicalKey::Character(ch) = logical_key {
        match ch.as_str() {
            "g" => {
                cursor_move_to_scrollback_top(state);
                return true;
            }
            "G" => {
                cursor_move_to_scrollback_bottom(state);
                return true;
            }
            _ => {}
        }
    }
    false
}

fn try_handle_page_nav(state: &mut GpuRuntimeState, key_event: &KeyboardEvent) -> bool {
    if !state.modifiers.ctrl_down {
        return false;
    }
    if let LogicalKey::Character(ch) = &key_event.logical_key {
        match ch.as_str() {
            "u" | "U" => {
                cursor_move_page_up(state);
                return true;
            }
            "d" | "D" => {
                cursor_move_page_down(state);
                return true;
            }
            _ => {}
        }
    }
    false
}

// ─────────────────────────────────────────────────────────────────────────────
// Cursor movement implementations
// ─────────────────────────────────────────────────────────────────────────────

fn cursor_move_left(state: &mut GpuRuntimeState) {
    let col = &mut state.tab_mut().copy_mode.cursor_col;
    if *col > 0 {
        *col -= 1;
    }
}

fn cursor_move_right(state: &mut GpuRuntimeState) {
    let col = &mut state.tab_mut().copy_mode.cursor_col;
    // TODO: clamp to actual line width from screen
    *col = col.saturating_add(1);
}

fn cursor_move_up(state: &mut GpuRuntimeState) {
    let scrollback_len = state.tab().app.scrollback_len() as isize;
    let row = &mut state.tab_mut().copy_mode.cursor_row;
    if *row > -scrollback_len {
        *row -= 1;
    }
}

fn cursor_move_down(state: &mut GpuRuntimeState) {
    let row = &mut state.tab_mut().copy_mode.cursor_row;
    if *row < 0 {
        *row += 1;
    }
}

fn cursor_jump_word_forward(state: &mut GpuRuntimeState) {
    // TODO: implement word boundary detection
    // For now, move right by 8 cells (rough word size)
    let col = &mut state.tab_mut().copy_mode.cursor_col;
    *col = col.saturating_add(8);
}

fn cursor_jump_word_backward(state: &mut GpuRuntimeState) {
    // TODO: implement word boundary detection
    let col = &mut state.tab_mut().copy_mode.cursor_col;
    *col = col.saturating_sub(8);
}

fn cursor_move_to_line_end(state: &mut GpuRuntimeState) {
    // TODO: get actual line width from screen
    state.tab_mut().copy_mode.cursor_col = 200; // placeholder
}

fn cursor_move_to_scrollback_top(state: &mut GpuRuntimeState) {
    let scrollback_len = state.tab().app.scrollback_len() as isize;
    state.tab_mut().copy_mode.cursor_row = -scrollback_len;
    state.tab_mut().copy_mode.cursor_col = 0;
}

fn cursor_move_to_scrollback_bottom(state: &mut GpuRuntimeState) {
    state.tab_mut().copy_mode.cursor_row = 0;
    state.tab_mut().copy_mode.cursor_col = 0;
}

fn cursor_move_page_up(state: &mut GpuRuntimeState) {
    let rows = state.tab().term_row_count as isize;
    let half = rows / 2;
    let scrollback_len = state.tab().app.scrollback_len() as isize;
    let row = &mut state.tab_mut().copy_mode.cursor_row;
    *row -= half;
    if *row < -scrollback_len {
        *row = -scrollback_len;
    }
}

fn cursor_move_page_down(state: &mut GpuRuntimeState) {
    let rows = state.tab().term_row_count as isize;
    let half = rows / 2;
    let row = &mut state.tab_mut().copy_mode.cursor_row;
    *row += half;
    if *row > 0 {
        *row = 0;
    }
}

fn copy_selection_and_exit(state: &mut GpuRuntimeState) {
    let copy_mode = &state.tab().copy_mode;
    if let Some((anchor_row, anchor_col)) = copy_mode.anchor {
        let cursor_row = copy_mode.cursor_row;
        let cursor_col = copy_mode.cursor_col;

        // Normalize selection: start at (min_row, min_col), end at (max_row, max_col)
        let (start_row, start_col, end_row, end_col) =
            if anchor_row > cursor_row || (anchor_row == cursor_row && anchor_col > cursor_col) {
                (cursor_row, cursor_col, anchor_row, anchor_col)
            } else {
                (anchor_row, anchor_col, cursor_row, cursor_col)
            };

        // Extract actual text from screen using selection bounds
        let selected_text = extract_selected_text(state, start_row, start_col, end_row, end_col);

        state.shell_services.clipboard_set(selected_text);
        state.push_toast("Selection copied", crate::state::ToastKind::Success);
    }

    state.tab_mut().copy_mode.active = false;
    state.tab_mut().copy_mode.anchor = None;
}

/// Extract text from the terminal screen between the given scrollback-relative coordinates.
///
/// Coordinates are scrollback-relative (isize):
/// - 0 = grid bottom (current cursor row)
/// - negative = scrollback (e.g., -1 = one line above grid)
/// - positive = should not occur in normal copy mode usage
fn extract_selected_text(
    state: &GpuRuntimeState,
    start_row: isize,
    start_col: usize,
    end_row: isize,
    end_col: usize,
) -> String {
    // Get full terminal content (scrollback + visible grid)
    let full_text = state.tab().app.terminal_snapshot_with_scrollback();
    let lines: Vec<&str> = full_text.lines().collect();

    // Total rows = scrollback + visible grid
    let total_rows = lines.len();
    if total_rows == 0 {
        return String::new();
    }

    let scrollback_len = state.tab().app.scrollback_len() as isize;

    // Convert scrollback-relative coordinates to absolute row indices
    // scrollback-relative: 0 = grid bottom, -1 = one line above grid
    // absolute: 0 = oldest line in full_text
    let abs_start_row = (scrollback_len + start_row).max(0) as usize;
    let abs_end_row = (scrollback_len + end_row).max(0) as usize;

    // Clamp to valid range
    let abs_start_row = abs_start_row.min(lines.len().saturating_sub(1));
    let abs_end_row = abs_end_row.min(lines.len().saturating_sub(1));

    let mut result = String::new();

    // Extract text row by row
    for abs_row in abs_start_row..=abs_end_row {
        if abs_row >= lines.len() {
            break;
        }

        let line = lines[abs_row];
        let col_start = if abs_row == abs_start_row {
            start_col
        } else {
            0
        };
        let col_end = if abs_row == abs_end_row {
            end_col.min(line.len())
        } else {
            line.len()
        };

        // Extract characters from the line
        let chars: Vec<char> = line.chars().collect();
        let col_start = col_start.min(chars.len());
        let col_end = col_end.min(chars.len());

        if col_start < col_end {
            for ch in &chars[col_start..col_end] {
                result.push(*ch);
            }
        }

        // Add newline between rows (but not after the last row)
        if abs_row < abs_end_row {
            result.push('\n');
        }
    }

    result
}
