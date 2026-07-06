use crate::GpuRuntimeState;
use crate::coords::{
    TerminalLayout, clamp_editor_scroll, cursor_to_terminal_cell, detect_terminal_links,
    editor_row_col_to_offset, editor_word_bounds, expand_tilde, extract_selection, strip_line_col,
};
use crate::launch::execute_context_menu_item;
use crate::search;
use platform_abstraction::{AppWindowEvent, InputState, PointerButton};
use render_model::SCROLLBAR_W_PX;
use std::time::Instant;

const TERMINAL_MENU_ITEMS: &[&str] = &["Copy", "Paste", "Scroll to Bottom"];
const EDITOR_MENU_ITEMS: &[&str] = &["Undo", "Redo", "Copy", "Cut", "Paste", "Select All"];
const CONTEXT_MENU_ROW_HEIGHT_FACTOR: f64 = 1.4;

fn context_menu_width_px(cell_w: f32, items: &[String]) -> f64 {
    let max_chars = items.iter().map(|s| s.chars().count()).max().unwrap_or(8);
    cell_w as f64 * (max_chars as f64 + 2.0)
}

pub(super) fn handle_event(state: &mut GpuRuntimeState, event: &AppWindowEvent) -> bool {
    if let AppWindowEvent::WindowMoved { x, y } = event {
        state.layout.window_x = *x;
        state.layout.window_y = *y;
        return true;
    }
    if let AppWindowEvent::Resized {
        width,
        height,
        scale_factor,
        cell_w,
        cell_h,
    } = event
    {
        state.layout.window_width = *width;
        state.layout.window_height = *height;
        state.layout.scale_factor = *scale_factor;
        state.layout.cell_w = *cell_w;
        state.layout.cell_h = *cell_h;
        // Keep terminal sessions informed of the current cell pixel size so
        // they can respond to OSC 1337;ReportCellSize queries correctly.
        for tab in state.tabs.iter_mut() {
            tab.app.set_cell_size(*cell_w, *cell_h);
        }
        // Freeze the terminal grid during rapid OS window resize. Both the
        // visual grid resize and SIGWINCH are deferred; the single
        // apply_deferred_resize call fires once the gesture has settled.
        let tab_bar_h = state.tab_bar_h();
        let available_h = *height as f32 - tab_bar_h;
        let pad_h = state.user_config.padding.horizontal as f32;
        let pad_v = state.user_config.padding.vertical as f32;
        let cols = ((*width as f32 - 2.0 * pad_h) / cell_w).max(1.0) as u16;
        let active = state.active_tab;
        let term_h = (available_h * state.tabs[active].split_ratio - 2.0 * pad_v).max(*cell_h);
        let rows = (term_h / cell_h).max(1.0) as u16;
        let now = Instant::now();
        state.overlays.last_resize = Some((now, cols, rows));
        if !state.overlays.initial_resize_done {
            // First resize corrects the hardcoded startup cell metrics. Apply
            // immediately so the PTY is sized correctly before the shell draws
            // its initial prompt. Skip the suppress window: nothing has been
            // output yet so there is no prompt-redraw noise to hide.
            state.overlays.initial_resize_done = true;
            state.apply_deferred_resize();
            for tab in state.tabs.iter_mut() {
                tab.suppress_until = None;
            }
        } else {
            state.overlays.pending_pty_resize = Some(now);
        }
        return true;
    }

    if let AppWindowEvent::CursorMoved { x, y } = event {
        return handle_cursor_moved(state, *x, *y);
    }

    if let AppWindowEvent::MouseInput {
        state: btn_state,
        button,
    } = event
    {
        return handle_mouse_input(state, *btn_state, *button);
    }

    if let AppWindowEvent::MouseWheel { delta_lines } = event {
        return handle_mouse_wheel(state, *delta_lines);
    }

    if let AppWindowEvent::ModifiersChanged(mods) = event {
        state.modifiers.ctrl_down = mods.ctrl;
        state.modifiers.super_down = mods.super_key;
        state.modifiers.shift_down = mods.shift;
        state.modifiers.alt_down = mods.alt;
        return true;
    }

    false
}

