use crate::GpuRuntimeState;
use crate::coords::{
    clamp_editor_scroll, current_line_prefix, cursor_at_line_end, editor_cursor_row_col,
    editor_row_col_to_offset, extract_selection, line_leading_spaces, replace_cursor_line,
};
use crate::search;
use crate::settings;
use render_wgpu::AppWindowEvent;
use winit::event::ElementState;
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // top-level keyboard dispatcher; flat match
pub(super) fn handle_event(state: &mut GpuRuntimeState, event: AppWindowEvent) {
    let AppWindowEvent::KeyboardInput(key_event) = &event else {
        if let AppWindowEvent::ImeCommit(text) = event
            && !text.is_empty()
            && text != "\r"
            && text != "\n"
        {
            if state.tab().app.is_alternate_screen() || state.tab().command_running {
                state.send_terminal_input(text.as_bytes());
            } else {
                state.tab_mut().app.insert_editor_input(text.as_str());
            }
        }
        return;
    };

    if key_event.state != ElementState::Pressed {
        return;
    }

    if state.settings.open {
        settings::handle_settings_key(state, key_event);
        return;
    }

    if state.tab().search.active {
        handle_search_key(state, key_event);
        return;
    }

    // When the shell is waiting on a foreground command (vim, less, htop,
    // sudo, a running script, ssh, …) route typing and navigation keys
    // straight to the PTY instead of the command editor.  Also covers the
    // legacy alternate-screen detection so apps that switch screens without
    // a separate process group keep working.
    let route_to_pty = state.tab().app.is_alternate_screen() || state.tab().command_running;
    if route_to_pty && !state.modifiers.super_down {
        let app_cursor = state.tab().app.application_cursor_keys();
        match &key_event.logical_key {
            // Keep settings shortcut available even in fullscreen mode.
            Key::Character(ch) if state.modifiers.ctrl_down && ch.as_str() == "," => {}
            Key::Named(NamedKey::Escape) => {
                state.send_terminal_input(b"\x1b");
                return;
            }
            Key::Named(NamedKey::Tab) => {
                state.send_terminal_input(b"\t");
                return;
            }
            Key::Named(NamedKey::Enter) => {
                #[cfg(windows)]
                {
                    state.send_terminal_input(b"\r\n");
                }
                #[cfg(not(windows))]
                {
                    state.send_terminal_input(b"\r");
                }
                return;
            }
            Key::Named(NamedKey::Backspace) => {
                state.send_terminal_input(b"\x7f");
                return;
            }
            Key::Named(NamedKey::Delete) => {
                state.send_terminal_input(b"\x1b[3~");
                return;
            }
            Key::Named(NamedKey::ArrowUp) => {
                state.send_terminal_input(if app_cursor { b"\x1bOA" } else { b"\x1b[A" });
                return;
            }
            Key::Named(NamedKey::ArrowDown) => {
                state.send_terminal_input(if app_cursor { b"\x1bOB" } else { b"\x1b[B" });
                return;
            }
            Key::Named(NamedKey::ArrowRight) => {
                state.send_terminal_input(if app_cursor { b"\x1bOC" } else { b"\x1b[C" });
                return;
            }
            Key::Named(NamedKey::ArrowLeft) => {
                state.send_terminal_input(if app_cursor { b"\x1bOD" } else { b"\x1b[D" });
                return;
            }
            Key::Named(NamedKey::Home) => {
                state.send_terminal_input(b"\x1b[H");
                return;
            }
            Key::Named(NamedKey::End) => {
                state.send_terminal_input(b"\x1b[F");
                return;
            }
            Key::Named(NamedKey::Space) => {
                state.send_terminal_input(b" ");
                return;
            }
            Key::Named(NamedKey::F1) => {
                state.send_terminal_input(b"\x1bOP");
                return;
            }
            Key::Named(NamedKey::F2) => {
                state.send_terminal_input(b"\x1bOQ");
                return;
            }
            Key::Named(NamedKey::F3) => {
                state.send_terminal_input(b"\x1bOR");
                return;
            }
            Key::Named(NamedKey::F4) => {
                state.send_terminal_input(b"\x1bOS");
                return;
            }
            Key::Named(NamedKey::F5) => {
                state.send_terminal_input(b"\x1b[15~");
                return;
            }
            Key::Named(NamedKey::F6) => {
                state.send_terminal_input(b"\x1b[17~");
                return;
            }
            Key::Named(NamedKey::F7) => {
                state.send_terminal_input(b"\x1b[18~");
                return;
            }
            Key::Named(NamedKey::F8) => {
                state.send_terminal_input(b"\x1b[19~");
                return;
            }
            Key::Named(NamedKey::F9) => {
                state.send_terminal_input(b"\x1b[20~");
                return;
            }
            Key::Named(NamedKey::F10) => {
                state.send_terminal_input(b"\x1b[21~");
                return;
            }
            Key::Named(NamedKey::F11) => {
                state.send_terminal_input(b"\x1b[23~");
                return;
            }
            Key::Named(NamedKey::F12) => {
                state.send_terminal_input(b"\x1b[24~");
                return;
            }
            Key::Character(ch) if state.modifiers.ctrl_down => {
                if let Some(c) = ch.as_str().chars().next() {
                    let lo = c.to_ascii_lowercase();
                    if lo.is_ascii_lowercase() {
                        state.send_terminal_input(&[lo as u8 - b'a' + 1]);
                    } else if c == '[' {
                        state.send_terminal_input(b"\x1b");
                    } else if c == '\\' {
                        state.send_terminal_input(b"\x1c");
                    } else if c == ']' {
                        state.send_terminal_input(b"\x1d");
                    }
                }
                return;
            }
            Key::Character(_) => {
                if let Some(text) = key_event.text.as_ref()
                    && text != "\n"
                    && text != "\r"
                    && text != "\r\n"
                {
                    state.send_terminal_input(text.as_bytes());
                }
                return;
            }
            _ => {}
        }
    }

    // Any key other than Tab/Shift+Tab ends the suggestion-cycling session so
    // that subsequent ghost-text lookups start fresh from the new editor text.
    // Exception: Up/Down and Esc are allowed to handle the dropdown themselves.
    let is_tab = matches!(&key_event.logical_key, Key::Named(NamedKey::Tab));
    let is_nav = matches!(
        &key_event.logical_key,
        Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown)
    );
    let is_esc = matches!(&key_event.logical_key, Key::Named(NamedKey::Escape));
    let is_enter = matches!(&key_event.logical_key, Key::Named(NamedKey::Enter));
    let cycling = state.tabs[state.active_tab].suggestion_index.is_some();
    if !(is_tab || (is_nav && cycling) || (is_esc && cycling) || (is_enter && cycling)) {
        let active = state.active_tab;
        state.tabs[active].suggestion_prefix = None;
        state.tabs[active].suggestion_index = None;
    }

    // Save terminal selection before (possibly) clearing it — Cmd+C needs the saved values.
    let saved_terminal_anchor = state.tab().selection_anchor;
    let saved_terminal_end = state.tab().selection_end;
    let saved_anchor_scroll = state.tab().selection_anchor_scroll;
    let saved_end_scroll = state.tab().selection_end_scroll;
    // Preserve selection while a modifier key is held or while the key being
    // pressed IS a modifier (KeyboardInput fires before ModifiersChanged, so
    // super_down/ctrl_down may not yet be set when the Cmd/Ctrl key arrives).
    let is_modifier_key = matches!(
        &key_event.logical_key,
        Key::Named(
            NamedKey::Super
                | NamedKey::Control
                | NamedKey::Shift
                | NamedKey::Alt
                | NamedKey::AltGraph
                | NamedKey::Meta
                | NamedKey::Hyper
                | NamedKey::CapsLock,
        )
    );
    if !state.modifiers.super_down && !state.modifiers.ctrl_down && !is_modifier_key {
        let active = state.active_tab;
        state.tabs[active].selection_anchor = None;
        state.tabs[active].selection_end = None;
        state.tabs[active].is_selecting = false;
    }

    match &key_event.logical_key {
        Key::Named(NamedKey::Escape) => {
            if cycling {
                // Dismiss the dropdown; the editor already holds the original prefix.
                state.tabs[state.active_tab].suggestion_prefix = None;
                state.tabs[state.active_tab].suggestion_index = None;
            } else {
                state.send_terminal_input(b"\x1b");
            }
        }
        Key::Named(NamedKey::Tab) => {
            // If the popup is already open and Tab (without Shift) is pressed,
            // confirm the highlighted entry: fill the editor and close the dropdown.
            if cycling && !state.modifiers.shift_down {
                let prefix = state.tabs[state.active_tab]
                    .suggestion_prefix
                    .clone()
                    .unwrap_or_default();
                let idx = state.tabs[state.active_tab].suggestion_index.unwrap_or(0);
                let matches = crate::suggestion_matches_frecency(
                    &state.tabs[state.active_tab].history,
                    &state.tabs[state.active_tab].history_entries,
                    &prefix,
                    &state.tabs[state.active_tab].cwd,
                );
                if let Some(full) = matches.get(idx).cloned() {
                    let editor_text = state.tab().app.editor_snapshot();
                    let cursor = state.tab().app.editor_cursor_offset();
                    let indent = line_leading_spaces(&editor_text, cursor).to_owned();
                    let full_indented = format!("{indent}{full}");
                    let (new_text, new_cursor) =
                        replace_cursor_line(&editor_text, cursor, &full_indented);
                    state.tab_mut().app.editor_clear();
                    state.tab_mut().app.insert_editor_input(&new_text);
                    state.tab_mut().app.set_editor_cursor(new_cursor, false);
                }
                state.tabs[state.active_tab].suggestion_prefix = None;
                state.tabs[state.active_tab].suggestion_index = None;
                return;
            }

            let editor_text = state.tab().app.editor_snapshot();
            let cursor = state.tab().app.editor_cursor_offset();
            // Only engage when the cursor sits at the end of its current line.
            if !cursor_at_line_end(&editor_text, cursor) {
                return;
            }
            // Reuse the saved cycling prefix (keeps the suggestion set stable
            // across multiple Tab presses) or fall back to the current line text.
            let line_prefix = current_line_prefix(&editor_text, cursor);
            let prefix = state.tabs[state.active_tab]
                .suggestion_prefix
                .clone()
                .unwrap_or_else(|| line_prefix.to_string());
            if prefix.is_empty() {
                return;
            }

            let matches = crate::suggestion_matches_frecency(
                &state.tabs[state.active_tab].history,
                &state.tabs[state.active_tab].history_entries,
                &prefix,
                &state.tabs[state.active_tab].cwd,
            );
            if matches.is_empty() {
                return;
            }

            let n = matches.len();
            let current_idx = state.tabs[state.active_tab].suggestion_index;
            let new_idx = if state.modifiers.shift_down {
                // Shift+Tab: cycle backward.
                match current_idx {
                    None | Some(0) => n - 1,
                    Some(i) => i - 1,
                }
            } else {
                // Tab: cycle forward.
                match current_idx {
                    None => 0,
                    Some(i) => (i + 1) % n,
                }
            };

            state.tabs[state.active_tab].suggestion_prefix = Some(prefix);
            state.tabs[state.active_tab].suggestion_index = Some(new_idx);
            // Editor text is unchanged; ghost text displays the selected match in gray.
        }

        Key::Named(NamedKey::PageUp) => {
            let max_scroll = state.tab().app.scrollback_len();
            let prev = state.tab().scroll_offset;
            state.tab_mut().scroll_offset = prev.saturating_add(5).min(max_scroll);
        }
        Key::Named(NamedKey::PageDown) => {
            let prev = state.tab().scroll_offset;
            state.tab_mut().scroll_offset = prev.saturating_sub(5);
        }

        Key::Character(ch) if is_copy_shortcut(state, ch.as_str()) => {
            if let Some((start, end)) = state.tab().app.editor_selection() {
                let editor_text = state.tab().app.editor_snapshot();
                let selected = editor_text[start..end].to_string();
                if !selected.is_empty() {
                    state.shell_services.clipboard_set(selected);
                }
            } else if let (Some(anchor), Some(sel_end)) =
                (saved_terminal_anchor, saved_terminal_end)
            {
                // Adjust stored rows to the current scroll offset so that
                // copy picks the correct text even after scrolling.
                let current_scroll = state.tab().scroll_offset as i64;
                let anchor_scroll = saved_anchor_scroll as i64;
                let end_scroll = saved_end_scroll as i64;
                let ar = (anchor.0 as i64 + current_scroll - anchor_scroll).max(0) as usize;
                let er = (sel_end.0 as i64 + current_scroll - end_scroll).max(0) as usize;
                let adjusted_anchor = (ar, anchor.1);
                let adjusted_end = (er, sel_end.1);
                let last_text = state.tab().last_terminal_text.clone();
                let text = extract_selection(&last_text, adjusted_anchor, adjusted_end);
                if !text.is_empty() {
                    state.shell_services.clipboard_set(text);
                }
            }
        }

        Key::Character(ch) if is_paste_shortcut(state, ch.as_str()) => {
            if let Some(text) = state.shell_services.clipboard_get() {
                let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
                if !normalized.is_empty() {
                    if state.tab().app.is_alternate_screen() || state.tab().command_running {
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

        Key::Character(ch) if state.modifiers.super_down => match ch.as_str() {
            "," => {
                state.settings.open = true;
                state.settings.cursor = 0;
                state.settings.edit_buf = None;
                return;
            }
            "a" => {
                let end = state.tab().app.editor_snapshot().len();
                state.tab_mut().app.set_editor_cursor(0, false);
                state.tab_mut().app.set_editor_cursor(end, true);
            }
            "f" => {
                let tab = state.tab_mut();
                tab.search.active = true;
                if tab.search.query.is_empty() {
                    if let Some(q) = state.overlays.last_search_query.clone() {
                        state.tab_mut().search.query = q;
                    }
                }
                search::refresh_search(state.tab_mut());
            }
            "t" => state.add_new_tab(),
            "w" => {
                let idx = state.active_tab;
                state.close_tab(idx);
            }
            "[" => {
                state.active_tab = state.active_tab.saturating_sub(1);
            }
            "]" => {
                let last = state.tabs.len() - 1;
                state.active_tab = (state.active_tab + 1).min(last);
            }
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                if let Some(d) = ch.as_str().chars().next().and_then(|c| c.to_digit(10)) {
                    let idx = (d as usize - 1).min(state.tabs.len() - 1);
                    state.active_tab = idx;
                }
            }
            _ => {}
        },

        Key::Character(ch) if state.modifiers.ctrl_down && ch.as_str() == "," => {
            state.settings.open = true;
            state.settings.cursor = 0;
            state.settings.edit_buf = None;
        }

        Key::Character(ch) if state.modifiers.ctrl_down && ch.as_str() == "f" => {
            let tab = state.tab_mut();
            tab.search.active = true;
            if tab.search.query.is_empty() {
                if let Some(q) = state.overlays.last_search_query.clone() {
                    state.tab_mut().search.query = q;
                }
            }
            search::refresh_search(state.tab_mut());
        }

        Key::Character(ch) if state.modifiers.ctrl_down => {
            if let Some(c) = ch.as_str().chars().next() {
                let lo = c.to_ascii_lowercase();
                if lo.is_ascii_lowercase() {
                    state.send_terminal_input(&[lo as u8 - b'a' + 1]);
                } else if c == '[' {
                    state.send_terminal_input(b"\x1b");
                } else if c == '\\' {
                    state.send_terminal_input(b"\x1c");
                } else if c == ']' {
                    state.send_terminal_input(b"\x1d");
                }
            }
        }

        Key::Named(NamedKey::Enter) => {
            if cycling && !state.modifiers.shift_down {
                // Confirm: fill the editor with the selected match before submitting.
                let prefix = state.tabs[state.active_tab]
                    .suggestion_prefix
                    .clone()
                    .unwrap_or_default();
                let idx = state.tabs[state.active_tab].suggestion_index.unwrap_or(0);
                let matches = crate::suggestion_matches_frecency(
                    &state.tabs[state.active_tab].history,
                    &state.tabs[state.active_tab].history_entries,
                    &prefix,
                    &state.tabs[state.active_tab].cwd,
                );
                if let Some(full) = matches.get(idx).cloned() {
                    let editor_text = state.tab().app.editor_snapshot();
                    let cursor = state.tab().app.editor_cursor_offset();
                    let indent = line_leading_spaces(&editor_text, cursor).to_owned();
                    let full_indented = format!("{indent}{full}");
                    let (new_text, new_cursor) =
                        replace_cursor_line(&editor_text, cursor, &full_indented);
                    state.tab_mut().app.editor_clear();
                    state.tab_mut().app.insert_editor_input(&new_text);
                    state.tab_mut().app.set_editor_cursor(new_cursor, false);
                }
                state.tabs[state.active_tab].suggestion_prefix = None;
                state.tabs[state.active_tab].suggestion_index = None;
            }
            if state.modifiers.shift_down {
                state.tab_mut().app.insert_editor_input("\n");
            } else {
                state.tab_mut().scroll_offset = 0;
                state.run_editor_command();
            }
        }
        Key::Named(NamedKey::Backspace) => {
            state.tab_mut().app.editor_backspace();
            if cycling {
                let active = state.active_tab;
                let editor_text = state.tab().app.editor_snapshot();
                let cursor = state.tab().app.editor_cursor_offset();
                let new_prefix = current_line_prefix(&editor_text, cursor).to_string();
                let matches = crate::suggestion_matches_frecency(
                    &state.tabs[active].history,
                    &state.tabs[active].history_entries,
                    &new_prefix,
                    &state.tabs[active].cwd,
                );
                if !matches.is_empty() && !new_prefix.is_empty() {
                    state.tabs[active].suggestion_prefix = Some(new_prefix);
                    state.tabs[active].suggestion_index = Some(0);
                } else {
                    state.tabs[active].suggestion_prefix = None;
                    state.tabs[active].suggestion_index = None;
                }
            }
        }
        Key::Named(NamedKey::Delete) => {
            state.tab_mut().app.editor_delete_forward();
        }
        Key::Named(NamedKey::ArrowLeft) => {
            let extend = state.modifiers.shift_down;
            state.tab_mut().app.editor_move_cursor_left(extend);
        }
        Key::Named(NamedKey::ArrowRight) => {
            let extend = state.modifiers.shift_down;
            state.tab_mut().app.editor_move_cursor_right(extend);
        }
        Key::Named(NamedKey::ArrowUp) => {
            if cycling {
                // Navigate dropdown: move to the previous item (Shift+Tab direction).
                let prefix = state.tabs[state.active_tab]
                    .suggestion_prefix
                    .clone()
                    .unwrap_or_default();
                let matches = crate::suggestion_matches_frecency(
                    &state.tabs[state.active_tab].history,
                    &state.tabs[state.active_tab].history_entries,
                    &prefix,
                    &state.tabs[state.active_tab].cwd,
                );
                let n = matches.len();
                if n > 0 {
                    let idx = state.tabs[state.active_tab].suggestion_index.unwrap_or(0);
                    let new_idx = if idx == 0 { n - 1 } else { idx - 1 };
                    state.tabs[state.active_tab].suggestion_index = Some(new_idx);
                }
            } else {
                let text = state.tab().app.editor_snapshot();
                let offset = state.tab().app.editor_cursor_offset();
                let (row, col) = editor_cursor_row_col(&text, offset);
                if row == 0 && !state.modifiers.shift_down {
                    state.history_prev();
                } else if row > 0 {
                    let new_offset = editor_row_col_to_offset(&text, row - 1, col);
                    let extend = state.modifiers.shift_down;
                    state.tab_mut().app.set_editor_cursor(new_offset, extend);
                }
            }
        }
        Key::Named(NamedKey::ArrowDown) => {
            if cycling {
                // Navigate dropdown: move to the next item.
                let prefix = state.tabs[state.active_tab]
                    .suggestion_prefix
                    .clone()
                    .unwrap_or_default();
                let matches = crate::suggestion_matches_frecency(
                    &state.tabs[state.active_tab].history,
                    &state.tabs[state.active_tab].history_entries,
                    &prefix,
                    &state.tabs[state.active_tab].cwd,
                );
                let n = matches.len();
                if n > 0 {
                    let idx = state.tabs[state.active_tab].suggestion_index.unwrap_or(0);
                    let new_idx = (idx + 1) % n;
                    state.tabs[state.active_tab].suggestion_index = Some(new_idx);
                }
            } else {
                let text = state.tab().app.editor_snapshot();
                let offset = state.tab().app.editor_cursor_offset();
                let (row, col) = editor_cursor_row_col(&text, offset);
                let last_row = text.lines().count().saturating_sub(1);
                if row >= last_row && !state.modifiers.shift_down {
                    state.history_next();
                } else if row < last_row {
                    let new_offset = editor_row_col_to_offset(&text, row + 1, col);
                    let extend = state.modifiers.shift_down;
                    state.tab_mut().app.set_editor_cursor(new_offset, extend);
                }
            }
        }
        Key::Named(NamedKey::Home) => {
            if state.modifiers.super_down {
                // Cmd+Home: scroll to the very beginning of scrollback.
                let tab = state.tab_mut();
                tab.scroll_offset = tab.app.scrollback_len();
            } else {
                let extend = state.modifiers.shift_down;
                state.tab_mut().app.set_editor_cursor(0, extend);
            }
        }
        Key::Named(NamedKey::End) => {
            if state.modifiers.super_down {
                // Cmd+End: scroll back to the live view.
                state.tab_mut().scroll_offset = 0;
            } else {
                let extend = state.modifiers.shift_down;
                let end = state.tab().app.editor_snapshot().len();
                state.tab_mut().app.set_editor_cursor(end, extend);
            }
        }
        Key::Named(NamedKey::Space) => {
            state.tab_mut().app.insert_editor_input(" ");
        }
        Key::Character(_) => {
            if let Some(text) = key_event.text.as_ref()
                && text != "\n"
                && text != "\r"
                && text != "\r\n"
            {
                state.tab_mut().app.insert_editor_input(text.as_str());
                if cycling {
                    let active = state.active_tab;
                    let editor_text = state.tab().app.editor_snapshot();
                    let cursor = state.tab().app.editor_cursor_offset();
                    let new_prefix = current_line_prefix(&editor_text, cursor).to_string();
                    let matches = crate::suggestion_matches_frecency(
                        &state.tabs[active].history,
                        &state.tabs[active].history_entries,
                        &new_prefix,
                        &state.tabs[active].cwd,
                    );
                    if !matches.is_empty() {
                        state.tabs[active].suggestion_prefix = Some(new_prefix);
                        state.tabs[active].suggestion_index = Some(0);
                    } else {
                        state.tabs[active].suggestion_prefix = None;
                        state.tabs[active].suggestion_index = None;
                    }
                }
            }
        }
        _ => {}
    }
    clamp_editor_scroll(state);
}

fn handle_search_key(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) {
    let shift = state.modifiers.shift_down;
    let alt = state.modifiers.alt_down;
    let super_ = state.modifiers.super_down;

    match &key_event.logical_key {
        // ── Close ────────────────────────────────────────────────────────────
        Key::Named(NamedKey::Escape) => {
            let query = state.tab().search.query.clone();
            if !query.is_empty() {
                state.overlays.last_search_query = Some(query);
            }
            search::close_search(state.tab_mut());
        }

        // ── Navigate matches ─────────────────────────────────────────────────
        Key::Named(NamedKey::Enter) => {
            if shift {
                search::prev_match(state.tab_mut());
            } else {
                search::next_match(state.tab_mut());
            }
        }

        // ── Cursor movement ──────────────────────────────────────────────────
        Key::Named(NamedKey::ArrowLeft) => {
            if super_ {
                search::search_move_home(state.tab_mut(), shift);
            } else if alt {
                search::search_move_word_left(state.tab_mut(), shift);
            } else {
                search::search_move_left(state.tab_mut(), shift);
            }
        }
        Key::Named(NamedKey::ArrowRight) => {
            if super_ {
                search::search_move_end(state.tab_mut(), shift);
            } else if alt {
                search::search_move_word_right(state.tab_mut(), shift);
            } else {
                search::search_move_right(state.tab_mut(), shift);
            }
        }
        Key::Named(NamedKey::ArrowUp) => {
            // Navigate to previous match (Up arrow outside the input).
            search::prev_match(state.tab_mut());
        }
        Key::Named(NamedKey::ArrowDown) => {
            search::next_match(state.tab_mut());
        }
        Key::Named(NamedKey::Home) => {
            search::search_move_home(state.tab_mut(), shift);
        }
        Key::Named(NamedKey::End) => {
            search::search_move_end(state.tab_mut(), shift);
        }

        // ── Deletion ──────────────────────────────────────────────────────────
        Key::Named(NamedKey::Backspace) => {
            if alt {
                search::search_delete_word_backward(state.tab_mut());
            } else {
                search::search_delete_backward(state.tab_mut());
            }
        }
        Key::Named(NamedKey::Delete) => {
            search::search_delete_forward(state.tab_mut());
        }

        // ── Character input / shortcuts ───────────────────────────────────────
        Key::Character(ch) => {
            if super_ {
                match ch.as_str() {
                    "a" | "A" => {
                        search::search_select_all(state.tab_mut());
                        return;
                    }
                    "c" | "C" => {
                        let text = search::search_selected_text(state.tab()).to_owned();
                        if !text.is_empty() {
                            state.shell_services.clipboard_set(text);
                        }
                        return;
                    }
                    "x" | "X" => {
                        let text = search::search_selected_text(state.tab()).to_owned();
                        if !text.is_empty() {
                            state.shell_services.clipboard_set(text);
                            search::search_delete_backward(state.tab_mut());
                        }
                        return;
                    }
                    "v" | "V" => {
                        if let Some(text) = state.shell_services.clipboard_get() {
                            let filtered: String =
                                text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                            if !filtered.is_empty() {
                                search::search_insert(state.tab_mut(), &filtered);
                            }
                        }
                        return;
                    }
                    _ => {}
                }
            }
            // Alt+R — toggle regex mode; Alt+C — toggle case-sensitive.
            // Use physical_key so the check is layout-independent (on macOS, Alt+R
            // produces '®' as logical_key, not 'r').
            if alt {
                match key_event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyR) => {
                        let tab = state.tab_mut();
                        tab.search.regex_mode = !tab.search.regex_mode;
                        search::refresh_search(tab);
                        return;
                    }
                    PhysicalKey::Code(KeyCode::KeyC) => {
                        let tab = state.tab_mut();
                        tab.search.case_sensitive = !tab.search.case_sensitive;
                        search::refresh_search(tab);
                        return;
                    }
                    _ => {}
                }
                // Don't insert alt-generated symbols (e.g. '®', 'ç') into the query.
                return;
            }
            if let Some(text) = key_event.text.as_ref() {
                search::search_insert(state.tab_mut(), text.as_str());
            }
        }
        Key::Named(NamedKey::Space) => {
            if let Some(text) = key_event.text.as_ref() {
                search::search_insert(state.tab_mut(), text.as_str());
            }
        }
        _ => {}
    }
}

fn is_copy_shortcut(state: &GpuRuntimeState, key: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        state.modifiers.super_down && key.eq_ignore_ascii_case("c")
    }
    #[cfg(not(target_os = "macos"))]
    {
        state.modifiers.ctrl_down && state.modifiers.shift_down && key.eq_ignore_ascii_case("c")
    }
}

fn is_paste_shortcut(state: &GpuRuntimeState, key: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        state.modifiers.super_down && key.eq_ignore_ascii_case("v")
    }
    #[cfg(not(target_os = "macos"))]
    {
        state.modifiers.ctrl_down && state.modifiers.shift_down && key.eq_ignore_ascii_case("v")
    }
}
