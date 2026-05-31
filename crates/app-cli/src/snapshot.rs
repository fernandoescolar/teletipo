use std::sync::Arc;

use crate::GpuRuntimeState;
use crate::coords::{
    current_line_prefix, cursor_at_line_end, cursor_to_terminal_cell, detect_terminal_links,
    read_child_cwd, shorten_cwd_label,
};
use crate::settings::build_settings_overlay;
use crate::theme;
use render_wgpu::{
    ColorTheme, DamageRegion, RenderCell, RenderRow, RenderSnapshot, SearchPanel,
    SuggestionDropdown, TabContextMenu, TerminalLink, Toast, ToastKind,
};

/// Truncate `s` to at most `max_chars` Unicode scalar values, appending `…`
/// if the string is longer.  Used to keep dropdown entries and ghost text
/// from overflowing the visible area.
fn truncate_display(s: &str, max_chars: usize) -> String {
    let mut char_indices = s.char_indices();
    match char_indices.nth(max_chars) {
        None => s.to_owned(),
        Some((byte_pos, _)) => format!("{}…", &s[..byte_pos]),
    }
}

fn tab_button_label(index: usize, title: Option<&str>, cwd: &str, max_chars: usize) -> String {
    let label_text = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(|title| truncate_display(title, max_chars))
        .unwrap_or_else(|| shorten_cwd_label(cwd, max_chars));
    format!("Cmd+{}  {}", index + 1, label_text)
}

fn tab_button_max_chars(tab_width_px: f32, cell_w_px: f32) -> usize {
    let shortcut_width_chars = 4.0;
    let close_button_width_chars = 2.0;
    let padding_chars = 2.0;
    let title_width_chars = (tab_width_px / cell_w_px).floor()
        - shortcut_width_chars
        - close_button_width_chars
        - padding_chars;
    title_width_chars.max(4.0) as usize
}

/// Convert the active `ThemeFile` (if any) to a `ColorTheme` for the renderer.
/// Falls back to `ColorTheme::default()` when no theme is selected.
pub(crate) fn theme_from_config(theme_file: Option<&theme::ThemeFile>) -> ColorTheme {
    let Some(tf) = theme_file else {
        return ColorTheme::default();
    };
    fn c(s: &str) -> [f32; 4] {
        crate::config::parse_color(s).unwrap_or([0.0, 0.0, 0.0, 1.0])
    }
    ColorTheme {
        terminal_bg: c(&tf.background),
        editor_bg: c(&tf.background),
        separator: c(&tf.terminal_colors.normal.black),
        separator_focused: c(&tf.accent),
        cursor: c(&tf.cursor),
        text: c(&tf.foreground),
        ansi_palette: theme::build_ansi_palette(tf),
    }
}