#[allow(clippy::too_many_lines)]
fn handle_cursor_moved(state: &mut GpuRuntimeState, x: f64, y: f64) -> bool {
    state.cursor.cursor_x = x;
    state.cursor.cursor_y = y;
    let tab_bar_h = state.tab_bar_h() as f64;
    let split_ratio = state.tab().split_ratio;

    if let Some(menu) = state.overlays.context_menu.as_mut() {
        let menu_w = context_menu_width_px(state.layout.cell_w, &menu.items);
        let menu_item_h = state.layout.cell_h as f64 * CONTEXT_MENU_ROW_HEIGHT_FACTOR;
        let menu_h = menu_item_h * menu.items.len() as f64;
        let mx = menu
            .x_px
            .min(state.layout.window_width as f64 - menu_w)
            .max(0.0);
        let my = menu
            .y_px
            .min(state.layout.window_height as f64 - menu_h)
            .max(0.0);
        let in_menu = x >= mx && x <= mx + menu_w && y >= my && y <= my + menu_h;
        menu.hovered_item = if in_menu {
            let item = ((y - my) / menu_item_h) as usize;
            if item < menu.items.len() && menu.enabled_items.get(item) == Some(&true) {
                Some(item)
            } else {
                None
            }
        } else {
            None
        };
    }

    if state.drag.dragging_separator {
        if state.tab().app.is_alternate_screen() {
            state.drag.dragging_separator = false;
            return true;
        }
        let available_h = state.layout.window_height as f64 - tab_bar_h;
        let new_ratio = (y - tab_bar_h) / available_h;
        let pad_v_f = state.user_config.padding.vertical as f32;
        let max_ratio = (1.0 - (state.layout.cell_h + 2.0 * pad_v_f) / available_h as f32).max(0.2);
        state.tab_mut().split_ratio = (new_ratio as f32).clamp(0.2, max_ratio);
        let active = state.active_tab;
        let sr = state.tabs[active].split_ratio;
        let pad_h = state.user_config.padding.horizontal as f32;
        let pad_v = state.user_config.padding.vertical as f32;
        let cols = ((state.layout.window_width as f32 - 2.0 * pad_h) / state.layout.cell_w).max(1.0)
            as u16;
        let term_h = (available_h as f32 * sr - 2.0 * pad_v).max(state.layout.cell_h);
        let rows = (term_h / state.layout.cell_h).max(1.0) as u16;
        // Visual-only resize during drag — defer SIGWINCH until release.
        state.resize_tab_visual(active, rows, cols);
        state.overlays.pending_pty_resize = Some(Instant::now());
    } else if state.drag.dragging_terminal_scrollbar {
        let available_h = state.layout.window_height as f64 - tab_bar_h;
        let term_bottom = tab_bar_h + available_h * state.tab().split_ratio as f64;
        if term_bottom > tab_bar_h {
            let frac = ((y - tab_bar_h) / (term_bottom - tab_bar_h)).clamp(0.0, 1.0);
            let max_scroll = state.tab().app.scrollback_len();
            state.tab_mut().scroll_offset = ((1.0 - frac) * max_scroll as f64) as usize;
        }
    } else if state.drag.dragging_editor_horizontal_scrollbar {
        let pad_h = state.user_config.padding.horizontal as f64;
        let track_w =
            (state.layout.window_width as f64 - 2.0 * pad_h - SCROLLBAR_W_PX as f64).max(1.0);
        let frac = ((x - pad_h) / track_w).clamp(0.0, 1.0);
        let editor_text = state.tab().app.editor_snapshot();
        let max_cols = editor_text
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        let visible_cols = if state.layout.cell_w > 0.0 {
            (track_w / state.layout.cell_w as f64).floor().max(1.0) as usize
        } else {
            1
        };
        let max_scroll = max_cols.saturating_sub(visible_cols);
        state.tab_mut().editor_horizontal_scroll_offset =
            (frac * max_scroll as f64).round() as usize;
    } else if state.drag.dragging_editor_scrollbar {
        let available_h = state.layout.window_height as f64 - tab_bar_h;
        let term_bottom = tab_bar_h + available_h * state.tab().split_ratio as f64;
        let edit_h_px = state.layout.window_height as f64 - term_bottom;
        if edit_h_px > 0.0 {
            let frac = ((y - term_bottom) / edit_h_px).clamp(0.0, 1.0);
            let editor_text = state.tab().app.editor_snapshot();
            let total_lines = editor_text.lines().count().max(1);
            let pad_v = state.user_config.padding.vertical as f32;
            let visible_rows = if state.layout.cell_h > 0.0 {
                ((edit_h_px as f32 - pad_v) / state.layout.cell_h)
                    .floor()
                    .max(1.0) as usize
            } else {
                1
            };
            let max_scroll = total_lines.saturating_sub(visible_rows);
            state.tab_mut().editor_scroll_offset = (frac * max_scroll as f64).round() as usize;
        }
    } else if state.tab().is_selecting {
        let term_row_count = state.tab().term_row_count;
        let pad_h = state.user_config.padding.horizontal as f32;
        let pad_v = state.user_config.padding.vertical as f32;
        if let Some(cell) = cursor_to_terminal_cell(
            x,
            y,
            state.layout.window_width,
            state.layout.window_height,
            &TerminalLayout {
                split_ratio,
                cell_w_px: state.layout.cell_w,
                cell_h_px: state.layout.cell_h,
                term_row_count,
                tab_bar_h: tab_bar_h as f32,
                pad_h,
                pad_v,
            },
        ) {
            state.tab_mut().selection_end = Some(cell);
            state.tab_mut().selection_end_scroll = state.tab().scroll_offset;
        }
    } else if state.tab().is_selecting_editor {
        let available_h = state.layout.window_height as f64 - tab_bar_h;
        let edit_top_px = tab_bar_h + split_ratio as f64 * available_h + 2.0;
        let editor_scroll_offset = state.tab().editor_scroll_offset;
        let pad_h = state.user_config.padding.horizontal as f64;
        let pad_v = state.user_config.padding.vertical as f64;
        let row = ((y - edit_top_px - pad_v) / state.layout.cell_h as f64)
            .max(0.0)
            .floor() as usize
            + editor_scroll_offset;
        let col = ((x - pad_h) / state.layout.cell_w as f64).max(0.0).floor() as usize
            + state.tab().editor_horizontal_scroll_offset;
        let text = state.tab().app.editor_snapshot();
        let offset = editor_row_col_to_offset(&text, row, col);
        state.tab_mut().app.set_editor_cursor(offset, true);
        clamp_editor_scroll(state);
    }

    // Forward cursor motion to PTY when in fullscreen with motion reporting.
    // Mode 1002: only when a mouse button is held (button-motion tracking).
    // Mode 1003: always (any-event tracking).
    // Shift or Alt/Option held: bypass reporting for local text selection.
    let mouse_mode = state.tab().app.mouse_mode();
    let bypass_mouse = state.modifiers.shift_down || state.modifiers.alt_down;
    if state.tab().app.is_alternate_screen() && mouse_mode >= 1002 {
        let should_send = !bypass_mouse
            && (mouse_mode == 1003
                || (mouse_mode == 1002 && state.cursor.mouse_btn_held.is_some()));
        if should_send {
            let tab_bar_h_f = state.tab_bar_h();
            let split_ratio = state.tab().split_ratio;
            let term_row_count = state.tab().term_row_count;
            let pad_h = state.user_config.padding.horizontal as f32;
            let pad_v = state.user_config.padding.vertical as f32;
            if let Some((row, col)) = cursor_to_terminal_cell(
                state.cursor.cursor_x,
                state.cursor.cursor_y,
                state.layout.window_width,
                state.layout.window_height,
                &TerminalLayout {
                    split_ratio,
                    cell_w_px: state.layout.cell_w,
                    cell_h_px: state.layout.cell_h,
                    term_row_count,
                    tab_bar_h: tab_bar_h_f,
                    pad_h,
                    pad_v,
                },
            ) {
                let encode_mode = if state.tab().app.mouse_sgr() {
                    1006
                } else {
                    mouse_mode
                };
                let bytes = encode_mouse_motion(state.cursor.mouse_btn_held, row, col, encode_mode);
                state.send_terminal_input(&bytes);
            }
        }
    }

    true
}

fn handle_mouse_input(
    state: &mut GpuRuntimeState,
    btn_state: InputState,
    button: PointerButton,
) -> bool {
    match button {
        PointerButton::Left => handle_left_button(state, btn_state),
        PointerButton::Middle => handle_middle_button(state, btn_state),
        PointerButton::Right => handle_right_button(state, btn_state),
        _ => false,
    }
}

