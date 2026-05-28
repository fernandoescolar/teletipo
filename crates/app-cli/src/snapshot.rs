use crate::coords::{current_line_prefix, cursor_at_line_end, cursor_to_terminal_cell, detect_terminal_links, read_child_cwd, shorten_cwd_label};
use crate::settings::build_settings_overlay;
use crate::theme;
use crate::GpuRuntimeState;
use render_wgpu::{ColorTheme, RenderSnapshot, SuggestionDropdown, TabContextMenu, TerminalLink};

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
        terminal_bg:       c(&tf.background),
        editor_bg:         c(&tf.background),
        separator:         c(&tf.terminal_colors.normal.black),
        separator_focused: c(&tf.accent),
        cursor:            c(&tf.cursor),
        text:              c(&tf.foreground),
        ansi_palette:      theme::build_ansi_palette(tf),
    }
}

/// Build a complete `RenderSnapshot` from the current state for the frame closure.
pub(crate) fn build_snapshot(state: &mut GpuRuntimeState) -> RenderSnapshot {
    // Clear one-shot just_saved flag after it has been shown for a frame.
    if state.settings.just_saved {
        state.settings.just_saved = false;
    }

    // Poll the background update-check thread (once; then drop the receiver).
    if let Some(ref rx) = state.update_rx {
        match rx.try_recv() {
            Ok(result) => {
                state.pending_update = result;
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
    }

    let active = state.active_tab;
    let scroll_offset = state.tabs[active].scroll_offset;
    let active_palette: Option<[[f32; 3]; 16]> = state.active_theme_idx
        .map(|i| theme::build_ansi_palette(&state.available_themes[i]));
    let styled = state.tabs[active].app.terminal_styled_snapshot_at_offset_with_palette(
        scroll_offset,
        active_palette.as_ref(),
    );
    let terminal_text: String = styled.iter().map(|(ch, _, _)| *ch).collect();
    let terminal_fg_colors: Vec<Option<[f32; 3]>> =
        styled.iter().map(|(_, fg, _)| *fg).collect();
    let terminal_bg_colors: Vec<Option<[f32; 3]>> =
        styled.iter().map(|(_, _, bg)| *bg).collect();
    state.tabs[active].last_terminal_text = terminal_text.clone();
    state.tabs[active].term_row_count = terminal_text.lines().count().max(1);
    // Underline only the link the cursor is currently hovering over.
    let terminal_links: Vec<TerminalLink> = {
        let all_links = detect_terminal_links(&terminal_text);
        if all_links.is_empty() {
            Vec::new()
        } else {
            let split_ratio = state.tabs[active].split_ratio;
            let tab_bar_h   = state.tab_bar_h();
            let pad_h = state.user_config.padding.horizontal as f32;
            let pad_v = state.user_config.padding.vertical   as f32;
            let term_row_count = state.tabs[active].term_row_count;
            if let Some((hover_row, hover_col)) = cursor_to_terminal_cell(
                state.cursor_x, state.cursor_y,
                state.window_width, state.window_height,
                split_ratio, state.cell_w, state.cell_h,
                term_row_count, tab_bar_h,
                pad_h, pad_v,
            ) {
                all_links
                    .into_iter()
                    .filter(|(r, cs, ce, _)| *r == hover_row && hover_col >= *cs && hover_col < *ce)
                    .map(|(row, col_start, col_end, target)| TerminalLink { row, col_start, col_end, target })
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

    let resize_overlay = if let Some(ref v) = state.pending_update {
        Some(format!("Updated to v{v} \u{2014} restart to apply"))
    } else if let Some((ref t, cols, rows)) = state.last_resize {
        if t.elapsed().as_secs_f32() < 1.0 {
            Some(format!("{cols}\u{d7}{rows}"))
        } else {
            state.last_resize = None;
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
            && let Some(new_cwd) = read_child_cwd(pid) {
                state.tabs[i].cwd = new_cwd;
            }
    }
    let tab_labels: Vec<String> = state.tabs.iter()
        .map(|t| shorten_cwd_label(&t.cwd, 16))
        .collect();
    let active_tab = state.active_tab;

    let tab_drag_insert_before = state.tab_drag.and_then(|_| {
        if (state.cursor_x - state.tab_drag_start_x).abs() > 5.0 {
            let n = state.tabs.len();
            let add_btn_w = state.cell_w as f64 * 2.0;
            let tab_area_w = (state.window_width as f64 - add_btn_w).max(1.0);
            let frac = (state.cursor_x / tab_area_w).clamp(0.0, 1.0);
            Some((frac * n as f64).round() as usize)
        } else {
            None
        }
    });

    let tab_context_menu = state.tab_context_menu.map(|(tab_idx, x_px, y_px)| TabContextMenu {
        tab_idx,
        x_px: x_px as f32,
        y_px: y_px as f32,
        hovered_item: state.tab_context_hover,
    });

    RenderSnapshot {
        terminal_text,
        terminal_fg_colors,
        terminal_bg_colors,
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
        tab_labels,
        active_tab,
        tab_context_menu,
        tab_drag_from: state.tab_drag,
        tab_drag_insert_before,
        theme: {
            let tf = state.active_theme_idx
                .map(|i| &state.available_themes[i]);
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
        terminal_links,
        request_exit: state.should_exit,
        cursor_shape: state.tabs[active].app.cursor_shape(),
        bell_active: state.bell_flash_until.map_or(false, |t| t > std::time::Instant::now()),
        terminal_cursor_row: state.tabs[active].app.terminal_cursor_pos().0,
        terminal_cursor_col: state.tabs[active].app.terminal_cursor_pos().1,
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
                    let display: Vec<String> =
                        items.into_iter().map(|s| truncate_display(&s, 50)).collect();
                    // Keep the selected item inside the visible window.
                    const MAX_VISIBLE: usize = 8;
                    let scroll_offset = idx
                        .saturating_sub(MAX_VISIBLE - 1)
                        .min(display.len().saturating_sub(MAX_VISIBLE));
                    Some(SuggestionDropdown { items: display, selected: idx, scroll_offset })
                } else {
                    None
                }
            } else {
                None
            }
        },
    }
}
