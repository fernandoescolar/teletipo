use std::sync::Arc;

use crate::GpuRuntimeState;
use crate::coords::{
    current_line_prefix, cursor_at_line_end, cursor_to_terminal_cell, detect_terminal_links,
    read_child_cwd, shorten_cwd_label,
};
use crate::settings::build_settings_overlay;
use crate::theme;
use render_wgpu::{
    ColorTheme, CommandPalette, ContextMenu, DamageRegion, RenderCell, RenderRow, RenderSnapshot,
    SearchPanel, SuggestionDropdown, TerminalLink, Toast, ToastKind,
};

/// Compute the rows hidden by collapsed blocks, in absolute row coordinates.
/// Each entry is `(start_abs, len)`: the block's output rows after the first
/// one (which stays visible as the "… N output lines" placeholder).
/// Sorted ascending and non-overlapping.
pub(crate) fn build_hidden_ranges(
    execution_blocks: &[app_orchestrator::ExecutionBlock],
    collapsed: &std::collections::HashSet<app_orchestrator::BlockId>,
    total_rows: usize,
) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = execution_blocks
        .iter()
        .filter(|b| collapsed.contains(&b.id))
        .filter_map(|b| {
            let s = b.output_start_row?;
            let e = b.output_end_row?.min(total_rows);
            let len = e.checked_sub(s + 1)?;
            (len > 0).then_some((s + 1, len))
        })
        .collect();
    ranges.sort_by_key(|&(start, _)| start);
    ranges
}

/// Number of hidden rows strictly below absolute row `r`.
fn hidden_before(r: usize, ranges: &[(usize, usize)]) -> usize {
    ranges
        .iter()
        .map(|&(start, len)| {
            if r <= start {
                0
            } else {
                len.min(r - start)
            }
        })
        .sum()
}

/// `true` when absolute row `r` is inside a collapsed block's hidden span.
pub(crate) fn is_hidden_row(r: usize, ranges: &[(usize, usize)]) -> bool {
    ranges
        .iter()
        .any(|&(start, len)| r >= start && r < start + len)
}

/// Virtual index of a visible absolute row: its position once hidden rows
/// are skipped.
pub(crate) fn virtual_index(r: usize, ranges: &[(usize, usize)]) -> usize {
    r.saturating_sub(hidden_before(r, ranges))
}

/// Absolute row of virtual index `v` (inverse of [`virtual_index`]).
pub(crate) fn abs_of_virtual(v: usize, ranges: &[(usize, usize)]) -> usize {
    let mut r = v;
    for &(start, len) in ranges {
        if r >= start {
            r += len;
        } else {
            break;
        }
    }
    r
}

/// Convert a viewport row to an absolute terminal row, mirroring the
/// collapse-aware geometry `build_snapshot` used for the last frame.
pub(crate) fn tab_view_row_to_abs(tab: &crate::tab::TabState, view_row: usize) -> usize {
    // Use the v_start cached by the last build_snapshot call so that click
    // handlers use the exact same geometry that placed the pixels, even if
    // the scrollback has grown since then.
    abs_of_virtual(
        tab.last_frame_v_start.saturating_add(view_row),
        &tab.collapsed_hidden_ranges,
    )
}

/// Virtual scroll offset that centers an absolute row in the viewport.
pub(crate) fn tab_scroll_offset_to_center(tab: &crate::tab::TabState, target_row: usize) -> usize {
    let visible = tab.term_row_count.max(1);
    let total = tab.app.scrollback_len().saturating_add(visible);
    let ranges = &tab.collapsed_hidden_ranges;
    let total_hidden: usize = ranges.iter().map(|&(_, len)| len).sum();
    let virtual_total = total.saturating_sub(total_hidden);
    let virtual_scrollback = virtual_total.saturating_sub(visible);
    let v_target = virtual_index(target_row, ranges);
    let v_start = v_target.saturating_sub(visible / 2).min(virtual_scrollback);
    virtual_scrollback.saturating_sub(v_start)
}