#[allow(clippy::cognitive_complexity, clippy::too_many_lines)]
fn handle_left_button(state: &mut GpuRuntimeState, btn_state: InputState) -> bool {
    if btn_state == InputState::Released {
        if let Some(drag_from) = state.drag.tab_drag {
            if (state.cursor.cursor_x - state.drag.tab_drag_start_x).abs() > 5.0 {
                let n = state.tabs.len();
                let add_btn_w = state.layout.cell_w as f64 * 2.0;
                let tab_area_w = (state.layout.window_width as f64 - add_btn_w).max(1.0);
                let frac = (state.cursor.cursor_x / tab_area_w).clamp(0.0, 1.0);
                let insert_before = (frac * n as f64).round() as usize;
                state.move_tab_to(drag_from, insert_before);
            }
            state.drag.tab_drag = None;
        }
        if state.drag.dragging_separator && state.overlays.pending_pty_resize.is_some() {
            state.flush_pty_resize();
        }
        state.drag.dragging_separator = false;
        state.drag.dragging_terminal_scrollbar = false;
        state.drag.dragging_editor_scrollbar = false;
        state.drag.dragging_editor_horizontal_scrollbar = false;
        state.tab_mut().is_selecting = false;
        state.tab_mut().is_selecting_editor = false;
        // Track whether the press was forwarded to PTY before clearing.
        // If Shift was held during press, mouse_btn_held was never set.
        let press_was_forwarded = state.cursor.mouse_btn_held.is_some();
        state.cursor.mouse_btn_held = None;
        // Send mouse release to PTY only if the press was also forwarded.
        let mouse_mode = state.tab().app.mouse_mode();
        if mouse_mode != 0 && press_was_forwarded {
            let tab_bar_h = state.tab_bar_h() as f64;
            let split_ratio = state.tab().split_ratio;
            let term_row_count = state.tab().term_row_count;
            let pad_h = state.user_config.padding.horizontal as f32;
            let pad_v = state.user_config.padding.vertical as f32;
            if let Some((row, col)) = cursor_to_terminal_cell(
                state.cursor.cursor_x,
                state.cursor.cursor_y,
                state.layout.window_width,
                state.layout.window_height,
                &TerminalLayout {
                    split_ratio,
                    cell_w_px: state.layout.cell_w,
                    cell_h_px: state.layout.cell_h,
                    term_row_count,
                    tab_bar_h: tab_bar_h as f32,
                    pad_h,
                    pad_v,
                },
            ) {
                let encode_mode = if state.tab().app.mouse_sgr() {
                    1006
                } else {
                    mouse_mode
                };
                let bytes = encode_mouse_btn(0, row, col, false, encode_mode);
                state.send_terminal_input(&bytes);
            }
        }
    }
    if btn_state == InputState::Pressed {
        if matches!(
            state.overlays.pending_update,
            Some(crate::UpdateBanner::Available(_))
        ) {
            crate::updater::restart_app();
            return true;
        }

        // Command palette click handler.
        if state.command_palette.is_some() {
            let palette_w_px = state.layout.cell_w as f64 * 50.0;
            let win_w = state.layout.window_width as f64;
            let win_h = state.layout.window_height as f64;
            let tab_bar_h = state.tab_bar_h() as f64;
            let palette_x0 = (win_w - palette_w_px) / 2.0;
            let palette_x1 = palette_x0 + palette_w_px;
            let header_h = state.layout.cell_h as f64 * 2.2;
            let item_h = state.layout.cell_h as f64 * 1.4;
            let palette_y0 = tab_bar_h + win_h * 0.08;
            let n_visible = state
                .command_palette
                .as_ref()
                .map(|cp| {
                    cp.filtered
                        .len()
                        .saturating_sub(cp.scroll_offset)
                        .min(crate::state::PALETTE_MAX_VISIBLE_PUB)
                })
                .unwrap_or(0);
            let palette_y1 = palette_y0 + header_h + item_h * n_visible as f64;

            let cx = state.cursor.cursor_x;
            let cy = state.cursor.cursor_y;
            if cx >= palette_x0 && cx <= palette_x1 && cy >= palette_y0 && cy <= palette_y1 {
                // Click inside the palette: select + execute if on an item.
                let items_y0 = palette_y0 + header_h;
                if cy >= items_y0 {
                    let row = ((cy - items_y0) / item_h) as usize;
                    let scroll_offset = state
                        .command_palette
                        .as_ref()
                        .map(|cp| cp.scroll_offset)
                        .unwrap_or(0);
                    let item_abs = scroll_offset + row;
                    if let Some(cp) = state.command_palette.as_mut() {
                        cp.selected = item_abs.min(cp.filtered.len().saturating_sub(1));
                    }
                    crate::input::keyboard::palette_execute_from_pointer(state);
                }
            } else {
                // Click outside: close palette.
                state.close_active_modal();
            }
            return true;
        }

        if let Some(menu) = state.overlays.context_menu.clone() {
            if let Some(item) = menu.hovered_item {
                match menu.kind {
                    crate::state::ContextMenuKind::Tab { tab_idx } => {
                        execute_context_menu_item(state, tab_idx, item);
                    }
                    crate::state::ContextMenuKind::Terminal => {
                        execute_terminal_context_menu_item(state, item);
                    }
                    crate::state::ContextMenuKind::Editor => {
                        execute_editor_context_menu_item(state, item);
                    }
                }
            }
            state.overlays.context_menu = None;
            return true;
        }

        if state.tab().search.active
            && let Some(hitbox) = search::search_panel_hitbox(
                state.layout.window_width,
                state.tab_bar_h(),
                state.layout.cell_w,
                state.layout.cell_h,
                state.user_config.padding.horizontal as f32,
                state.user_config.padding.vertical as f32,
            )
            && search::in_panel(&hitbox, state.cursor.cursor_x, state.cursor.cursor_y)
        {
            if search::hit_close(&hitbox, state.cursor.cursor_x, state.cursor.cursor_y) {
                let q = state.tab().search.query.clone();
                if !q.is_empty() {
                    state.overlays.last_search_query = Some(q);
                }
                search::close_search(state.tab_mut());
            } else if search::hit_prev(&hitbox, state.cursor.cursor_x, state.cursor.cursor_y) {
                search::prev_match(state.tab_mut());
            } else if search::hit_next(&hitbox, state.cursor.cursor_x, state.cursor.cursor_y) {
                search::next_match(state.tab_mut());
            } else {
                // Click inside the query input area — position the cursor.
                let query_text_x = hitbox.panel_x
                    + state.layout.cell_w as f64 * search::QUERY_TEXT_OFFSET_CELLS as f64;
                let click_x = state.cursor.cursor_x - query_text_x;
                let cell_w = state.layout.cell_w as f64;
                let char_idx = if click_x <= 0.0 {
                    0
                } else {
                    ((click_x + cell_w * 0.5) / cell_w) as usize
                };
                search::search_set_cursor(state.tab_mut(), char_idx);
            }
            return true;
        }

        if let Some(prompt_row) = state.overlays.sticky_command_prompt_row
            && let Some(hitbox) = search::sticky_command_hitbox(
                state.layout.window_width,
                state.tab_bar_h(),
                state.layout.cell_h,
            )
            && search::in_sticky_command_overlay(
                &hitbox,
                state.cursor.cursor_x,
                state.cursor.cursor_y,
            )
        {
            let visible_rows = state.tab().term_row_count.max(1);
            let scrollback = state.tab().app.scrollback_len();
            let total_rows = scrollback.saturating_add(visible_rows);
            let max_start = total_rows.saturating_sub(visible_rows);
            let clamped_start = prompt_row.min(max_start);
            state.tab_mut().scroll_offset = total_rows
                .saturating_sub(visible_rows)
                .saturating_sub(clamped_start)
                .min(scrollback);
            state.push_accessibility_tree();
            return true;
        }

        let tab_bar_h = state.tab_bar_h() as f64;
        if state.cursor.cursor_y < tab_bar_h {
            let n = state.tabs.len();
            let add_btn_w = state.layout.cell_w as f64 * 2.0;
            let tab_area_w = state.layout.window_width as f64 - add_btn_w;

            if state.cursor.cursor_x >= state.layout.window_width as f64 - add_btn_w {
                state.add_new_tab();
                return true;
            }

            let tab_w = tab_area_w / n as f64;
            let tab_idx = (state.cursor.cursor_x / tab_w).min(n as f64 - 1.0) as usize;
            let close_w = state.layout.cell_w as f64 * 1.5;
            let tab_right = (tab_idx + 1) as f64 * tab_w;
            if state.cursor.cursor_x >= tab_right - close_w {
                state.close_tab(tab_idx);
            } else {
                state.active_tab = tab_idx;
                state.push_accessibility_tree();
                state.drag.tab_drag = Some(tab_idx);
                state.drag.tab_drag_start_x = state.cursor.cursor_x;
            }
            return true;
        }

        let split_ratio = state.tab().split_ratio;
        let available_h = state.layout.window_height as f64 - tab_bar_h;
        let sep_y_px = tab_bar_h + available_h * split_ratio as f64;
        let fullscreen = state.tab().app.is_alternate_screen();

        if !fullscreen && (state.cursor.cursor_y - sep_y_px).abs() < 6.0 {
            state.drag.dragging_separator = true;
            return true;
        }

        let sb_left = state.layout.window_width as f64 - SCROLLBAR_W_PX as f64;
        let term_bottom = sep_y_px;

        // Click on the "scrolled up" badge to jump back to bottom.
        if state.tab().scroll_offset > 0 {
            let pill_w = state.layout.cell_w as f64 * 14.0;
            let pill_h = state.layout.cell_h as f64 * 1.4;
            let margin = state.layout.cell_h as f64 * 0.5;
            let cx = state.layout.window_width as f64 / 2.0;
            let left = cx - pill_w / 2.0;
            let right = cx + pill_w / 2.0;
            let bottom = term_bottom - margin;
            let top = bottom - pill_h;
            let x = state.cursor.cursor_x;
            let y = state.cursor.cursor_y;
            if x >= left && x <= right && y >= top && y <= bottom {
                state.tab_mut().scroll_offset = 0;
                return true;
            }
        }

        if state.cursor.cursor_x >= sb_left
            && state.cursor.cursor_y >= tab_bar_h
            && state.cursor.cursor_y <= term_bottom
        {
            let frac = (state.cursor.cursor_y - tab_bar_h) / (term_bottom - tab_bar_h);
            let max_scroll = state.tab().app.scrollback_len();
            state.tab_mut().scroll_offset = ((1.0 - frac) * max_scroll as f64) as usize;
            state.drag.dragging_terminal_scrollbar = true;
            return true;
        }

        if !fullscreen
            && state.cursor.cursor_y >= state.layout.window_height as f64 - SCROLLBAR_W_PX as f64
            && state.cursor.cursor_x < sb_left
        {
            let pad_h = state.user_config.padding.horizontal as f64;
            let track_w = (sb_left - 2.0 * pad_h).max(1.0);
            let frac = ((state.cursor.cursor_x - pad_h) / track_w).clamp(0.0, 1.0);
            let editor_text = state.tab().app.editor_snapshot();
            let max_cols = editor_text
                .lines()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0);
            let visible_cols = if state.layout.cell_w > 0.0 {
                (track_w / state.layout.cell_w as f64).floor().max(1.0) as usize
            } else {
                1
            };
            let max_scroll = max_cols.saturating_sub(visible_cols);
            if max_scroll > 0 {
                state.tab_mut().editor_horizontal_scroll_offset =
                    (frac * max_scroll as f64).round() as usize;
                state.drag.dragging_editor_horizontal_scrollbar = true;
                return true;
            }
        }

        if !fullscreen && state.cursor.cursor_x >= sb_left && state.cursor.cursor_y > term_bottom {
            let edit_h_px = state.layout.window_height as f64 - term_bottom;
            if edit_h_px > 0.0 {
                let frac = (state.cursor.cursor_y - term_bottom) / edit_h_px;
                let editor_text = state.tab().app.editor_snapshot();
                let total_lines = editor_text.lines().count().max(1);
                let pad_v = state.user_config.padding.vertical as f32;
                let visible_rows = if state.layout.cell_h > 0.0 {
                    ((edit_h_px as f32 - pad_v) / state.layout.cell_h)
                        .floor()
                        .max(1.0) as usize
                } else {
                    1
                };
                let max_scroll = total_lines.saturating_sub(visible_rows);
                state.tab_mut().editor_scroll_offset = (frac * max_scroll as f64).round() as usize;
                state.drag.dragging_editor_scrollbar = true;
            }
            return true;
        }

        // Open a detected terminal link without starting selection.
        // macOS uses Command; Linux/Windows use Ctrl.
        let open_link_modifier_down = {
            #[cfg(target_os = "macos")]
            {
                state.modifiers.super_down
            }
            #[cfg(not(target_os = "macos"))]
            {
                state.modifiers.ctrl_down || state.modifiers.super_down
            }
        };
        if open_link_modifier_down {
            let pad_h = state.user_config.padding.horizontal as f32;
            let pad_v = state.user_config.padding.vertical as f32;
            let term_row_count = state.tab().term_row_count;
            if let Some((row, col)) = cursor_to_terminal_cell(
                state.cursor.cursor_x,
                state.cursor.cursor_y,
                state.layout.window_width,
                state.layout.window_height,
                &TerminalLayout {
                    split_ratio,
                    cell_w_px: state.layout.cell_w,
                    cell_h_px: state.layout.cell_h,
                    term_row_count,
                    tab_bar_h: tab_bar_h as f32,
                    pad_h,
                    pad_v,
                },
            ) {
                let last_text = state.tab().last_terminal_text.clone();
                let term_cols = if state.layout.cell_w > 0.0 {
                    let pad_h = state.user_config.padding.horizontal as f32;
                    ((state.layout.window_width as f32 - 2.0 * pad_h) / state.layout.cell_w).floor()
                        as usize
                } else {
                    0
                };
                let links = detect_terminal_links(&last_text, term_cols);
                if let Some((_, _, _, target)) = links
                    .iter()
                    .find(|(r, cs, ce, _)| *r == row && col >= *cs && col < *ce)
                {
                    let cwd = state.tab().cwd.clone();
                    let target = target.clone();
                    open_link(&mut *state.shell_services, &target, &cwd);
                    return true;
                }
            }
        }

        let term_row_count = state.tab().term_row_count;
        let pad_h = state.user_config.padding.horizontal as f32;
        let pad_v = state.user_config.padding.vertical as f32;
        if let Some(cell) = cursor_to_terminal_cell(
            state.cursor.cursor_x,
            state.cursor.cursor_y,
            state.layout.window_width,
            state.layout.window_height,
            &TerminalLayout {
                split_ratio,
                cell_w_px: state.layout.cell_w,
                cell_h_px: state.layout.cell_h,
                term_row_count,
                tab_bar_h: tab_bar_h as f32,
                pad_h,
                pad_v,
            },
        ) {
            // If a mouse reporting mode is active and neither Shift nor Alt/Option
            // is held, forward the click to the PTY. Either modifier bypasses mouse
            // reporting so the user can do a local text selection.
            let mouse_mode = state.tab().app.mouse_mode();
            if mouse_mode != 0 && !state.modifiers.shift_down && !state.modifiers.alt_down {
                let (row, col) = cell;
                let encode_mode = if state.tab().app.mouse_sgr() {
                    1006
                } else {
                    mouse_mode
                };
                let bytes = encode_mouse_btn(0, row, col, true, encode_mode);
                state.send_terminal_input(&bytes);
                state.cursor.mouse_btn_held = Some(0);
                return true;
            }

            // Detect double/triple clicks.
            const DOUBLE_CLICK_MS: u128 = 400;
            let now = Instant::now();
            let click_count = {
                if let (Some(last_t), Some(last_cell)) =
                    (state.cursor.last_click_time, state.cursor.last_click_cell)
                {
                    let elapsed = now.duration_since(last_t).as_millis();
                    if elapsed <= DOUBLE_CLICK_MS && last_cell == cell {
                        (state.cursor.click_count + 1).min(3)
                    } else {
                        1
                    }
                } else {
                    1
                }
            };
            state.cursor.last_click_time = Some(now);
            state.cursor.last_click_cell = Some(cell);
            state.cursor.last_click_was_editor = false;
            state.cursor.click_count = click_count;

            if click_count == 3 {
                // Triple-click: select the entire line.
                let (row, _col) = cell;
                let n_cols = state.tab().term_row_count.max(1);
                // Find actual terminal columns from the text.
                let last_text = state.tab().last_terminal_text.clone();
                let line_len = last_text
                    .lines()
                    .nth(row)
                    .map(|l| l.chars().count())
                    .unwrap_or(0);
                let end_col = line_len.saturating_sub(1);
                let scroll = state.tab().scroll_offset;
                state.tab_mut().selection_anchor = Some((row, 0));
                state.tab_mut().selection_anchor_scroll = scroll;
                state.tab_mut().selection_end = Some((row, end_col.max(n_cols)));
                state.tab_mut().selection_end_scroll = scroll;
                state.tab_mut().is_selecting = false;
            } else if click_count == 2 {
                // Double-click: select word under cursor.
                let (row, col) = cell;
                let last_text = state.tab().last_terminal_text.clone();
                let scroll = state.tab().scroll_offset;
                if let Some(line) = last_text.lines().nth(row) {
                    let chars: Vec<char> = line.chars().collect();
                    let col = col.min(chars.len().saturating_sub(1));
                    let is_word = |c: char| c.is_alphanumeric() || "_-./:~".contains(c);
                    // Expand left.
                    let mut start_col = col;
                    while start_col > 0 && is_word(chars[start_col - 1]) {
                        start_col -= 1;
                    }
                    // If the char at cursor isn't a word char, try single char.
                    let mut end_col = if col < chars.len() && is_word(chars[col]) {
                        col + 1
                    } else {
                        col.saturating_add(1)
                    };
                    while end_col < chars.len() && is_word(chars[end_col]) {
                        end_col += 1;
                    }
                    state.tab_mut().selection_anchor = Some((row, start_col));
                    state.tab_mut().selection_anchor_scroll = scroll;
                    state.tab_mut().selection_end =
                        Some((row, end_col.saturating_sub(1).max(start_col)));
                    state.tab_mut().selection_end_scroll = scroll;
                    state.tab_mut().is_selecting = false;
                }
            } else {
                // Single click: start drag selection.
                state.tab_mut().selection_anchor = Some(cell);
                state.tab_mut().selection_anchor_scroll = state.tab().scroll_offset;
                state.tab_mut().selection_end = Some(cell);
                state.tab_mut().selection_end_scroll = state.tab().scroll_offset;
                state.tab_mut().is_selecting = true;
            }
        } else if !fullscreen && state.cursor.cursor_y > term_bottom {
            let edit_top_px = term_bottom + 2.0;
            let editor_scroll_offset = state.tab().editor_scroll_offset;
            let pad_h_f = state.user_config.padding.horizontal as f64;
            let pad_v_f = state.user_config.padding.vertical as f64;
            let row = ((state.cursor.cursor_y - edit_top_px - pad_v_f) / state.layout.cell_h as f64)
                .max(0.0)
                .floor() as usize
                + editor_scroll_offset;
            let col = ((state.cursor.cursor_x - pad_h_f) / state.layout.cell_w as f64)
                .max(0.0)
                .floor() as usize
                + state.tab().editor_horizontal_scroll_offset;
            // Clicking in the editor clears any terminal text selection.
            state.tab_mut().selection_anchor = None;
            state.tab_mut().selection_end = None;
            state.tab_mut().is_selecting = false;
            let text = state.tab().app.editor_snapshot();
            let offset = editor_row_col_to_offset(&text, row, col);
            const DOUBLE_CLICK_MS: u128 = 400;
            let now = Instant::now();
            let is_double_click = state.cursor.last_click_was_editor
                && state.cursor.last_click_cell == Some((row, col))
                && state
                    .cursor
                    .last_click_time
                    .is_some_and(|last| now.duration_since(last).as_millis() <= DOUBLE_CLICK_MS);
            state.cursor.last_click_time = Some(now);
            state.cursor.last_click_cell = Some((row, col));
            state.cursor.last_click_was_editor = true;
            if is_double_click {
                let (start, end) = editor_word_bounds(&text, offset);
                state.tab_mut().app.set_editor_cursor(start, false);
                state.tab_mut().app.set_editor_cursor(end, true);
                state.tab_mut().is_selecting_editor = false;
                state.cursor.click_count = 2;
            } else {
                let extend = state.modifiers.shift_down;
                state.tab_mut().app.set_editor_cursor(offset, extend);
                state.tab_mut().is_selecting_editor = true;
                state.cursor.click_count = 1;
            }
            clamp_editor_scroll(state);
        }
    }
    true
}