/// Build a complete `RenderSnapshot` from the current state for the frame closure.
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // gathers every layer's view of the world into one struct
pub(crate) fn build_snapshot(state: &mut GpuRuntimeState) -> RenderSnapshot {
    // Clear one-shot just_saved flag after it has been shown for a frame.
    if state.settings.just_saved {
        state.settings.just_saved = false;
    }

    // Poll the background update-check thread (once; then drop the receiver).
    if let Some(ref rx) = state.update_rx {
        match rx.try_recv() {
            Ok(Ok(Some(version))) => {
                state.overlays.pending_update = Some(crate::UpdateBanner::Available(version));
                state.update_rx = None;
            }
            Ok(Ok(None)) => {
                state.update_rx = None;
            }
            Ok(Err(err)) => {
                state.overlays.pending_update = Some(crate::UpdateBanner::Failed(err));
                state.update_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Thread exited without finding an update (rate-gate or error).
                state.update_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    let had_data = state.pump_all_ptys();
    if had_data {
        let active = state.active_tab;
        state.tabs[active].scroll_offset = 0;
        // Reset cursor blink to visible whenever terminal output arrives.
        state.overlays.cursor_blink_last = std::time::Instant::now();
        state.overlays.cursor_blink_phase = true;
    }

    // Advance cursor blink: toggle every 500 ms.
    if state.overlays.cursor_blink_last.elapsed().as_millis() >= crate::consts::BLINK_HALF_MS {
        state.overlays.cursor_blink_phase = !state.overlays.cursor_blink_phase;
        state.overlays.cursor_blink_last = std::time::Instant::now();
    }

    // The active tab is always "read" — clear any pending unread indicator.
    state.tabs[state.active_tab].unread_output = false;

    // Show a one-shot toast for config parse errors on startup.
    if let Some(err) = state.config_error.take() {
        state.push_toast(format!("Config error: {err}"), crate::state::ToastKind::Error);
    }

    let active = state.active_tab;
    let scroll_offset = state.tabs[active].scroll_offset;
    let active_palette: Option<[[f32; 3]; 16]> = state
        .themes_fonts
        .active_theme_idx
        .map(|i| theme::build_ansi_palette(&state.themes_fonts.available_themes[i]));
    let styled = state.tabs[active]
        .app
        .terminal_styled_snapshot_at_offset_with_palette(scroll_offset, active_palette.as_ref());
    let terminal_text: String = styled.iter().map(|(ch, _, _, _)| *ch).collect();
    let terminal_fg_colors: Vec<Option<[f32; 3]>> =
        styled.iter().map(|(_, fg, _, _)| *fg).collect();
    let terminal_bg_colors: Vec<Option<[f32; 3]>> =
        styled.iter().map(|(_, _, bg, _)| *bg).collect();
    let terminal_styles: Vec<u8> = styled.iter().map(|(_, _, _, s)| *s).collect();
    let mut terminal_rows: Vec<RenderRow> = Vec::new();
    let mut current_row: Vec<RenderCell> = Vec::new();
    for (ch, fg, bg, style) in &styled {
        if *ch == '\n' {
            terminal_rows.push(RenderRow {
                cells: std::mem::take(&mut current_row),
                dirty: false,
            });
            continue;
        }
        current_row.push(RenderCell {
            ch: *ch,
            fg: *fg,
            bg: *bg,
            style: *style,
        });
    }
    terminal_rows.push(RenderRow {
        cells: current_row,
        dirty: false,
    });

    let (term_rows, term_cols) = if let Some(first) = terminal_rows.first() {
        (terminal_rows.len(), first.cells.len())
    } else {
        (1usize, 0usize)
    };
    let mut damage = DamageRegion {
        full_redraw: false,
        dirty_rows: Vec::new(),
        cols: term_cols,
        dirty_cells: vec![false; term_rows.saturating_mul(term_cols)],
    };
    let screen_damage = state.tabs[active].app.terminal_take_damage();
    damage.full_redraw = screen_damage.full_redraw;
    damage.dirty_rows = screen_damage.dirty_rows.clone();
    for row in &screen_damage.dirty_rows {
        if let Some(render_row) = terminal_rows.get_mut(*row) {
            render_row.dirty = true;
        }
        for col in 0..term_cols {
            let idx = row.saturating_mul(term_cols).saturating_add(col);
            if idx < damage.dirty_cells.len() {
                damage.dirty_cells[idx] = true;
            }
        }
    }
    let terminal_damage = Arc::new(damage);
    state.tabs[active].last_terminal_text = terminal_text.clone();
    state.tabs[active].term_row_count = terminal_text.lines().count().max(1);

    if state.tabs[active].search.active {
        crate::search::refresh_search(&mut state.tabs[active]);
    }

    let (search_panel, search_highlights, search_current_highlight) =
        if state.tabs[active].search.active {
            let tab = &state.tabs[active];
            let visible_rows = tab.term_row_count.max(1);
            let total_rows = tab.search.total_rows.max(visible_rows);
            let window_start = total_rows
                .saturating_sub(visible_rows)
                .saturating_sub(tab.scroll_offset.min(tab.app.scrollback_len()));
            let window_end = window_start.saturating_add(visible_rows);

            let highlights: Vec<(usize, usize, usize)> = tab
                .search
                .matches
                .iter()
                .filter_map(|m| {
                    if m.abs_row >= window_start && m.abs_row < window_end {
                        Some((m.abs_row - window_start, m.col_start, m.col_end))
                    } else {
                        None
                    }
                })
                .collect();

            let current = tab.search.matches.get(tab.search.current).and_then(|m| {
                if m.abs_row >= window_start && m.abs_row < window_end {
                    Some((m.abs_row - window_start, m.col_start, m.col_end))
                } else {
                    None
                }
            });

            let current_match = if tab.search.matches.is_empty() {
                0
            } else {
                tab.search.current + 1
            };

            (
                Some(SearchPanel {
                    query: tab.search.query.clone(),
                    match_count: tab.search.matches.len(),
                    current_match,
                    regex_mode: tab.search.regex_mode,
                    case_sensitive: tab.search.case_sensitive,
                    error: tab.search.error.clone(),
                    cursor_char: tab.search.cursor_char_index(),
                    sel_char_range: tab.search.sel_char_range(),
                }),
                highlights,
                current,
            )
        } else {
            (None, Vec::new(), None)
        };
    // Underline only the link the cursor is currently hovering over.
    let terminal_links: Vec<TerminalLink> = {
        let all_links = detect_terminal_links(&terminal_text);
        if all_links.is_empty() {
            Vec::new()
        } else {
            let split_ratio = state.tabs[active].split_ratio;
            let tab_bar_h = state.tab_bar_h();
            let pad_h = state.user_config.padding.horizontal as f32;
            let pad_v = state.user_config.padding.vertical as f32;
            let term_row_count = state.tabs[active].term_row_count;
            if let Some((hover_row, hover_col)) = cursor_to_terminal_cell(
                state.cursor.cursor_x,
                state.cursor.cursor_y,
                state.layout.window_width,
                state.layout.window_height,
                split_ratio,
                state.layout.cell_w,
                state.layout.cell_h,
                term_row_count,
                tab_bar_h,
                pad_h,
                pad_v,
            ) {
                all_links
                    .into_iter()
                    .filter(|(r, cs, ce, _)| *r == hover_row && hover_col >= *cs && hover_col < *ce)
                    .map(|(row, col_start, col_end, target)| TerminalLink {
                        row,
                        col_start,
                        col_end,
                        target,
                    })
                    .collect()
            } else {
                Vec::new()
            }
        }
    };
    let editor_text = state.tabs[active].app.editor_snapshot();
    let editor_line_count = editor_text.lines().count().max(1);
    let editor_cursor_offset = state.tabs[active].app.editor_cursor_offset();

    // Ghost-text suggestion: the suffix of the most-recently-used history
    // entry (case-insensitive prefix match) that extends the current editor
    // text.  Not shown while Tab-cycling is active — the editor content
    // already shows the selected match in that case.
    let editor_suggestion = if let Some(idx) = state.tabs[active].suggestion_index {
        // Cycling in progress: the editor holds the prefix; display the selected
        // match's completion as gray ghost text so the user sees a live preview.
        let prefix = state.tabs[active]
            .suggestion_prefix
            .as_deref()
            .unwrap_or_else(|| current_line_prefix(&editor_text, editor_cursor_offset));
        let matches = crate::suggestion_matches_frecency(
            &state.tabs[active].history,
            &state.tabs[active].history_entries,
            prefix,
            &state.tabs[active].cwd,
        );
        matches
            .get(idx)
            .map(|full| truncate_display(&full[prefix.len()..], 80))
            .unwrap_or_default()
    } else if cursor_at_line_end(&editor_text, editor_cursor_offset) {
        let prefix = current_line_prefix(&editor_text, editor_cursor_offset);
        if prefix.is_empty() {
            String::new()
        } else {
            crate::suggestion_matches_frecency(
                &state.tabs[active].history,
                &state.tabs[active].history_entries,
                prefix,
                &state.tabs[active].cwd,
            )
            .into_iter()
            .next()
            .map(|full| truncate_display(&full[prefix.len()..], 80))
            .unwrap_or_default()
        }
    } else {
        String::new()
    };

    let scrollback_lines = state.tabs[active].app.scrollback_len();
    let editor_scroll_offset = state.tabs[active].editor_scroll_offset;
    let editor_selection = state.tabs[active].app.editor_selection();
    let split_ratio = state.tabs[active].split_ratio;
    let selection_anchor = state.tabs[active].selection_anchor;
    let selection_anchor_scroll = state.tabs[active].selection_anchor_scroll;
    let selection_end = state.tabs[active].selection_end;
    let selection_end_scroll = state.tabs[active].selection_end_scroll;
    let current_scroll = state.tabs[active].scroll_offset;

    let resize_overlay = if let Some(ref banner) = state.overlays.pending_update {
        Some(match banner {
            crate::UpdateBanner::Available(v) => {
                format!("Updated to v{v} \u{2014} restart to apply")
            }
            crate::UpdateBanner::Failed(err) => format!("Update failed: {err}"),
        })
    } else if let Some((ref t, ref message)) = state.overlays.pty_status {
        if t.elapsed().as_secs_f32() < 2.5 {
            Some(message.clone())
        } else {
            state.overlays.pty_status = None;
            None
        }
    } else if let Some((ref t, cols, rows)) = state.overlays.last_resize {
        if t.elapsed().as_secs_f32() < 1.0 {
            Some(format!("{cols}\u{d7}{rows}"))
        } else {
            state.overlays.last_resize = None;
            None
        }
    } else {
        None
    };

    let selection = match (selection_anchor, selection_end) {
        (Some(a), Some(e)) => {
            // Adjust stored rows to the current scroll offset.  When
            // scroll_offset increases (user scrolled back further), visible
            // content moves down, so the row number increases by the delta.
            let delta_a = current_scroll as i64 - selection_anchor_scroll as i64;
            let delta_e = current_scroll as i64 - selection_end_scroll as i64;
            let ar = (a.0 as i64 + delta_a).max(0) as usize;
            let er = (e.0 as i64 + delta_e).max(0) as usize;
            let (sr, sc, er_final, ec) = if (ar, a.1) <= (er, e.1) {
                (ar, a.1, er, e.1)
            } else {
                (er, e.1, ar, a.1)
            };
            Some((sr, sc, er_final, ec))
        }
        _ => None,
    };

    // Refresh cwd labels from child process (best-effort; silent on failure).
    let n_tabs = state.tabs.len();
    for i in 0..n_tabs {
        if let Some(pid) = state.tabs[i].pty.as_ref().and_then(|p| p.child_pid())
            && let Some(new_cwd) = read_child_cwd(pid)
        {
            state.tabs[i].cwd = new_cwd;
        }
    }
    let tab_labels: Vec<String> = if state.tabs.len() > 1 {
        let n = state.tabs.len();
        let add_btn_w = state.layout.cell_w * 2.0;
        let tab_area_w = state.layout.window_width as f32 - add_btn_w;
        let tab_w_px = tab_area_w / n as f32;
        let max_chars = tab_button_max_chars(tab_w_px, state.layout.cell_w);
        state
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let label = tab_button_label(index, tab.app.window_title(), &tab.cwd, max_chars);
                if index != state.active_tab && tab.unread_output {
                    format!("• {label}")
                } else {
                    label
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let active_tab = state.active_tab;

    let tab_drag_insert_before = state.drag.tab_drag.and_then(|_| {
        if state.tabs.len() <= 1 {
            return None;
        }
        if (state.cursor.cursor_x - state.drag.tab_drag_start_x).abs() > 5.0 {
            let n = state.tabs.len();
            let add_btn_w = state.layout.cell_w as f64 * 2.0;
            let tab_area_w = (state.layout.window_width as f64 - add_btn_w).max(1.0);
            let frac = (state.cursor.cursor_x / tab_area_w).clamp(0.0, 1.0);
            Some((frac * n as f64).round() as usize)
        } else {
            None
        }
    });

    let tab_context_menu = if state.tabs.len() > 1 {
        state
            .overlays
            .tab_context_menu
            .map(|(tab_idx, x_px, y_px)| TabContextMenu {
                tab_idx,
                x_px: x_px as f32,
                y_px: y_px as f32,
                hovered_item: state.overlays.tab_context_hover,
            })
    } else {
        None
    };

    // GC expired toasts.
    let now = std::time::Instant::now();
    state.overlays.toasts.retain(|t| t.expires_at > now);
    let toast_stack: Vec<Toast> = state
        .overlays
        .toasts
        .iter()
        .map(|t| Toast {
            text: t.text.clone(),
            kind: match t.kind {
                crate::state::ToastKind::Info => ToastKind::Info,
                crate::state::ToastKind::Success => ToastKind::Success,
                crate::state::ToastKind::Warn => ToastKind::Warn,
                crate::state::ToastKind::Error => ToastKind::Error,
            },
        })
        .collect();

    RenderSnapshot {
        terminal_rows,
        terminal_damage,
        terminal_text,
        terminal_fg_colors,
        terminal_bg_colors,
        terminal_styles,
        editor_text: editor_text.clone(),
        editor_cursor_offset,
        scroll_offset,
        scrollback_lines,
        editor_focused: true,
        split_ratio,
        resize_overlay,
        editor_line_count,
        editor_scroll_offset,
        editor_selection,
        selection,
        search_highlights,
        search_current_highlight,
        tab_labels,
        active_tab,
        tab_context_menu,
        tab_drag_from: state.drag.tab_drag,
        tab_drag_insert_before,
        theme: {
            let tf = state
                .themes_fonts
                .active_theme_idx
                .map(|i| &state.themes_fonts.available_themes[i]);
            theme_from_config(tf)
        },
        padding_h: state.user_config.padding.horizontal,
        padding_v: state.user_config.padding.vertical,
        settings_overlay: build_settings_overlay(state),
        title_cwd: {
            // OSC 0/2 window title takes priority; fall back to CWD path.
            if let Some(title) = state.tabs[state.active_tab].app.window_title() {
                title.to_owned()
            } else {
                let home = std::env::var("HOME").unwrap_or_default();
                let cwd = &state.tabs[state.active_tab].cwd;
                if !home.is_empty() && cwd.starts_with(&home) {
                    format!("~{}", &cwd[home.len()..])
                } else {
                    cwd.clone()
                }
            }
        },
        editor_suggestion,
        search_panel,
        terminal_links,
        request_exit: state.should_exit,
        cursor_shape: state.tabs[active].app.cursor_shape(),
        bell_active: state
            .overlays
            .bell_flash_until
            .is_some_and(|t| t > std::time::Instant::now()),
        cursor_blink_on: state.overlays.cursor_blink_phase,
        terminal_cursor_row: state.tabs[active].app.terminal_cursor_pos().0,
        terminal_cursor_col: state.tabs[active].app.terminal_cursor_pos().1,
        terminal_fullscreen: state.tabs[active].was_terminal_fullscreen,
        terminal_screen_version: state.tabs[active].app.terminal_screen_version(),
        toast_stack,
        suggestion_dropdown: {
            if let Some(idx) = state.tabs[active].suggestion_index {
                let prefix = state.tabs[active]
                    .suggestion_prefix
                    .as_deref()
                    .unwrap_or_else(|| current_line_prefix(&editor_text, editor_cursor_offset));
                let items = crate::suggestion_matches_frecency(
                    &state.tabs[active].history,
                    &state.tabs[active].history_entries,
                    prefix,
                    &state.tabs[active].cwd,
                );
                if items.len() >= 2 {
                    let display: Vec<String> = items
                        .into_iter()
                        .map(|s| truncate_display(&s, 50))
                        .collect();
                    let scroll_offset = idx
                        .saturating_sub(crate::consts::SUGGESTION_DROPDOWN_MAX_VISIBLE - 1)
                        .min(
                            display
                                .len()
                                .saturating_sub(crate::consts::SUGGESTION_DROPDOWN_MAX_VISIBLE),
                        );
                    Some(SuggestionDropdown {
                        items: display,
                        selected: idx,
                        scroll_offset,
                    })
                } else {
                    None
                }
            } else {
                None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::tab_button_label;

    #[test]
    fn tab_button_label_uses_title_when_present() {
        assert_eq!(
            tab_button_label(0, Some("My Shell"), "/tmp", 16),
            "Cmd+1  My Shell"
        );
    }

    #[test]
    fn tab_button_label_falls_back_to_cwd() {
        assert_eq!(
            tab_button_label(1, None, "/tmp/project", 16),
            "Cmd+2  /tmp/project"
        );
    }

    #[test]
    fn tab_button_label_truncates_to_fit() {
        assert_eq!(
            tab_button_label(2, Some("very long terminal title"), "/tmp", 8),
            "Cmd+3  very lon…"
        );
    }
}
