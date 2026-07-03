use std::sync::Arc;

use crate::GpuRuntimeState;
use crate::coords::{
    TerminalLayout, current_line_prefix, cursor_at_line_end, cursor_to_terminal_cell,
    detect_terminal_links, read_child_cwd, shorten_cwd_label,
};
use crate::settings::build_settings_overlay;
use crate::theme;
use editor_lang::{LanguageHighlighter, ShellLikeHighlighter};
use render_model::{
    ColorTheme, CommandPalette, ContextMenu, DamageRegion, RenderCell, RenderRow, RenderSnapshot,
    SearchPanel, SnapshotImage, StickyCommandOverlay, SuggestionDropdown, TerminalLink, Toast,
    ToastKind,
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

fn truncate_overlay_command(s: &str, max_chars: usize) -> String {
    let first = s.lines().next().unwrap_or("");
    let compact = first.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return "command".to_owned();
    }
    truncate_display(&compact, max_chars)
}

fn tab_button_label(index: usize, title: Option<&str>, cwd: &str, max_chars: usize) -> String {
    let label_text = title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(|title| truncate_display(title, max_chars))
        .unwrap_or_else(|| shorten_cwd_label(cwd, max_chars));
    format!("Cmd+{}  {}", index + 1, label_text)
}

pub(crate) fn tab_button_label_for_tab(
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

pub(crate) fn tab_button_max_chars(tab_width_px: f32, cell_w_px: f32) -> usize {
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
    crate::tick::housekeeping(state);

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

    let (copy_mode_highlights, copy_mode_cursor) = build_copy_mode_section(state, active);

    let terminal_images = build_terminal_images(state, active, scroll_offset);

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
    let sticky_command_overlay = build_sticky_command_overlay(state, active);
    state.overlays.sticky_command_prompt_row =
        sticky_command_overlay.as_ref().map(|o| o.prompt_row);

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
            copy_mode_highlights,
            copy_mode_cursor,
            terminal_images,
            terminal_links,
            resize_overlay,
            selection,
            tab_labels,
            tab_drag_insert_before,
            context_menu,
            toast_stack,
            suggestion_dropdown,
            command_palette,
            sticky_command_overlay,
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
    copy_mode_highlights: Vec<(usize, usize, usize)>,
    copy_mode_cursor: Option<(usize, usize)>,
    terminal_images: Vec<SnapshotImage>,
    terminal_links: Vec<TerminalLink>,
    resize_overlay: Option<String>,
    selection: Option<(usize, usize, usize, usize)>,
    tab_labels: Vec<String>,
    tab_drag_insert_before: Option<usize>,
    context_menu: Option<ContextMenu>,
    toast_stack: Vec<Toast>,
    suggestion_dropdown: Option<SuggestionDropdown>,
    command_palette: Option<CommandPalette>,
    sticky_command_overlay: Option<StickyCommandOverlay>,
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
        copy_mode_highlights: f.copy_mode_highlights,
        copy_mode_cursor: f.copy_mode_cursor,
        terminal_images: f.terminal_images,
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
        sticky_command_overlay: f.sticky_command_overlay,
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
        opacity: state.user_config.terminal.opacity,
    }
}