fn handle_middle_button(state: &mut GpuRuntimeState, btn_state: InputState) -> bool {
    if btn_state == InputState::Pressed {
        let tab_bar_h = state.tab_bar_h() as f64;
        if state.cursor.cursor_y < tab_bar_h && state.tabs.len() > 1 {
            let n = state.tabs.len();
            let add_btn_w = state.layout.cell_w as f64 * 2.0;
            let tab_area_w = state.layout.window_width as f64 - add_btn_w;
            if state.cursor.cursor_x < state.layout.window_width as f64 - add_btn_w && n > 0 {
                let tab_w = tab_area_w / n as f64;
                let tab_idx = (state.cursor.cursor_x / tab_w).min(n as f64 - 1.0) as usize;
                state.close_tab(tab_idx);
            }
        }
        true
    } else {
        false
    }
}

fn handle_right_button(state: &mut GpuRuntimeState, btn_state: InputState) -> bool {
    if btn_state == InputState::Pressed {
        state.overlays.context_menu = None;

        let tab_bar_h = state.tab_bar_h() as f64;
        if state.cursor.cursor_y < tab_bar_h {
            let n = state.tabs.len();
            let add_btn_w = state.layout.cell_w as f64 * 2.0;
            let tab_area_w = state.layout.window_width as f64 - add_btn_w;
            if n > 0 && state.cursor.cursor_x < state.layout.window_width as f64 - add_btn_w {
                let tab_w = tab_area_w / n as f64;
                let tab_idx = (state.cursor.cursor_x / tab_w).min(n as f64 - 1.0) as usize;
                let tab_menu_commands = crate::command_registry::tab_context_menu_commands();
                state.overlays.context_menu = Some(crate::state::ContextMenuState {
                    kind: crate::state::ContextMenuKind::Tab { tab_idx },
                    x_px: state.cursor.cursor_x,
                    y_px: tab_bar_h,
                    hovered_item: None,
                    items: tab_menu_commands
                        .iter()
                        .map(|def| def.context_menu_label.unwrap_or(def.label).to_owned())
                        .collect(),
                    enabled_items: vec![true; tab_menu_commands.len()],
                });
            }
        } else {
            // Terminal/editor pane right-click menu.
            let available_h = state.layout.window_height as f64 - tab_bar_h;
            let term_bottom = if state.tab().app.is_alternate_screen() {
                state.layout.window_height as f64
            } else {
                tab_bar_h + available_h * state.tab().split_ratio as f64
            };
            if state.cursor.cursor_y >= tab_bar_h && state.cursor.cursor_y <= term_bottom {
                state.overlays.context_menu = Some(crate::state::ContextMenuState {
                    kind: crate::state::ContextMenuKind::Terminal,
                    x_px: state.cursor.cursor_x,
                    y_px: state.cursor.cursor_y,
                    hovered_item: None,
                    items: TERMINAL_MENU_ITEMS
                        .iter()
                        .map(|s| (*s).to_owned())
                        .collect(),
                    enabled_items: vec![true; TERMINAL_MENU_ITEMS.len()],
                });
            } else if state.cursor.cursor_y > term_bottom {
                let has_selection = state.tab().app.editor_selection().is_some();
                let has_text = !state.tab().app.editor_snapshot().is_empty();
                let can_paste = state
                    .shell_services
                    .clipboard_get()
                    .is_some_and(|text| !text.is_empty());
                state.overlays.context_menu = Some(crate::state::ContextMenuState {
                    kind: crate::state::ContextMenuKind::Editor,
                    x_px: state.cursor.cursor_x,
                    y_px: state.cursor.cursor_y,
                    hovered_item: None,
                    items: EDITOR_MENU_ITEMS.iter().map(|s| (*s).to_owned()).collect(),
                    enabled_items: vec![
                        state.tab().app.editor_can_undo(),
                        state.tab().app.editor_can_redo(),
                        has_selection,
                        has_selection,
                        can_paste,
                        has_text,
                    ],
                });
            }
        }
        true
    } else {
        false
    }
}

