use std::sync::Arc;

use crate::GpuRuntimeState;
use crate::coords::{
    TerminalLayout, current_line_prefix, cursor_at_line_end, cursor_to_terminal_cell,
    detect_terminal_links, read_child_cwd, shorten_cwd_label,
};
use crate::settings::build_settings_overlay;
use crate::theme;
use editor_lang::{LanguageHighlighter, ShellLikeHighlighter};
use render_glow::{
    ColorTheme, CommandPalette, ContextMenu, DamageRegion, RenderCell, RenderRow, RenderSnapshot,
    SearchPanel, SuggestionDropdown, TerminalLink, Toast, ToastKind,
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

fn tab_button_label_for_tab(
    index: usize,
    _title: Option<&str>,
    cwd: &str,
    command_running: bool,
    pending_cmd: Option<&str>,
    max_chars: usize,
) -> String {
    // Shells commonly publish the same generic OSC title for every tab. Use
    // live per-tab state for labels so tabs remain distinguishable; OSC titles
    // still drive the native window title.
    let fallback = if command_running {
        if let Some(cmd) = pending_cmd.map(str::trim).filter(|cmd| !cmd.is_empty()) {
            truncate_display(&format!("[run] {cmd}"), max_chars)
        } else {
            truncate_display(
                &format!("[run] {}", shorten_cwd_label(cwd, max_chars)),
                max_chars,
            )
        }
    } else {
        shorten_cwd_label(cwd, max_chars)
    };
    tab_button_label(index, Some(&fallback), cwd, max_chars)
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
pub(crate) fn build_snapshot(state: &mut GpuRuntimeState) -> RenderSnapshot {
    tick_frame_housekeeping(state);

    let active = state.active_tab;
    let scroll_offset = state.tabs[active].scroll_offset;

    let (
        terminal_rows,
        terminal_damage,
        terminal_text,
        terminal_fg_colors,
        terminal_bg_colors,
        terminal_styles,
        term_cols,
        editor_disabled,
    ) = build_terminal_content(state, active, scroll_offset);

    let (search_panel, search_highlights, search_current_highlight) =
        build_search_section(state, active);

    let terminal_links =
        build_terminal_links(state, active, &terminal_text, term_cols, scroll_offset);

    let editor_text = state.tabs[active].app.editor_snapshot();
    let editor_fg_colors = build_editor_syntax_colors(state, &editor_text);
    let editor_line_count = editor_text.lines().count().max(1);
    let editor_cursor_offset = state.tabs[active].app.editor_cursor_offset();
    let editor_suggestion =
        build_editor_suggestion(state, active, &editor_text, editor_cursor_offset);
    let suggestion_dropdown =
        build_suggestion_dropdown(state, active, &editor_text, editor_cursor_offset);

    let resize_overlay = build_resize_overlay(state);
    let selection = adjust_selection_for_scroll(
        state.tabs[active].selection_anchor,
        state.tabs[active].selection_end,
        state.tabs[active].selection_anchor_scroll,
        state.tabs[active].selection_end_scroll,
        state.tabs[active].scroll_offset,
    );
    let (tab_labels, tab_drag_insert_before) = build_tab_bar(state);
    let context_menu = build_context_menu(state);
    let toast_stack = collect_toasts(state);
    let command_palette = build_command_palette_snapshot(state);

    assemble_snapshot(
        state,
        active,
        ComputedFrame {
            scroll_offset,
            editor_text,
            editor_fg_colors,
            editor_line_count,
            editor_cursor_offset,
            editor_suggestion,
            editor_disabled,
            terminal_rows,
            terminal_damage,
            terminal_text,
            terminal_fg_colors,
            terminal_bg_colors,
            terminal_styles,
            search_panel,
            search_highlights,
            search_current_highlight,
            terminal_links,
            resize_overlay,
            selection,
            tab_labels,
            tab_drag_insert_before,
            context_menu,
            toast_stack,
            suggestion_dropdown,
            command_palette,
        },
    )
}

/// All per-frame computed data passed to `assemble_snapshot`.
struct ComputedFrame {
    scroll_offset: usize,
    editor_text: String,
    editor_fg_colors: Vec<Option<[f32; 3]>>,
    editor_line_count: usize,
    editor_cursor_offset: usize,
    editor_suggestion: String,
    editor_disabled: bool,
    terminal_rows: Vec<RenderRow>,
    terminal_damage: Arc<DamageRegion>,
    terminal_text: String,
    terminal_fg_colors: Vec<Option<[f32; 3]>>,
    terminal_bg_colors: Vec<Option<[f32; 3]>>,
    terminal_styles: Vec<u8>,
    search_panel: Option<SearchPanel>,
    search_highlights: Vec<(usize, usize, usize)>,
    search_current_highlight: Option<(usize, usize, usize)>,
    terminal_links: Vec<TerminalLink>,
    resize_overlay: Option<String>,
    selection: Option<(usize, usize, usize, usize)>,
    tab_labels: Vec<String>,
    tab_drag_insert_before: Option<usize>,
    context_menu: Option<ContextMenu>,
    toast_stack: Vec<Toast>,
    suggestion_dropdown: Option<SuggestionDropdown>,
    command_palette: Option<CommandPalette>,
}

/// Assemble the final `RenderSnapshot` from all pre-computed parts.
fn assemble_snapshot(state: &GpuRuntimeState, active: usize, f: ComputedFrame) -> RenderSnapshot {
    let theme = {
        let tf = state
            .themes_fonts
            .active_theme_idx
            .map(|i| &state.themes_fonts.available_themes[i]);
        theme_from_config(tf)
    };
    RenderSnapshot {
        terminal_rows: f.terminal_rows,
        terminal_damage: f.terminal_damage,
        terminal_text: f.terminal_text,
        terminal_fg_colors: f.terminal_fg_colors,
        terminal_bg_colors: f.terminal_bg_colors,
        terminal_styles: f.terminal_styles,
        editor_text: f.editor_text.clone(),
        editor_fg_colors: f.editor_fg_colors,
        editor_cursor_offset: f.editor_cursor_offset,
        scroll_offset: f.scroll_offset,
        scrollback_lines: state.tabs[active].app.scrollback_len(),
        editor_focused: true,
        editor_disabled: f.editor_disabled,
        split_ratio: state.tabs[active].split_ratio,
        resize_overlay: f.resize_overlay,
        editor_line_count: f.editor_line_count,
        editor_scroll_offset: state.tabs[active].editor_scroll_offset,
        editor_horizontal_scroll_offset: state.tabs[active].editor_horizontal_scroll_offset,
        editor_selection: state.tabs[active].app.editor_selection(),
        selection: f.selection,
        search_highlights: f.search_highlights,
        search_current_highlight: f.search_current_highlight,
        tab_labels: f.tab_labels,
        active_tab: active,
        context_menu: f.context_menu,
        tab_drag_from: state.drag.tab_drag,
        tab_drag_insert_before: f.tab_drag_insert_before,
        theme,
        padding_h: state.user_config.padding.horizontal,
        padding_v: state.user_config.padding.vertical,
        settings_overlay: build_settings_overlay(state),
        keybindings_overlay: crate::keybindings_ui::build_keybindings_overlay(state),
        title_cwd: build_title_cwd(state),
        editor_suggestion: f.editor_suggestion,
        search_panel: f.search_panel,
        terminal_links: f.terminal_links,
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
        toast_stack: f.toast_stack,
        suggestion_dropdown: f.suggestion_dropdown,
        font_size: state.user_config.font.size,
        command_palette: f.command_palette,
    }
}

/// Build per-character syntax colors for the command editor.
///
/// The output vector is parallel to `text.chars()` so renderer indexing matches
/// even when skipping lines/columns in the viewport.
fn build_editor_syntax_colors(state: &GpuRuntimeState, text: &str) -> Vec<Option<[f32; 3]>> {
    let mut colors: Vec<Option<[f32; 3]>> = text.chars().map(|_| None).collect();
    if text.is_empty() {
        return colors;
    }

    let theme = {
        let tf = state
            .themes_fonts
            .active_theme_idx
            .map(|i| &state.themes_fonts.available_themes[i]);
        theme_from_config(tf)
    };
    let bg = [theme.editor_bg[0], theme.editor_bg[1], theme.editor_bg[2]];
    let default_text = [theme.text[0], theme.text[1], theme.text[2]];
    let command_color = pick_high_contrast_color(
        bg,
        &[
            theme.ansi_palette[10],
            theme.ansi_palette[2],
            theme.ansi_palette[12],
            default_text,
        ],
        default_text,
    );
    let flag_color = pick_high_contrast_color(
        bg,
        &[
            theme.ansi_palette[11],
            theme.ansi_palette[3],
            theme.ansi_palette[9],
            default_text,
        ],
        default_text,
    );
    let arg_color = pick_high_contrast_color(
        bg,
        &[
            theme.ansi_palette[14],
            theme.ansi_palette[6],
            theme.ansi_palette[15],
            default_text,
        ],
        default_text,
    );

    let highlighter = ShellLikeHighlighter;
    let ranges = highlighter.highlight(text);

    let char_byte_starts: Vec<usize> = text.char_indices().map(|(idx, _)| idx).collect();
    let to_char_idx = |byte_idx: usize| -> usize {
        match char_byte_starts.binary_search(&byte_idx) {
            Ok(i) => i,
            Err(i) => i,
        }
    };

    for h in ranges {
        let start = to_char_idx(h.range.start);
        let end = to_char_idx(h.range.end);
        let token_color = match h.token {
            "command" => Some(command_color),
            "flag" => Some(flag_color),
            "arg" => Some(arg_color),
            _ => None,
        };
        if let Some(c) = token_color {
            for slot in &mut colors[start..end] {
                *slot = Some(c);
            }
        }
    }

    colors
}

fn pick_high_contrast_color(
    bg: [f32; 3],
    candidates: &[[f32; 3]],
    fallback: [f32; 3],
) -> [f32; 3] {
    const MIN_RATIO: f32 = 3.0;
    let mut best = fallback;
    let mut best_ratio = contrast_ratio(bg, fallback);
    for &candidate in candidates {
        let ratio = contrast_ratio(bg, candidate);
        if ratio > best_ratio {
            best_ratio = ratio;
            best = candidate;
        }
        if ratio >= MIN_RATIO {
            return candidate;
        }
    }
    best
}

fn contrast_ratio(a: [f32; 3], b: [f32; 3]) -> f32 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (l1, l2) = if la >= lb { (la, lb) } else { (lb, la) };
    (l1 + 0.05) / (l2 + 0.05)
}

fn relative_luminance(rgb: [f32; 3]) -> f32 {
    let f = |c: f32| {
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(rgb[0].clamp(0.0, 1.0))
        + 0.7152 * f(rgb[1].clamp(0.0, 1.0))
        + 0.0722 * f(rgb[2].clamp(0.0, 1.0))
}

/// Per-frame housekeeping: polls update channel, autosaves session, handles deferred
/// resize, pumps PTYs, advances cursor blink, and resets per-tab read indicators.
fn tick_frame_housekeeping(state: &mut GpuRuntimeState) {
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
                state.update_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    // Autosave session every 5 minutes when session restore is enabled.
    const AUTOSAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
    if state.user_config.terminal.restore_session
        && state.last_session_save.elapsed() >= AUTOSAVE_INTERVAL
    {
        crate::launch::save_session(state);
        state.last_session_save = std::time::Instant::now();
    }

    // Re-arm the update check once per day while the app stays open.
    const UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
    if state.update_rx.is_none()
        && state.overlays.pending_update.is_none()
        && state.update_last_checked.elapsed() >= UPDATE_INTERVAL
    {
        state.update_rx = Some(crate::updater::spawn_update());
        state.update_last_checked = std::time::Instant::now();
    }

    // Apply deferred resize once window resizing has been idle for ≥ 150 ms.
    if state
        .overlays
        .pending_pty_resize
        .is_some_and(|t| t.elapsed().as_millis() >= 150)
        && !state.drag.dragging_separator
    {
        state.apply_deferred_resize();
    }

    let had_data = state.pump_all_ptys();
    if had_data {
        let active = state.active_tab;
        state.tabs[active].scroll_offset = 0;
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
    state.tabs[state.active_tab].bell_pending = false;

    // Show a one-shot toast for config parse errors on startup.
    if let Some(err) = state.config_error.take() {
        state.push_toast(
            format!("Config error: {err}"),
            crate::state::ToastKind::Error,
        );
    }
}

/// Render the active tab's terminal content into rows and a damage region.
///
/// Returns `(rows, damage, text, fg_colors, bg_colors, styles, term_cols, editor_disabled)`.
#[allow(clippy::type_complexity)]
fn build_terminal_content(
    state: &mut GpuRuntimeState,
    active: usize,
    scroll_offset: usize,
) -> (
    Vec<RenderRow>,
    Arc<DamageRegion>,
    String,
    Vec<Option<[f32; 3]>>,
    Vec<Option<[f32; 3]>>,
    Vec<u8>,
    usize,
    bool,
) {
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
    let editor_disabled = state.tabs[active].command_running && !state.tabs[active].editor_unlocked;
    damage.full_redraw =
        screen_damage.full_redraw || (editor_disabled != state.last_editor_disabled);
    state.last_editor_disabled = editor_disabled;
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
    state.tabs[active].last_terminal_text = terminal_text.clone();
    state.tabs[active].term_row_count = terminal_text.lines().count().max(1);
    (
        terminal_rows,
        Arc::new(damage),
        terminal_text,
        terminal_fg_colors,
        terminal_bg_colors,
        terminal_styles,
        term_cols,
        editor_disabled,
    )
}

/// `(panel, all_highlights, current_highlight)` for [`build_search_section`].
type SearchSection = (
    Option<SearchPanel>,
    Vec<(usize, usize, usize)>,
    Option<(usize, usize, usize)>,
);

/// Build the search panel and highlight lists for the active tab.
fn build_search_section(state: &mut GpuRuntimeState, active: usize) -> SearchSection {
    if state.tabs[active].search.active {
        crate::search::refresh_search(&mut state.tabs[active]);
    }
    if !state.tabs[active].search.active {
        return (None, Vec::new(), None);
    }
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
}

/// Detect terminal links and return only the hovered URL's segments (if any).
fn build_terminal_links(
    state: &GpuRuntimeState,
    active: usize,
    terminal_text: &str,
    term_cols: usize,
    scroll_offset: usize,
) -> Vec<TerminalLink> {
    // Pattern-detected links (URLs, file paths) from the rendered text.
    let mut all_links = detect_terminal_links(terminal_text, term_cols);

    // OSC 8 explicit hyperlinks — authoritative over pattern-detected links.
    let osc8 = state.tabs[active]
        .app
        .terminal
        .hyperlink_spans(scroll_offset);
    for (row, cs, ce, id) in &osc8 {
        if let Some(uri) = state.tabs[active].app.terminal.hyperlink_uri(*id) {
            all_links.retain(|(r, lcs, lce, _)| *r != *row || *lce <= *cs || *lcs >= *ce);
            all_links.push((*row, *cs, *ce, uri.to_owned()));
        }
    }

    if all_links.is_empty() {
        return Vec::new();
    }
    let split_ratio = state.tabs[active].split_ratio;
    let tab_bar_h = state.tab_bar_h();
    let pad_h = state.user_config.padding.horizontal as f32;
    let pad_v = state.user_config.padding.vertical as f32;
    let term_row_count = state.tabs[active].term_row_count;
    let Some((hover_row, hover_col)) = cursor_to_terminal_cell(
        state.cursor.cursor_x,
        state.cursor.cursor_y,
        state.layout.window_width,
        state.layout.window_height,
        &TerminalLayout {
            split_ratio,
            cell_w_px: state.layout.cell_w,
            cell_h_px: state.layout.cell_h,
            term_row_count,
            tab_bar_h,
            pad_h,
            pad_v,
        },
    ) else {
        return Vec::new();
    };
    let hovered_url = all_links
        .iter()
        .find(|(r, cs, ce, _)| *r == hover_row && hover_col >= *cs && hover_col < *ce)
        .map(|(_, _, _, url)| url.clone());
    let Some(url) = hovered_url else {
        return Vec::new();
    };
    // Return ALL segments that belong to the same URL (multi-line).
    all_links
        .into_iter()
        .filter(|(_, _, _, u)| *u == url)
        .map(|(row, col_start, col_end, target)| TerminalLink {
            row,
            col_start,
            col_end,
            target,
        })
        .collect()
}

/// Compute the ghost-text editor suggestion (history prefix match or cycling preview).
fn build_editor_suggestion(
    state: &GpuRuntimeState,
    active: usize,
    editor_text: &str,
    editor_cursor_offset: usize,
) -> String {
    if let Some(idx) = state.tabs[active].suggestion_index {
        // Cycling in progress: display the selected match's completion as ghost text.
        let prefix = state.tabs[active]
            .suggestion_prefix
            .as_deref()
            .unwrap_or_else(|| current_line_prefix(editor_text, editor_cursor_offset));
        let matches = crate::suggestion_matches_frecency(
            &state.tabs[active].history,
            &state.tabs[active].history_entries,
            prefix,
            &state.tabs[active].cwd,
            &state.shell,
        );
        matches
            .get(idx)
            .map(|full| truncate_display(&full[prefix.len()..], 80))
            .unwrap_or_default()
    } else if cursor_at_line_end(editor_text, editor_cursor_offset) {
        let prefix = current_line_prefix(editor_text, editor_cursor_offset);
        if prefix.is_empty() {
            String::new()
        } else {
            crate::suggestion_matches_frecency(
                &state.tabs[active].history,
                &state.tabs[active].history_entries,
                prefix,
                &state.tabs[active].cwd,
                &state.shell,
            )
            .into_iter()
            .next()
            .map(|full| truncate_display(&full[prefix.len()..], 80))
            .unwrap_or_default()
        }
    } else {
        String::new()
    }
}

/// Build the transient overlay label shown in the top-right corner (resize, PTY status, etc.).
fn build_resize_overlay(state: &mut GpuRuntimeState) -> Option<String> {
    if let Some(ref banner) = state.overlays.pending_update {
        return Some(match banner {
            crate::UpdateBanner::Available(v) => {
                format!("Update ready v{v} \u{2014} click to restart")
            }
            crate::UpdateBanner::Failed(err) => format!("Update failed: {err}"),
        });
    }
    if let Some((ref t, ref message)) = state.overlays.pty_status {
        if t.elapsed().as_secs_f32() < 2.5 {
            return Some(message.clone());
        }
        state.overlays.pty_status = None;
    }
    if let Some((ref t, cols, rows)) = state.overlays.last_resize {
        if t.elapsed().as_secs_f32() < 1.0 {
            return Some(format!("{cols}\u{d7}{rows}"));
        }
        state.overlays.last_resize = None;
    }
    if let Some((ref t, ref label)) = state.overlays.last_cmd_duration {
        if t.elapsed().as_secs_f32() < 4.0 {
            return Some(label.clone());
        }
        state.overlays.last_cmd_duration = None;
    }
    None
}

/// Translate stored selection anchor/end to current-scroll-relative coordinates.
fn adjust_selection_for_scroll(
    selection_anchor: Option<(usize, usize)>,
    selection_end: Option<(usize, usize)>,
    selection_anchor_scroll: usize,
    selection_end_scroll: usize,
    current_scroll: usize,
) -> Option<(usize, usize, usize, usize)> {
    let (a, e) = (selection_anchor?, selection_end?);
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

/// Refresh CWD labels, compute tab button labels, and compute drag insert position.
fn build_tab_bar(state: &mut GpuRuntimeState) -> (Vec<String>, Option<usize>) {
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
                let label = tab_button_label_for_tab(
                    index,
                    tab.app.window_title(),
                    &tab.cwd,
                    tab.command_running,
                    tab.pending_cmd.as_deref(),
                    max_chars,
                );
                if index != state.active_tab {
                    let mut marker = String::new();
                    if tab.bell_pending {
                        marker.push('!');
                    }
                    if tab.unread_output {
                        marker.push('•');
                    }
                    if marker.is_empty() {
                        label
                    } else {
                        format!("{marker} {label}")
                    }
                } else {
                    label
                }
            })
            .collect()
    } else {
        Vec::new()
    };
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
    (tab_labels, tab_drag_insert_before)
}

/// GC expired toasts and convert them to the renderer's `Toast` type.
fn collect_toasts(state: &mut GpuRuntimeState) -> Vec<Toast> {
    let now = std::time::Instant::now();
    state.overlays.toasts.retain(|t| t.expires_at > now);
    state
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
        .collect()
}

/// Build the suggestion dropdown if a Tab-cycle is in progress.
fn build_suggestion_dropdown(
    state: &GpuRuntimeState,
    active: usize,
    editor_text: &str,
    editor_cursor_offset: usize,
) -> Option<SuggestionDropdown> {
    let idx = state.tabs[active].suggestion_index?;
    let prefix = state.tabs[active]
        .suggestion_prefix
        .as_deref()
        .unwrap_or_else(|| current_line_prefix(editor_text, editor_cursor_offset));
    let items = crate::suggestion_matches_frecency(
        &state.tabs[active].history,
        &state.tabs[active].history_entries,
        prefix,
        &state.tabs[active].cwd,
        &state.shell,
    );
    if items.len() < 2 {
        return None;
    }
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
}

/// Build the window title string (OSC 0/2 if set; otherwise CWD with `~` abbreviation).
fn build_title_cwd(state: &GpuRuntimeState) -> String {
    if let Some(title) = state.tabs[state.active_tab].app.window_title() {
        return title.to_owned();
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let cwd = &state.tabs[state.active_tab].cwd;
    if !home.is_empty() && cwd.starts_with(&home) {
        format!("~{}", &cwd[home.len()..])
    } else {
        cwd.clone()
    }
}

/// Snapshot the context menu overlay (if open).
fn build_context_menu(state: &GpuRuntimeState) -> Option<ContextMenu> {
    state.overlays.context_menu.as_ref().map(|m| ContextMenu {
        x_px: m.x_px as f32,
        y_px: m.y_px as f32,
        items: m.items.clone(),
        enabled_items: m.enabled_items.clone(),
        hovered_item: m.hovered_item,
    })
}

/// Snapshot the command palette overlay (if open).
fn build_command_palette_snapshot(state: &GpuRuntimeState) -> Option<CommandPalette> {
    state.command_palette.as_ref().map(|cp| {
        let sub_prompt_label = cp.sub_prompt.as_ref().map(|sp| match sp {
            crate::state::SubPrompt::Ssh => "SSH → New connection (user@host):".to_owned(),
        });
        let items: Vec<String> = if sub_prompt_label.is_some() {
            vec![]
        } else {
            cp.filtered
                .iter()
                .map(|&i| cp.all_items[i].label.clone())
                .collect()
        };
        let cursor_char = cp.query[..cp.cursor_byte.min(cp.query.len())]
            .chars()
            .count();
        CommandPalette {
            query: cp.query.clone(),
            cursor_char,
            items,
            selected: cp.selected,
            scroll_offset: cp.scroll_offset,
            sub_prompt_label,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{tab_button_label, tab_button_label_for_tab};

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

    #[test]
    fn tab_button_label_uses_running_command() {
        assert_eq!(
            tab_button_label_for_tab(0, None, "/tmp/project", true, Some("cargo test"), 20),
            "Cmd+1  [run] cargo test"
        );
    }

    #[test]
    fn tab_button_label_uses_running_cwd_when_no_title_or_command() {
        assert_eq!(
            tab_button_label_for_tab(0, None, "/tmp/project", true, None, 20),
            "Cmd+1  [run] /tmp/project"
        );
    }

    #[test]
    fn tab_button_label_prefers_per_tab_cwd_over_generic_shell_title() {
        assert_eq!(
            tab_button_label_for_tab(1, Some("shell"), "/tmp/project", false, None, 20),
            "Cmd+2  /tmp/project"
        );
    }
}
