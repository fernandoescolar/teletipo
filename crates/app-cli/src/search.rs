use crate::tab::TabState;

/// Panel geometry tuning expressed in grid units.
pub(crate) const PANEL_WIDTH_CELLS: f32 = 34.0;
pub(crate) const PANEL_HEIGHT_CELLS: f32 = 1.6;
pub(crate) const PANEL_BUTTON_CELLS: f32 = 2.0;

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
    tab.search.matches = compute_matches(&full_text, &tab.search.query);
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

fn compute_matches(text: &str, query: &str) -> Vec<SearchMatch> {
    let needle = query.trim();
    if needle.is_empty() {
        return Vec::new();
    }

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

    out
}