#[allow(clippy::too_many_lines)]
fn handle_mouse_wheel(state: &mut GpuRuntimeState, delta_lines: f32) -> bool {
    if state.overlays.context_menu.is_some() {
        state.overlays.context_menu = None;
    }
    let lines = delta_lines.round().abs().max(1.0) as usize;
    let tab_bar_h = state.tab_bar_h() as f64;
    let split_ratio = state.tab().split_ratio;
    let term_bottom =
        tab_bar_h + (state.layout.window_height as f64 - tab_bar_h) * split_ratio as f64;

    if state.cursor.cursor_y > term_bottom {
        let editor_text = state.tab().app.editor_snapshot();
        if state.modifiers.shift_down {
            let max_cols = editor_text
                .lines()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0);
            let visible_cols = if state.layout.cell_w > 0.0 {
                ((state.layout.window_width as f32
                    - 2.0 * state.user_config.padding.horizontal as f32)
                    / state.layout.cell_w)
                    .floor()
                    .max(1.0) as usize
            } else {
                1
            };
            let max_scroll = max_cols.saturating_sub(visible_cols);
            let prev = state.tab().editor_horizontal_scroll_offset;
            state.tab_mut().editor_horizontal_scroll_offset = if delta_lines > 0.0 {
                prev.saturating_sub(lines)
            } else {
                prev.saturating_add(lines).min(max_scroll)
            };
            return true;
        }
        let total_lines = editor_text.lines().count().max(1);
        let edit_h_px = state.layout.window_height as f64 - term_bottom;
        let pad_v = state.user_config.padding.vertical as f32;
        let visible_rows = if state.layout.cell_h > 0.0 {
            ((edit_h_px as f32 - pad_v) / state.layout.cell_h)
                .floor()
                .max(1.0) as usize
        } else {
            1
        };
        let max_scroll = total_lines.saturating_sub(visible_rows);
        let prev = state.tab().editor_scroll_offset;
        if delta_lines > 0.0 {
            state.tab_mut().editor_scroll_offset = prev.saturating_sub(lines);
        } else {
            state.tab_mut().editor_scroll_offset = prev.saturating_add(lines).min(max_scroll);
        }
    } else {
        let prev = state.tab().scroll_offset;
        // When mouse reporting is active, send scroll events to the PTY
        // instead of scrolling the local scrollback buffer.
        let mouse_mode = state.tab().app.mouse_mode();
        if mouse_mode != 0 {
            let tab_bar_h_f = state.tab_bar_h() as f64;
            let split_ratio = state.tab().split_ratio;
            let term_row_count = state.tab().term_row_count;
            let pad_h = state.user_config.padding.horizontal as f32;
            let pad_v = state.user_config.padding.vertical as f32;
            let term_bottom_for_scroll = tab_bar_h_f
                + (state.layout.window_height as f64 - tab_bar_h_f) * split_ratio as f64;
            if state.cursor.cursor_y < term_bottom_for_scroll
                && let Some((row, col)) = cursor_to_terminal_cell(
                    state.cursor.cursor_x,
                    state.cursor.cursor_y,
                    state.layout.window_width,
                    state.layout.window_height,
                    &TerminalLayout {
                        split_ratio,
                        cell_w_px: state.layout.cell_w,
                        cell_h_px: state.layout.cell_h,
                        term_row_count,
                        tab_bar_h: tab_bar_h_f as f32,
                        pad_h,
                        pad_v,
                    },
                )
            {
                // Button 64 = scroll up, 65 = scroll down.
                let btn = if delta_lines > 0.0 { 64u8 } else { 65u8 };
                let encode_mode = if state.tab().app.mouse_sgr() {
                    1006
                } else {
                    mouse_mode
                };
                for _ in 0..lines {
                    let bytes = encode_mouse_btn(btn, row, col, true, encode_mode);
                    state.send_terminal_input(&bytes);
                }
                return true;
            }
        }
        if delta_lines > 0.0 {
            let max_scroll = state.tab().app.scrollback_len();
            state.tab_mut().scroll_offset = prev.saturating_add(lines).min(max_scroll);
        } else {
            state.tab_mut().scroll_offset = prev.saturating_sub(lines);
        }
        state.push_accessibility_tree();
    }
    true
}