fn build_sticky_command_overlay(
    state: &GpuRuntimeState,
    active: usize,
) -> Option<StickyCommandOverlay> {
    let tab = &state.tabs[active];
    if tab.app.is_alternate_screen() {
        return None;
    }
    if state.overlays.active_modal.is_some()
        || state.command_palette.is_some()
        || state.overlays.context_menu.is_some()
    {
        return None;
    }

    let visible_rows = tab.term_row_count.max(1);
    let scrollback = tab.app.scrollback_len();
    let total_rows = scrollback.saturating_add(visible_rows);
    if total_rows == 0 {
        return None;
    }
    let window_start = total_rows
        .saturating_sub(visible_rows)
        .saturating_sub(tab.scroll_offset.min(scrollback));
    let window_end = window_start.saturating_add(visible_rows);

    let prompt_marks = tab.app.prompt_marks();
    if prompt_marks.is_empty() {
        return None;
    }

    let mut sticky_candidates: Vec<(usize, usize, usize)> = Vec::new();
    for (idx, &prompt_row) in prompt_marks.iter().enumerate() {
        let end_row = prompt_marks
            .get(idx + 1)
            .copied()
            .map(|next| next.saturating_sub(1))
            .unwrap_or_else(|| total_rows.saturating_sub(1));
        let intersects_view = prompt_row < window_end && end_row >= window_start;
        let prompt_hidden = prompt_row < window_start;
        let first_non_prompt_visible = window_start.max(prompt_row.saturating_add(1));
        let has_visible_non_prompt_rows =
            first_non_prompt_visible < window_end && first_non_prompt_visible <= end_row;
        if intersects_view && prompt_hidden && has_visible_non_prompt_rows {
            sticky_candidates.push((idx, prompt_row, end_row));
        }
    }

    let (block_idx, prompt_row, _end_row) = sticky_candidates
        .into_iter()
        .max_by_key(|(_, prompt_row, _)| *prompt_row)?;

    // Find the command text from completed or current blocks by prompt row.
    // This replaces the old index-based logic that could desync with history.
    let raw_command = if let Some(block) = tab.command_blocks.get(block_idx) {
        block.command.clone()
    } else if block_idx == tab.command_blocks.len() {
        tab.current_block
            .as_ref()
            .map(|b| b.command.clone())
            .unwrap_or_default()
    } else {
        // Fallback: shouldn't happen, but use empty string if index is out of range.
        String::new()
    };

    let max_chars = if state.layout.cell_w > 0.0 {
        ((state.layout.window_width as f32 / state.layout.cell_w).floor() as usize)
            .saturating_sub(4)
            .max(8)
    } else {
        80
    };

    Some(StickyCommandOverlay {
        text: truncate_overlay_command(&raw_command, max_chars),
        prompt_row,
    })
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

fn pick_high_contrast_color(bg: [f32; 3], candidates: &[[f32; 3]], fallback: [f32; 3]) -> [f32; 3] {
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
    let mut terminal_text = String::with_capacity(styled.len());
    let mut terminal_fg_colors = Vec::with_capacity(styled.len());
    let mut terminal_bg_colors = Vec::with_capacity(styled.len());
    let mut terminal_styles = Vec::with_capacity(styled.len());
    let mut terminal_rows: Vec<RenderRow> =
        Vec::with_capacity(state.tabs[active].term_row_count.max(1));
    let mut current_row: Vec<RenderCell> = Vec::new();
    for (ch, fg, bg, style) in &styled {
        terminal_text.push(*ch);
        terminal_fg_colors.push(*fg);
        terminal_bg_colors.push(*bg);
        terminal_styles.push(*style);

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
    state.tabs[active].last_terminal_text = Arc::new(terminal_text.clone());
    state.tabs[active].term_row_count = terminal_rows.len().max(1);
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

/// `(highlights, cursor_position)` for copy mode rendering.
type CopyModeSection = (Vec<(usize, usize, usize)>, Option<(usize, usize)>);

/// Build copy mode highlights and cursor position for the active tab.
fn build_copy_mode_section(state: &GpuRuntimeState, active: usize) -> CopyModeSection {
    let tab = &state.tabs[active];
    if !tab.copy_mode.active {
        return (Vec::new(), None);
    }

    let visible_rows = tab.term_row_count.max(1);
    let total_rows = (tab.app.scrollback_len() + visible_rows).max(visible_rows);
    let window_start = total_rows
        .saturating_sub(visible_rows)
        .saturating_sub(tab.scroll_offset.min(tab.app.scrollback_len()));
    let window_end = window_start.saturating_add(visible_rows);

    // Build selection highlights if anchor is set
    let mut highlights = Vec::new();
    if let Some((anchor_row, anchor_col)) = tab.copy_mode.anchor {
        let cursor_row = tab.copy_mode.cursor_row;
        let cursor_col = tab.copy_mode.cursor_col;

        // Normalize selection bounds
        let (start_row, start_col, end_row, end_col) =
            if anchor_row > cursor_row || (anchor_row == cursor_row && anchor_col > cursor_col) {
                (cursor_row, cursor_col, anchor_row, anchor_col)
            } else {
                (anchor_row, anchor_col, cursor_row, cursor_col)
            };

        // Convert from scrollback-relative coordinates to viewport coordinates
        // scrollback_len() + rows covers the entire terminal height (scrollback + visible grid)
        // Row 0 = current screen bottom (latest output), negative rows = scrollback
        let scrollback_len = tab.app.scrollback_len() as isize;
        let abs_start_row = (scrollback_len + start_row) as usize;
        let abs_end_row = (scrollback_len + end_row) as usize;

        // Add selection highlights for all rows in range
        if abs_start_row < window_end && abs_end_row >= window_start {
            let vis_start_row = abs_start_row.saturating_sub(window_start);
            let vis_end_row = (abs_end_row + 1).min(window_end) - window_start;

            if vis_start_row == vis_end_row {
                // Single-row selection
                highlights.push((vis_start_row, start_col, end_col));
            } else {
                // Multi-row selection: highlight entire rows in between
                if abs_start_row >= window_start {
                    highlights.push((vis_start_row, start_col, 200)); // Start row: from start_col to EOL
                }
                for row in (abs_start_row + 1)..abs_end_row {
                    if row >= window_start && row < window_end {
                        let vis_row = row - window_start;
                        highlights.push((vis_row, 0, 200)); // Full row width
                    }
                }
                if abs_end_row < window_end {
                    let vis_row = abs_end_row - window_start;
                    highlights.push((vis_row, 0, end_col)); // End row: from 0 to end_col
                }
            }
        }
    }

    // Build cursor position (viewport coordinates)
    let scrollback_len = tab.app.scrollback_len() as isize;
    let abs_cursor_row = (scrollback_len + tab.copy_mode.cursor_row) as usize;
    let cursor_pos = if abs_cursor_row >= window_start && abs_cursor_row < window_end {
        Some((abs_cursor_row - window_start, tab.copy_mode.cursor_col))
    } else {
        None
    };

    (highlights, cursor_pos)
}

/// Build terminal image list in viewport coordinates.
fn build_terminal_images(
    _state: &GpuRuntimeState,
    _active: usize,
    _scroll_offset: usize,
) -> Vec<SnapshotImage> {
    // TODO: Once AppTerminal exposes screen images publicly, project them here.
    // For now, images are stored on the screen but not displayed via snapshot.
    // The infrastructure is in place (sixel decoder → screen.place_image → images vec)
    // but rendering support requires exposing the images through the App/AppTerminal API.
    Vec::new()
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
            .and_then(|full| full.strip_prefix(prefix))
            .map(|suffix| truncate_display(suffix, 80))
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
            .and_then(|full| full.strip_prefix(prefix).map(str::to_owned))
            .map(|suffix| truncate_display(&suffix, 80))
            .unwrap_or_default()
        }
    } else {
        String::new()
    }
}

/// Build the transient overlay label shown in the top-right corner (resize, PTY status, etc.).
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
            crate::state::SubPrompt::SnippetPlaceholders {
                placeholders,
                current_placeholder_idx,
                options,
                current_option_idx,
                ..
            } => {
                if *current_placeholder_idx < placeholders.len() {
                    let placeholder_name = &placeholders[*current_placeholder_idx];
                    let available_options = options.get(*current_placeholder_idx);
                    if let Some(opts) = available_options {
                        if opts.is_empty() {
                            // No options available: accept free-form input
                            format!(
                                "Enter {} (type value or press Enter to skip):",
                                placeholder_name
                            )
                        } else {
                            // Show dropdown options
                            let mut label =
                                format!("Select {} (↑/↓ to navigate):\n", placeholder_name);
                            for (idx, opt) in opts.iter().enumerate() {
                                if idx == *current_option_idx {
                                    label.push_str(&format!("  > {}\n", opt));
                                } else {
                                    label.push_str(&format!("    {}\n", opt));
                                }
                            }
                            label
                        }
                    } else {
                        format!("Select {} (loading options...):", placeholder_name)
                    }
                } else {
                    "Executing snippet...".to_owned()
                }
            }
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
