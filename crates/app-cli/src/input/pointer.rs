use crate::coords::{clamp_editor_scroll, cursor_to_terminal_cell, editor_row_col_to_offset};
use crate::launch::execute_context_menu_item;
use crate::GpuRuntimeState;
use render_wgpu::{AppWindowEvent, SCROLLBAR_W_PX};
use std::time::Instant;
use winit::event::{ElementState, MouseButton};

pub(super) fn handle_event(state: &mut GpuRuntimeState, event: &AppWindowEvent) -> bool {
    if let AppWindowEvent::Resized { width, height, scale_factor, cell_w, cell_h } = event {
        state.window_width = *width;
        state.window_height = *height;
        state.scale_factor = *scale_factor;
        state.cell_w = *cell_w;
        state.cell_h = *cell_h;
        state.resize_all_tabs();
        let tab_bar_h = state.tab_bar_h();
        let available_h = *height as f32 - tab_bar_h;
        let pad_h = state.user_config.padding.horizontal as f32;
        let pad_v = state.user_config.padding.vertical as f32;
        let cols = ((*width as f32 - 2.0 * pad_h) / cell_w).max(1.0) as u16;
        let active = state.active_tab;
        let term_h = (available_h * state.tabs[active].split_ratio - 2.0 * pad_v).max(*cell_h);
        let rows = (term_h / cell_h).max(1.0) as u16;
        state.last_resize = Some((Instant::now(), cols, rows));
        return true;
    }

    if let AppWindowEvent::CursorMoved { x, y } = event {
        state.cursor_x = *x;
        state.cursor_y = *y;
        let tab_bar_h = state.tab_bar_h() as f64;
        let split_ratio = state.tab().split_ratio;

        if let Some((_, mx, my)) = state.tab_context_menu {
            let menu_w      = state.cell_w as f64 * 13.0;
            let menu_item_h = state.cell_h as f64 * 1.15;
            let in_menu = *x >= mx && *x <= mx + menu_w && *y >= my;
            state.tab_context_hover = if in_menu {
                let item = ((*y - my) / menu_item_h) as usize;
                if item < 4 { Some(item) } else { None }
            } else {
                None
            };
        }

        if state.dragging_separator {
            let available_h = state.window_height as f64 - tab_bar_h;
            let new_ratio = (*y - tab_bar_h) / available_h;
            state.tab_mut().split_ratio = (new_ratio as f32).clamp(0.2, 0.85);
            let active = state.active_tab;
            let sr = state.tabs[active].split_ratio;
            let pad_h = state.user_config.padding.horizontal as f32;
            let pad_v = state.user_config.padding.vertical as f32;
            let cols = ((state.window_width as f32 - 2.0 * pad_h) / state.cell_w).max(1.0) as u16;
            let term_h = (available_h as f32 * sr - 2.0 * pad_v).max(state.cell_h);
            let rows = (term_h / state.cell_h).max(1.0) as u16;
            state.resize_tab(active, rows, cols);
        } else if state.dragging_terminal_scrollbar {
            let available_h = state.window_height as f64 - tab_bar_h;
            let term_bottom = tab_bar_h + available_h * state.tab().split_ratio as f64;
            if term_bottom > tab_bar_h {
                let frac = ((*y - tab_bar_h) / (term_bottom - tab_bar_h)).clamp(0.0, 1.0);
                let max_scroll = state.tab().app.scrollback_len();
                state.tab_mut().scroll_offset = ((1.0 - frac) * max_scroll as f64) as usize;
            }
        } else if state.dragging_editor_scrollbar {
            let available_h = state.window_height as f64 - tab_bar_h;
            let term_bottom = tab_bar_h + available_h * state.tab().split_ratio as f64;
            let edit_h_px = state.window_height as f64 - term_bottom;
            if edit_h_px > 0.0 {
                let frac = ((*y - term_bottom) / edit_h_px).clamp(0.0, 1.0);
                let editor_text = state.tab().app.editor_snapshot();
                let total_lines = editor_text.lines().count().max(1);
                let visible_rows = if state.cell_h > 0.0 {
                    (edit_h_px as f32 / state.cell_h).floor().max(1.0) as usize
                } else {
                    1
                };
                let max_scroll = total_lines.saturating_sub(visible_rows);
                state.tab_mut().editor_scroll_offset =
                    (frac * max_scroll as f64).round() as usize;
            }
        } else if state.tab().is_selecting {
            let term_row_count = state.tab().term_row_count;
            if let Some(cell) = cursor_to_terminal_cell(
                *x, *y,
                state.window_width, state.window_height,
                split_ratio, state.cell_w, state.cell_h,
                term_row_count, tab_bar_h as f32,
            ) {
                state.tab_mut().selection_end = Some(cell);
            }
        } else if state.tab().is_selecting_editor {
            let available_h = state.window_height as f64 - tab_bar_h;
            let edit_top_px = tab_bar_h + split_ratio as f64 * available_h + 2.0;
            let editor_scroll_offset = state.tab().editor_scroll_offset;
            let row = ((*y - edit_top_px) / state.cell_h as f64)
                .max(0.0).floor() as usize + editor_scroll_offset;
            let prefix_cols = if row == 0 { 2.0_f64 } else { 0.0 };
            let col = (*x / state.cell_w as f64 - prefix_cols)
                .max(0.0).floor() as usize;
            let text = state.tab().app.editor_snapshot();
            let offset = editor_row_col_to_offset(&text, row, col);
            state.tab_mut().app.set_editor_cursor(offset, true);
            clamp_editor_scroll(state);
        }
        return true;
    }

    if let AppWindowEvent::MouseInput {
        state: btn_state,
        button: MouseButton::Left,
    } = event
    {
        if *btn_state == ElementState::Released {
            if let Some(drag_from) = state.tab_drag {
                if (state.cursor_x - state.tab_drag_start_x).abs() > 5.0 {
                    let n = state.tabs.len();
                    let add_btn_w = state.cell_w as f64 * 2.0;
                    let tab_area_w = (state.window_width as f64 - add_btn_w).max(1.0);
                    let frac = (state.cursor_x / tab_area_w).clamp(0.0, 1.0);
                    let insert_before = (frac * n as f64).round() as usize;
                    state.move_tab_to(drag_from, insert_before);
                }
                state.tab_drag = None;
            }
            state.dragging_separator = false;
            state.dragging_terminal_scrollbar = false;
            state.dragging_editor_scrollbar = false;
            state.tab_mut().is_selecting = false;
            state.tab_mut().is_selecting_editor = false;
        }
        if *btn_state == ElementState::Pressed {
            if state.tab_context_menu.is_some() {
                if let (Some(item), Some((tab_idx, _, _))) =
                    (state.tab_context_hover, state.tab_context_menu)
                {
                    execute_context_menu_item(state, tab_idx, item);
                }
                state.tab_context_menu = None;
                state.tab_context_hover = None;
                return true;
            }

            let tab_bar_h = state.tab_bar_h() as f64;
            if state.cursor_y < tab_bar_h {
                let n = state.tabs.len();
                let add_btn_w = state.cell_w as f64 * 2.0;
                let tab_area_w = state.window_width as f64 - add_btn_w;

                if state.cursor_x >= state.window_width as f64 - add_btn_w {
                    state.add_new_tab();
                    return true;
                }

                let tab_w   = tab_area_w / n as f64;
                let tab_idx = (state.cursor_x / tab_w).min(n as f64 - 1.0) as usize;
                let close_w   = state.cell_w as f64 * 1.5;
                let tab_right = (tab_idx + 1) as f64 * tab_w;
                if state.cursor_x >= tab_right - close_w {
                    state.close_tab(tab_idx);
                } else {
                    state.active_tab = tab_idx;
                    state.tab_drag = Some(tab_idx);
                    state.tab_drag_start_x = state.cursor_x;
                }
                return true;
            }

            let split_ratio = state.tab().split_ratio;
            let available_h = state.window_height as f64 - tab_bar_h;
            let sep_y_px = tab_bar_h + available_h * split_ratio as f64;

            if (state.cursor_y - sep_y_px).abs() < 6.0 {
                state.dragging_separator = true;
                return true;
            }

            let sb_left = state.window_width as f64 - SCROLLBAR_W_PX as f64;
            let term_bottom = sep_y_px;

            if state.cursor_x >= sb_left
                && state.cursor_y >= tab_bar_h
                && state.cursor_y <= term_bottom
            {
                let frac = (state.cursor_y - tab_bar_h) / (term_bottom - tab_bar_h);
                let max_scroll = state.tab().app.scrollback_len();
                state.tab_mut().scroll_offset = ((1.0 - frac) * max_scroll as f64) as usize;
                state.dragging_terminal_scrollbar = true;
                return true;
            }

            if state.cursor_x >= sb_left && state.cursor_y > term_bottom {
                let edit_h_px = state.window_height as f64 - term_bottom;
                if edit_h_px > 0.0 {
                    let frac = (state.cursor_y - term_bottom) / edit_h_px;
                    let editor_text = state.tab().app.editor_snapshot();
                    let total_lines = editor_text.lines().count().max(1);
                    let visible_rows = if state.cell_h > 0.0 {
                        (edit_h_px as f32 / state.cell_h).floor().max(1.0) as usize
                    } else {
                        1
                    };
                    let max_scroll = total_lines.saturating_sub(visible_rows);
                    state.tab_mut().editor_scroll_offset =
                        (frac * max_scroll as f64).round() as usize;
                    state.dragging_editor_scrollbar = true;
                }
                return true;
            }

            let term_row_count = state.tab().term_row_count;
            if let Some(cell) = cursor_to_terminal_cell(
                state.cursor_x, state.cursor_y,
                state.window_width, state.window_height,
                split_ratio, state.cell_w, state.cell_h,
                term_row_count, tab_bar_h as f32,
            ) {
                state.tab_mut().selection_anchor = Some(cell);
                state.tab_mut().selection_end = Some(cell);
                state.tab_mut().is_selecting = true;
            } else if state.cursor_y > term_bottom {
                let edit_top_px = term_bottom + 2.0;
                let editor_scroll_offset = state.tab().editor_scroll_offset;
                let row = ((state.cursor_y - edit_top_px) / state.cell_h as f64)
                    .max(0.0).floor() as usize + editor_scroll_offset;
                let prefix_cols = if row == 0 { 2.0_f64 } else { 0.0 };
                let col = (state.cursor_x / state.cell_w as f64 - prefix_cols)
                    .max(0.0).floor() as usize;
                let text = state.tab().app.editor_snapshot();
                let offset = editor_row_col_to_offset(&text, row, col);
                let extend = state.shift_down;
                state.tab_mut().app.set_editor_cursor(offset, extend);
                state.tab_mut().is_selecting_editor = true;
                clamp_editor_scroll(state);
            }
        }
        return true;
    }

    if let AppWindowEvent::MouseInput {
        state: btn_state,
        button: MouseButton::Right,
    } = event
    {
        if *btn_state == ElementState::Pressed {
            state.tab_context_menu = None;
            state.tab_context_hover = None;

            let tab_bar_h = state.tab_bar_h() as f64;
            if state.cursor_y < tab_bar_h {
                let n = state.tabs.len();
                let add_btn_w = state.cell_w as f64 * 2.0;
                let tab_area_w = state.window_width as f64 - add_btn_w;
                if n > 0 && state.cursor_x < state.window_width as f64 - add_btn_w {
                    let tab_w = tab_area_w / n as f64;
                    let tab_idx = (state.cursor_x / tab_w).min(n as f64 - 1.0) as usize;
                    state.tab_context_menu = Some((tab_idx, state.cursor_x, tab_bar_h));
                }
            }
        }
        return true;
    }

    if let AppWindowEvent::MouseWheel { delta_lines } = event {
        if state.tab_context_menu.is_some() {
            state.tab_context_menu = None;
            state.tab_context_hover = None;
        }
        let lines = delta_lines.round().abs().max(1.0) as usize;
        let tab_bar_h = state.tab_bar_h() as f64;
        let split_ratio = state.tab().split_ratio;
        let term_bottom = tab_bar_h + (state.window_height as f64 - tab_bar_h) * split_ratio as f64;

        if state.cursor_y > term_bottom {
            let editor_text = state.tab().app.editor_snapshot();
            let total_lines = editor_text.lines().count().max(1);
            let edit_h_px = state.window_height as f64 - term_bottom;
            let visible_rows = if state.cell_h > 0.0 {
                (edit_h_px as f32 / state.cell_h).floor().max(1.0) as usize
            } else {
                1
            };
            let max_scroll = total_lines.saturating_sub(visible_rows);
            let prev = state.tab().editor_scroll_offset;
            if *delta_lines > 0.0 {
                state.tab_mut().editor_scroll_offset = prev.saturating_sub(lines);
            } else {
                state.tab_mut().editor_scroll_offset =
                    prev.saturating_add(lines).min(max_scroll);
            }
        } else {
            let prev = state.tab().scroll_offset;
            if *delta_lines > 0.0 {
                let max_scroll = state.tab().app.scrollback_len();
                state.tab_mut().scroll_offset = prev.saturating_add(lines).min(max_scroll);
            } else {
                state.tab_mut().scroll_offset = prev.saturating_sub(lines);
            }
        }
        return true;
    }

    if let AppWindowEvent::ModifiersChanged(mods) = event {
        state.ctrl_down = mods.control_key();
        state.super_down = mods.super_key();
        state.shift_down = mods.shift_key();
        return true;
    }

    false
}