fn editor_selected_text(state: &GpuRuntimeState) -> Option<String> {
    let (start, end) = state.tab().app.editor_selection()?;
    state
        .tab()
        .app
        .editor_snapshot()
        .get(start..end)
        .map(str::to_owned)
}

fn copy_editor_selection(state: &mut GpuRuntimeState) -> bool {
    let Some(selected) = editor_selected_text(state).filter(|text| !text.is_empty()) else {
        return false;
    };
    let copied_len = selected.chars().count();
    state.shell_services.clipboard_set(selected);
    state.push_toast(
        format!("Copied {copied_len} chars"),
        crate::state::ToastKind::Success,
    );
    true
}

fn paste_into_editor(state: &mut GpuRuntimeState) {
    if let Some(text) = state.shell_services.clipboard_get() {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        if !normalized.is_empty() {
            state.tab_mut().app.insert_editor_input(&normalized);
        }
    }
}

pub(crate) fn execute_editor_context_menu_item(state: &mut GpuRuntimeState, item: usize) {
    match item {
        0 if state.tab().app.editor_can_undo() => state.tab_mut().app.editor_undo(),
        1 if state.tab().app.editor_can_redo() => state.tab_mut().app.editor_redo(),
        2 => {
            copy_editor_selection(state);
        }
        3 if copy_editor_selection(state) => state.tab_mut().app.editor_backspace(),
        4 => paste_into_editor(state),
        5 => {
            let end = state.tab().app.editor_snapshot().len();
            if end > 0 {
                state.tab_mut().app.set_editor_cursor(0, false);
                state.tab_mut().app.set_editor_cursor(end, true);
            }
        }
        _ => {}
    }
}