/// Flatten `terminal_rows` into parallel fg/bg/style vectors that the painter
/// uses for per-character color lookups.  Must be called *after* any collapse
/// splicing so the indices stay in sync with `terminal_text_from_rows`.
#[allow(clippy::type_complexity)]
fn flatten_rows_colors(
    rows: &[RenderRow],
) -> (Vec<Option<[f32; 3]>>, Vec<Option<[f32; 3]>>, Vec<u8>) {
    let mut fg = Vec::new();
    let mut bg = Vec::new();
    let mut style = Vec::new();
    for (row_idx, row) in rows.iter().enumerate() {
        for cell in &row.cells {
            fg.push(cell.fg);
            bg.push(cell.bg);
            style.push(cell.style);
        }
        if row_idx + 1 < rows.len() {
            fg.push(None);
            bg.push(None);
            style.push(0);
        }
    }
    (fg, bg, style)
}

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

    // Re-arm the update check once per day while the app stays open.
    const UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
    if state.update_rx.is_none()
        && state.overlays.pending_update.is_none()
        && state.update_last_checked.elapsed() >= UPDATE_INTERVAL
    {
        state.update_rx = Some(crate::updater::spawn_update());
        state.update_last_checked = std::time::Instant::now();
    }
    // shows the update banner for testing without needing to run the background thread or build an actual update:
    // state.overlays.pending_update.get_or_insert(crate::UpdateBanner::Available("TEST".to_owned()));

    // Safety net: if a tab still has deferred restore content (e.g. no resize
    // event fired before the first frame), replay it now — before pumping the
    // PTY — so restored output stays above any live shell prompt.
    for idx in 0..state.tabs.len() {
        if state.tabs[idx].pending_restore.is_some() {
            state.flush_pending_restore(idx);
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
    state.tabs[state.active_tab].bell_pending = false;

    // Show a one-shot toast for config parse errors on startup.
    if let Some(err) = state.config_error.take() {
        state.push_toast(
            format!("Config error: {err}"),
            crate::state::ToastKind::Error,
        );
    }

    let active = state.active_tab;

    let real_scrollback = state.tabs[active].app.scrollback_len();
    let active_palette: Option<[[f32; 3]; 16]> = state
        .themes_fonts
        .active_theme_idx
        .map(|i| theme::build_ansi_palette(&state.themes_fonts.available_themes[i]));

    // Fetch one real window of the terminal at a scrollback offset and parse
    // it into render rows.
    let fetch_rows = |app: &app_orchestrator::App, off: usize| -> Vec<RenderRow> {
        let styled = app.terminal_styled_snapshot_at_offset_with_palette(off, active_palette.as_ref());
        let mut rows: Vec<RenderRow> = Vec::new();
        let mut current: Vec<RenderCell> = Vec::new();
        for (ch, fg, bg, style) in &styled {
            if *ch == '\n' {
                rows.push(RenderRow {
                    cells: std::mem::take(&mut current),
                    dirty: false,
                });
                continue;
            }
            current.push(RenderCell {
                ch: *ch,
                fg: *fg,
                bg: *bg,
                style: *style,
            });
        }
        rows.push(RenderRow {
            cells: current,
            dirty: false,
        });
        rows
    };

    let execution_blocks: Vec<_> = state.tabs[active]
        .app
        .execution_blocks()
        .iter()
        .chain(state.tabs[active].app.current_execution_block())
        .cloned()
        .collect();

    // Probe the live window to learn the viewport height, then compute the
    // virtual (collapse-aware) content geometry.  `scroll_offset` is stored
    // in VIRTUAL lines: collapsed rows simply do not exist in scroll space,
    // so the scrollbar and the wheel both move through visible content only.
    let bottom_rows = fetch_rows(&state.tabs[active].app, 0);
    let visible_count = bottom_rows.len().max(1);
    let total_rows = real_scrollback.saturating_add(visible_count);

    let hidden_ranges = build_hidden_ranges(
        &execution_blocks,
        &state.tabs[active].collapsed_blocks,
        total_rows,
    );
    let total_hidden: usize = hidden_ranges.iter().map(|&(_, len)| len).sum();
    let virtual_total = total_rows.saturating_sub(total_hidden);
    let virtual_scrollback = virtual_total.saturating_sub(visible_count);

    // Auto-clamp: collapsing can shrink scroll space below the current offset.
    if state.tabs[active].scroll_offset > virtual_scrollback {
        state.tabs[active].scroll_offset = virtual_scrollback;
    }
    let scroll_offset = state.tabs[active].scroll_offset;
    state.tabs[active].virtual_scrollback_lines = virtual_scrollback;
    state.tabs[active].collapsed_hidden_ranges = hidden_ranges.clone();

    // First virtual row shown in the viewport.
    let v_start = virtual_total
        .saturating_sub(visible_count)
        .saturating_sub(scroll_offset);
    state.tabs[active].last_frame_v_start = v_start;

    // Assemble the viewport from visible rows only.  When collapsed blocks
    // overlap the window this stitches rows from several real windows so the
    // screen always fills with adjacent content.
    let mut terminal_rows: Vec<RenderRow> = Vec::with_capacity(visible_count);
    if hidden_ranges.is_empty() && scroll_offset == 0 {
        terminal_rows = bottom_rows;
    } else {
        let mut windows: Vec<(usize, Vec<RenderRow>)> =
            vec![(total_rows - visible_count, bottom_rows)];
        for i in 0..visible_count {
            let r = abs_of_virtual(v_start + i, &hidden_ranges);
            let cached = windows
                .iter()
                .find(|(ws, rows)| r >= *ws && r < ws + rows.len())
                .map(|(ws, rows)| rows[r - ws].clone());
            let row = if let Some(row) = cached {
                row
            } else {
                let off = total_rows
                    .saturating_sub(visible_count)
                    .saturating_sub(r)
                    .min(real_scrollback);
                let rows = fetch_rows(&state.tabs[active].app, off);
                let ws = total_rows.saturating_sub(visible_count).saturating_sub(off);
                let row = r
                    .checked_sub(ws)
                    .and_then(|idx| rows.get(idx).cloned())
                    .unwrap_or(RenderRow {
                        cells: Vec::new(),
                        dirty: true,
                    });
                windows.push((ws, rows));
                row
            };
            terminal_rows.push(row);
        }
    }

    // block_header_rows: collected for the painter to draw border-to-border
    // separator lines and overlay toolbar actions on each command-block header.
    let mut block_header_rows: Vec<render_wgpu::BlockHeaderRow> = Vec::new();

    for block in &execution_blocks {
        let Some(view_row) =
            virtual_index(block.prompt_start_row, &hidden_ranges).checked_sub(v_start)
        else {
            continue;
        };
        let Some(row) = terminal_rows.get_mut(view_row) else {
            continue;
        };
        let selected = state.tabs[active].selected_block == Some(block.id);

        block_header_rows.push(render_wgpu::BlockHeaderRow {
            row: view_row,
            selected,
            exit_code: block.exit_code,
        });

        // Clear every cell so that the row shows only the toolbar text; the
        // painter draws the tinted background and separator line as GPU rects.
        for cell in row.cells.iter_mut() {
            cell.ch = ' ';
            cell.fg = None;
            cell.bg = None;
        }

        // Status/elapsed text and toolbar baked into the rightmost cells.
        let collapsed = state.tabs[active].collapsed_blocks.contains(&block.id);
        let toolbar = crate::runtime::block_toolbar_text(collapsed);
        let elapsed = block.elapsed().map_or_else(
            || "—".to_owned(),
            |duration| {
                if duration.as_secs_f64() >= 1.0 {
                    format!("{:.1}s", duration.as_secs_f64())
                } else {
                    format!("{}ms", duration.as_millis())
                }
            },
        );
        let status = if block.exit_code == Some(0) {
            format!("✓ {elapsed}")
        } else if block.exit_code.is_some() {
            format!("✕ {elapsed}")
        } else if block.command.is_some() {
            format!("● {elapsed}")
        } else {
            "○".to_owned()
        };
        // Status/timing fg colour reflects success/failure/running.
        let status_fg = if block.exit_code == Some(0) {
            [0.42, 0.80, 0.54]
        } else if block.exit_code.is_some() {
            [0.92, 0.42, 0.44]
        } else {
            [0.60, 0.72, 1.0]
        };
        // The toolbar must occupy the last BLOCK_TOOLBAR_WIDTH columns exactly:
        // the pointer handler hit-tests it against the right edge.
        let label = format!("{status}  {toolbar}");
        let label_chars: Vec<char> = label.chars().collect();
        let n_cells = row.cells.len();
        let start = n_cells.saturating_sub(label_chars.len());
        let toolbar_start = label_chars
            .len()
            .saturating_sub(crate::runtime::BLOCK_TOOLBAR_WIDTH);
        let status_len = status.chars().count();
        // Muted colour for the space between status and toolbar buttons.
        let gap_fg: [f32; 3] = [0.35, 0.38, 0.44];
        // Brighter colour for interactive action labels.
        let action_fg: [f32; 3] = [0.78, 0.88, 1.0];
        for (i, (cell, ch)) in row.cells[start..]
            .iter_mut()
            .zip(label_chars.iter())
            .enumerate()
        {
            cell.ch = *ch;
            if i < status_len {
                cell.fg = Some(status_fg);
            } else if i < toolbar_start {
                cell.fg = Some(gap_fg);
            } else if crate::runtime::block_toolbar_action(
                i.saturating_sub(toolbar_start),
            )
            .is_some()
            {
                cell.fg = Some(action_fg);
            } else {
                cell.fg = Some(gap_fg);
            }
        }
    }

    // Collapsing is a render-only projection. The original output remains in
    // scrollback and is still available to copy or restore instantly.  The
    // hidden rows were already skipped during viewport assembly; here we only
    // overwrite the surviving first output row with the placeholder label.
    for block in execution_blocks
        .iter()
        .filter(|block| state.tabs[active].collapsed_blocks.contains(&block.id))
    {
        let (Some(s), Some(e)) = (block.output_start_row, block.output_end_row) else {
            continue;
        };
        if e <= s {
            continue;
        }
        let Some(idx) = virtual_index(s, &hidden_ranges).checked_sub(v_start) else {
            continue;
        };
        let Some(row) = terminal_rows.get_mut(idx) else {
            continue;
        };
        let label = format!("… {} output lines hidden · click to expand", e - s);
        let label_len = label.chars().count();
        if row.cells.len() < label_len {
            row.cells.resize(
                label_len,
                RenderCell {
                    ch: ' ',
                    fg: None,
                    bg: None,
                    style: 0,
                },
            );
        }
        for cell in &mut row.cells {
            cell.ch = ' ';
            cell.fg = None;
            cell.bg = Some([0.12, 0.12, 0.12]);
            cell.style = 0;
        }
        for (cell, ch) in row.cells.iter_mut().zip(label.chars()) {
            cell.ch = ch;
            // Bluish tint hints that the placeholder itself is clickable.
            cell.fg = Some([0.58, 0.66, 0.82]);
        }
        row.dirty = true;
    }

    // Rebuild the on-screen text from the final rows so copy, link detection
    // and the painter's per-character lookups all agree with what is shown.
    let terminal_text: String = {
        let mut text = String::new();
        for (i, row) in terminal_rows.iter().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            for cell in &row.cells {
                text.push(cell.ch);
            }
        }
        text
    };
    let (terminal_fg_colors, terminal_bg_colors, terminal_styles) =
        flatten_rows_colors(&terminal_rows);

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
    // Collapse remaps viewport rows, so per-row damage tracking no longer
    // lines up with screen rows — repaint everything while blocks are folded.
    damage.full_redraw = screen_damage.full_redraw || !hidden_ranges.is_empty();
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
    // Use the actual viewport row count: `lines().count()` drops a trailing
    // empty row, which skews the view-row → absolute-row mapping used by
    // block selection and the quick-action toolbar.
    state.tabs[active].term_row_count = terminal_rows.len().max(1);

    if state.tabs[active].search.active {
        crate::search::refresh_search(&mut state.tabs[active]);
    }

    let (search_panel, search_highlights, search_current_highlight) =
        if state.tabs[active].search.active {
            let tab = &state.tabs[active];
            // Map an absolute match row to its viewport row through the
            // collapse-aware virtual projection; hidden rows yield None.
            let to_view = |abs_row: usize| -> Option<usize> {
                if is_hidden_row(abs_row, &hidden_ranges) {
                    return None;
                }
                let view = virtual_index(abs_row, &hidden_ranges).checked_sub(v_start)?;
                (view < visible_count).then_some(view)
            };

            let highlights: Vec<(usize, usize, usize)> = tab
                .search
                .matches
                .iter()
                .filter_map(|m| to_view(m.abs_row).map(|row| (row, m.col_start, m.col_end)))
                .collect();

            let current = tab
                .search
                .matches
                .get(tab.search.current)
                .and_then(|m| to_view(m.abs_row).map(|row| (row, m.col_start, m.col_end)));

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
        // Pattern-detected links (URLs, file paths) from the rendered text.
        let mut all_links = detect_terminal_links(&terminal_text);

        // OSC 8 explicit hyperlinks from the terminal cell data.  These are
        // authoritative: if a cell range has an OSC 8 link ID we prefer it
        // over any pattern-detected link that overlaps the same cells.
        // OSC 8 spans are addressed by real scrollback offset; use the real
        // offset of the viewport top so links stay aligned when collapsed
        // blocks sit above the window.
        let real_link_offset = total_rows
            .saturating_sub(visible_count)
            .saturating_sub(abs_of_virtual(v_start, &hidden_ranges));
        let osc8 = state.tabs[active]
            .app
            .terminal
            .hyperlink_spans(real_link_offset);
        if !osc8.is_empty() {
            for (row, cs, ce, id) in &osc8 {
                if let Some(uri) = state.tabs[active].app.terminal.hyperlink_uri(*id) {
                    // Remove any pattern links that overlap this OSC 8 span.
                    all_links.retain(|(r, lcs, lce, _)| *r != *row || *lce <= *cs || *lcs >= *ce);
                    all_links.push((*row, *cs, *ce, uri.to_owned()));
                }
            }
        }

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

    let scrollback_lines = virtual_scrollback;
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
                format!("Update ready v{v} \u{2014} click to restart")
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

    let context_menu = state.overlays.context_menu.as_ref().map(|m| ContextMenu {
        x_px: m.x_px as f32,
        y_px: m.y_px as f32,
        items: m.items.clone(),
        enabled_items: m.enabled_items.clone(),
        hovered_item: m.hovered_item,
    });

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
        editor_horizontal_scroll_offset: state.tabs[active].editor_horizontal_scroll_offset,
        editor_selection,
        selection,
        search_highlights,
        search_current_highlight,
        tab_labels,
        active_tab,
        context_menu,
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
        command_palette: state.command_palette.as_ref().map(|cp| {
            let items: Vec<String> = cp
                .filtered
                .iter()
                .map(|&i| cp.all_items[i].label.clone())
                .collect();
            let cursor_char = cp.query[..cp.cursor_byte.min(cp.query.len())]
                .chars()
                .count();
            CommandPalette {
                query: cp.query.clone(),
                cursor_char,
                items,
                selected: cp.selected,
                scroll_offset: cp.scroll_offset,
            }
        }),
        block_header_rows,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        abs_of_virtual, is_hidden_row, tab_button_label, tab_button_label_for_tab, virtual_index,
    };

    #[test]
    fn virtual_mapping_skips_hidden_rows() {
        // Hidden: rows 5..10 (len 5) and 20..22 (len 2).
        let ranges = vec![(5, 5), (20, 2)];
        assert_eq!(virtual_index(0, &ranges), 0);
        assert_eq!(virtual_index(4, &ranges), 4);
        // First visible row after the first gap.
        assert_eq!(virtual_index(10, &ranges), 5);
        assert_eq!(virtual_index(19, &ranges), 14);
        assert_eq!(virtual_index(22, &ranges), 15);
        assert!(is_hidden_row(5, &ranges));
        assert!(is_hidden_row(9, &ranges));
        assert!(!is_hidden_row(10, &ranges));
        assert!(is_hidden_row(21, &ranges));
        // Round trip over visible rows.
        for r in (0..30).filter(|r| !is_hidden_row(*r, &ranges)) {
            assert_eq!(abs_of_virtual(virtual_index(r, &ranges), &ranges), r);
        }
    }

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
