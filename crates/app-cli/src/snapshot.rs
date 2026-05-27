use crate::coords::{read_child_cwd, shorten_cwd_label};
use crate::settings::build_settings_overlay;
use crate::theme;
use crate::GpuRuntimeState;
use render_wgpu::{ColorTheme, RenderSnapshot, SuggestionDropdown, TabContextMenu};

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
    let editor_text = state.tabs[active].app.editor_snapshot();
    let editor_line_count = editor_text.lines().count().max(1);
    let editor_cursor_offset = state.tabs[active].app.editor_cursor_offset();

    // Ghost-text suggestion: the suffix of the most-recently-used history
    // entry (case-insensitive prefix match) that extends the current editor
    // text.  Not shown while Tab-cycling is active — the editor content
    // already shows the selected match in that case.
    let editor_suggestion = if state.tabs[active].suggestion_index.is_some() {
        // Cycling in progress: the editor holds the full selected match, so
        // there is nothing to add as a ghost-text suffix.
        String::new()
    } else if !editor_text.is_empty() && editor_cursor_offset == editor_text.len() {
        crate::suggestion_matches_frecency(
            &state.tabs[active].history,
            &state.tabs[active].history_entries,
            &editor_text,
            &state.tabs[active].cwd,
        )
        .into_iter()
        .next()
        .map(|full| truncate_display(&full[editor_text.len()..], 80))
        .unwrap_or_default()
    } else {
        String::new()
    };

    let scrollback_lines = state.tabs[active].app.scrollback_len();
    let editor_scroll_offset = state.tabs[active].editor_scroll_offset;
    let editor_selection = state.tabs[active].app.editor_selection();
    let split_ratio = state.tabs[active].split_ratio;
    let selection_anchor = state.tabs[active].selection_anchor;
    let selection_end = state.tabs[active].selection_end;

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
            let (sr, sc, er, ec) = if a <= e {
                (a.0, a.1, e.0, e.1)
            } else {
                (e.0, e.1, a.0, a.1)
            };
            Some((sr, sc, er, ec))
        }
        _ => None,
    };

    // Refresh cwd labels from child process (best-effort; silent on failure).
    let n_tabs = state.tabs.len();
    for i in 0..n_tabs {
        if let Some(pid) = state.tabs[i].pty.as_ref().and_then(|p| p.child_pid()) {
            if let Some(new_cwd) = read_child_cwd(pid) {
                state.tabs[i].cwd = new_cwd;
            }
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
            let home = std::env::var("HOME").unwrap_or_default();
            let cwd = &state.tabs[state.active_tab].cwd;
            if !home.is_empty() && cwd.starts_with(&home) {
                format!("~{}", &cwd[home.len()..])
            } else {
                cwd.clone()
            }
        },
        editor_suggestion,
        suggestion_dropdown: {
            if let Some(idx) = state.tabs[active].suggestion_index {
                let prefix = state.tabs[active]
                    .suggestion_prefix
                    .as_deref()
                    .unwrap_or(editor_text.as_str());
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