fn execute_terminal_context_menu_item(state: &mut GpuRuntimeState, item: usize) {
    match item {
        // Copy (smart-trim): prefer selection, trim trailing whitespace/newlines.
        0 => {
            let mut copied = String::new();
            if let (Some(anchor), Some(sel_end)) =
                (state.tab().selection_anchor, state.tab().selection_end)
            {
                let current_scroll = state.tab().scroll_offset as i64;
                let anchor_scroll = state.tab().selection_anchor_scroll as i64;
                let end_scroll = state.tab().selection_end_scroll as i64;
                let ar = (anchor.0 as i64 + current_scroll - anchor_scroll).max(0) as usize;
                let er = (sel_end.0 as i64 + current_scroll - end_scroll).max(0) as usize;
                let text = extract_selection(
                    &state.tab().last_terminal_text,
                    (ar, anchor.1),
                    (er, sel_end.1),
                );
                copied = text.trim_end_matches(['\n', '\r', ' ', '\t']).to_owned();
            }
            if !copied.is_empty() {
                let n = copied.chars().count();
                state.shell_services.clipboard_set(copied);
                state.push_toast(
                    format!("Copied {n} chars"),
                    crate::state::ToastKind::Success,
                );
            }
        }
        // Paste.
        1 => {
            if let Some(text) = state.shell_services.clipboard_get() {
                let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
                if !normalized.is_empty() {
                    let route_to_pty = state.tab().app.is_alternate_screen()
                        || (state.tab().command_running && !state.tab().editor_unlocked);
                    if route_to_pty {
                        if state.tab().app.bracketed_paste() {
                            let bracketed = format!("\x1b[200~{normalized}\x1b[201~");
                            state.send_terminal_input(bracketed.as_bytes());
                        } else {
                            state.send_terminal_input(normalized.as_bytes());
                        }
                    } else {
                        state.tab_mut().app.insert_editor_input(&normalized);
                    }
                }
            }
        }
        // Scroll to bottom.
        2 => {
            state.tab_mut().scroll_offset = 0;
        }
        _ => {}
    }
}

/// Encode a cursor-motion event for the PTY (modes 1002/1003).
///
/// `held_btn`: the button currently held (0=left, 1=mid, 2=right) or `None` when
/// reporting all-motion without a button (mode 1003).  The motion flag (bit 5 of
/// the button code) is applied automatically.
fn encode_mouse_motion(held_btn: Option<u8>, row: usize, col: usize, mouse_mode: u16) -> Vec<u8> {
    // Button code: held button with motion bit (32) OR button 3 (release/none) + motion.
    let btn_code = held_btn.map(|b| b + 32).unwrap_or(35);
    if mouse_mode == 1006 {
        format!("\x1b[<{};{};{}M", btn_code, col + 1, row + 1).into_bytes()
    } else {
        let b = btn_code.wrapping_add(32);
        let cx = (col + 1 + 32) as u8;
        let cy = (row + 1 + 32) as u8;
        vec![0x1b, b'[', b'M', b, cx, cy]
    }
}

/// Encode a mouse button event for the PTY.
///
/// `button`: 0 = left, 1 = middle, 2 = right, 64/65 = scroll up/down.
/// `row`, `col`: 0-based terminal grid coordinates.
/// `pressed`: `true` for press, `false` for release.
/// `mouse_mode`: the active reporting mode (1000/1002/1003 = X10, 1006 = SGR).
fn encode_mouse_btn(button: u8, row: usize, col: usize, pressed: bool, mouse_mode: u16) -> Vec<u8> {
    if mouse_mode == 1006 {
        // SGR encoding: \x1b[<btn;col+1;row+1M  (press) or m (release)
        let suffix = if pressed { 'M' } else { 'm' };
        format!("\x1b[<{};{};{}{}", button, col + 1, row + 1, suffix).into_bytes()
    } else {
        // X10 encoding: limited to col/row <= 222 (byte value saturates at 255).
        let b = button.wrapping_add(32);
        let cx = (col + 1 + 32) as u8;
        let cy = (row + 1 + 32) as u8;
        vec![0x1b, b'[', b'M', b, cx, cy]
    }
}

/// Open a terminal link (URL or file path), stripping any `:line:col` suffix,
/// resolving relative paths against `cwd`, and showing an alert on failure.
///
/// URLs (http/https/ftp) are delegated to the [`crate::shell::AppShell`] so
/// tests can capture them. Local file paths are still resolved + opened here
/// because the shell abstraction only covers URLs.
fn open_link(shell: &mut dyn crate::shell::AppShell, raw_target: &str, cwd: &str) {
    let is_url = raw_target.starts_with("http://")
        || raw_target.starts_with("https://")
        || raw_target.starts_with("ftp://");

    if is_url {
        shell.open_url(raw_target);
        return;
    }

    // OSC 8 can emit `file://[host]/absolute/path` URIs. Strip the scheme and
    // optional authority (hostname) component so the path handler below can
    // resolve it uniformly.
    //
    // RFC 8089 forms handled:
    //   file:///absolute/path        → /absolute/path
    //   file://hostname/absolute/path → /absolute/path
    let effective_target: &str = if let Some(rest) = raw_target.strip_prefix("file://") {
        // `rest` is either `/absolute/path` (empty host) or `hostname/path`.
        if rest.starts_with('/') {
            // file:///path — authority is empty, rest begins with the path.
            rest
        } else {
            // file://hostname/path — skip hostname up to first slash.
            rest.find('/').map(|i| &rest[i..]).unwrap_or(rest)
        }
    } else {
        raw_target
    };

    // File path: strip :line:col, expand ~, resolve relative paths.
    let bare = strip_line_col(effective_target);
    let expanded = expand_tilde(bare);
    let path = if std::path::Path::new(&expanded).is_absolute() {
        std::path::PathBuf::from(&expanded)
    } else {
        std::path::Path::new(cwd).join(&expanded)
    };

    if !path.exists() {
        show_alert(&format!("File not found:\n{}", bare));
        return;
    }

    // Extract line number suffix from the original target (e.g. "foo.rs:42" → 42).
    let line_number = extract_line_number(effective_target);

    // Prefer $EDITOR / $VISUAL when set and the target is a file (not a dir).
    if !path.is_dir() {
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_default();
        if !editor.is_empty()
            && let Some(cmd) = build_editor_command(&editor, &path, line_number)
        {
            if let Err(err) = std::process::Command::new("sh").arg("-c").arg(&cmd).spawn() {
                tracing::warn!(cmd = %cmd, error = %err, "failed to open file with $EDITOR");
            }
            return;
        }
    }

    #[cfg(target_os = "macos")]
    if let Err(err) = std::process::Command::new("open").arg(&path).spawn() {
        tracing::warn!(path = %path.display(), error = %err, "failed to open link target");
    }
    #[cfg(not(target_os = "macos"))]
    if let Err(err) = std::process::Command::new("xdg-open").arg(&path).spawn() {
        tracing::warn!(path = %path.display(), error = %err, "failed to open link target");
    }
}

/// Extract the first `:N` line number from a path like `src/main.rs:42` or `src/main.rs:42:5`.
fn extract_line_number(path: &str) -> Option<u32> {
    // Walk backwards from the end, skip optional `:col`, then read `:line`.
    let bytes = path.as_bytes();
    let mut end = bytes.len();
    for _ in 0..2 {
        let mut i = end;
        while i > 0 && bytes[i - 1].is_ascii_digit() {
            i -= 1;
        }
        if i < end && i > 0 && bytes[i - 1] == b':' {
            let num_str = &path[i..end];
            if let Ok(n) = num_str.parse::<u32>()
                && n > 0
            {
                return Some(n);
            }
            end = i - 1;
        } else {
            break;
        }
    }
    None
}

/// Build a shell command that opens `path` at `line` using the given editor binary.
/// Returns `None` when the editor name is not recognised (caller falls back to OS default).
fn build_editor_command(editor: &str, path: &std::path::Path, line: Option<u32>) -> Option<String> {
    let path_s = path.to_string_lossy();
    // Extract just the binary name for matching (editor may be a full path).
    let bin = std::path::Path::new(editor)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| editor.to_owned());
    let bin_lc = bin.to_lowercase();

    let line_arg = line.map(|n| n.to_string());

    let cmd = match bin_lc.as_str() {
        // vim / neovim: `vim +42 /path`
        "vim" | "nvim" | "vi" | "gvim" | "mvim" => {
            if let Some(ref l) = line_arg {
                format!("{editor} +{l} {path_s:?}")
            } else {
                format!("{editor} {path_s:?}")
            }
        }
        // emacs: `emacs +42 /path`
        "emacs" | "emacsclient" => {
            if let Some(ref l) = line_arg {
                format!("{editor} +{l} {path_s:?}")
            } else {
                format!("{editor} {path_s:?}")
            }
        }
        // helix: `hx /path:42`
        "hx" | "helix" => {
            if let Some(ref l) = line_arg {
                format!("{editor} {path_s:?}:{l}")
            } else {
                format!("{editor} {path_s:?}")
            }
        }
        // nano / micro: `nano +42 /path`
        "nano" | "micro" => {
            if let Some(ref l) = line_arg {
                format!("{editor} +{l} {path_s:?}")
            } else {
                format!("{editor} {path_s:?}")
            }
        }
        // kate / kwrite: `kate -l 42 /path`
        "kate" | "kwrite" => {
            if let Some(ref l) = line_arg {
                format!("{editor} -l {l} {path_s:?}")
            } else {
                format!("{editor} {path_s:?}")
            }
        }
        // Unknown terminal editor: open the file without line number.
        _ => format!("{editor} {path_s:?}"),
    };
    Some(cmd)
}

/// Show a modal alert dialog (macOS) or print to stderr (other platforms).
fn show_alert(message: &str) {
    #[cfg(target_os = "macos")]
    {
        let escaped = message.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "display alert \"Teletipo\" message \"{}\" buttons {{\"OK\"}} default button \"OK\"",
            escaped
        );
        if let Err(err) = std::process::Command::new("osascript")
            .args(["-e", &script])
            .spawn()
        {
            tracing::warn!(error = %err, "failed to show alert dialog");
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        tracing::info!(message = %message, "notification");
    }
}
