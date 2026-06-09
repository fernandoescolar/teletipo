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

#[allow(clippy::cognitive_complexity)] // top-level keyboard dispatcher; flat match
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

    if handle_pre_dispatch(state, key_event) {
        return;
    }

    if try_route_to_pty(state, key_event) {
        return;
    }

    let cycling = reset_suggestion_cycle_if_needed(state, key_event);
    let saved_selection = capture_terminal_selection(state);
    clear_selection_if_needed(state, key_event);

    handle_post_dispatch_key(state, key_event, cycling, &saved_selection);
    clamp_editor_scroll(state);
}

#[derive(Clone, Copy)]
struct SavedSelection {
    anchor: Option<(usize, usize)>,
    end: Option<(usize, usize)>,
    anchor_scroll: usize,
    end_scroll: usize,
}

fn handle_pre_dispatch(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) -> bool {
    if state.settings.open {
        settings::handle_settings_key(state, key_event);
        return true;
    }
    if state.command_palette.is_some() {
        handle_palette_key(state, key_event);
        return true;
    }
    if state.tab().search.active {
        handle_search_key(state, key_event);
        return true;
    }
    if handle_windows_shortcuts(state, key_event) {
        return true;
    }

    // Windows often intercepts Win+Shift+P, so keep Ctrl+Shift+P as a
    // reliable cross-platform way to open the command palette.
    if let Key::Character(ch) = &key_event.logical_key
        && state.modifiers.ctrl_down
        && state.modifiers.shift_down
        && ch.as_str().eq_ignore_ascii_case("p")
    {
        open_command_palette(state);
        return true;
    }
    false
}

#[cfg(target_os = "windows")]
fn handle_windows_shortcuts(
    state: &mut GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
) -> bool {
    if let Key::Character(ch) = &key_event.logical_key
        && state.modifiers.ctrl_down
    {
        match ch.as_str() {
            "t" | "T" => {
                crate::commands::execute_ui_command(
                    state,
                    crate::commands::CommandId::NewTab,
                    crate::commands::CommandContext::default(),
                );
                return true;
            }
            "w" | "W" => {
                crate::commands::execute_ui_command(
                    state,
                    crate::commands::CommandId::CloseTab,
                    crate::commands::CommandContext::default(),
                );
                return true;
            }
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                if let Some(d) = ch.as_str().chars().next().and_then(|c| c.to_digit(10)) {
                    let idx = (d as usize - 1).min(state.tabs.len() - 1);
                    state.active_tab = idx;
                    state.push_accessibility_tree();
                }
                return true;
            }
            _ => {}
        }
    }

    if state.modifiers.ctrl_down {
        match &key_event.logical_key {
            Key::Named(NamedKey::PageUp) => {
                state.active_tab = state.active_tab.saturating_sub(1);
                state.push_accessibility_tree();
                return true;
            }
            Key::Named(NamedKey::PageDown) => {
                let last = state.tabs.len() - 1;
                state.active_tab = (state.active_tab + 1).min(last);
                state.push_accessibility_tree();
                return true;
            }
            _ => {}
        }
    }

    false
}

#[cfg(not(target_os = "windows"))]
fn handle_windows_shortcuts(
    _state: &mut GpuRuntimeState,
    _key_event: &winit::event::KeyEvent,
) -> bool {
    false
}

fn pty_named_sequence(named: &NamedKey, app_cursor: bool) -> Option<&'static [u8]> {
    match named {
        NamedKey::Escape => Some(b"\x1b"),
        NamedKey::Tab => Some(b"\t"),
        NamedKey::Enter => Some(b"\r"),
        NamedKey::Backspace => Some(b"\x7f"),
        NamedKey::Delete => Some(b"\x1b[3~"),
        NamedKey::ArrowUp => Some(if app_cursor { b"\x1bOA" } else { b"\x1b[A" }),
        NamedKey::ArrowDown => Some(if app_cursor { b"\x1bOB" } else { b"\x1b[B" }),
        NamedKey::ArrowRight => Some(if app_cursor { b"\x1bOC" } else { b"\x1b[C" }),
        NamedKey::ArrowLeft => Some(if app_cursor { b"\x1bOD" } else { b"\x1b[D" }),
        NamedKey::Home => Some(b"\x1b[H"),
        NamedKey::End => Some(b"\x1b[F"),
        NamedKey::Space => Some(b" "),
        NamedKey::F1 => Some(b"\x1bOP"),
        NamedKey::F2 => Some(b"\x1bOQ"),
        NamedKey::F3 => Some(b"\x1bOR"),
        NamedKey::F4 => Some(b"\x1bOS"),
        NamedKey::F5 => Some(b"\x1b[15~"),
        NamedKey::F6 => Some(b"\x1b[17~"),
        NamedKey::F7 => Some(b"\x1b[18~"),
        NamedKey::F8 => Some(b"\x1b[19~"),
        NamedKey::F9 => Some(b"\x1b[20~"),
        NamedKey::F10 => Some(b"\x1b[21~"),
        NamedKey::F11 => Some(b"\x1b[23~"),
        NamedKey::F12 => Some(b"\x1b[24~"),
        _ => None,
    }
}

fn send_ctrl_character_to_terminal(state: &mut GpuRuntimeState, ch: &str) -> bool {
    if let Some(c) = ch.chars().next() {
        let lo = c.to_ascii_lowercase();
        if lo.is_ascii_lowercase() {
            state.send_terminal_input(&[lo as u8 - b'a' + 1]);
            return true;
        }
        if c == '[' {
            state.send_terminal_input(b"\x1b");
            return true;
        }
        if c == '\\' {
            state.send_terminal_input(b"\x1c");
            return true;
        }
        if c == ']' {
            state.send_terminal_input(b"\x1d");
            return true;
        }
    }
    false
}

fn try_route_to_pty(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) -> bool {
    let route_to_pty = state.tab().app.is_alternate_screen() || state.tab().command_running;
    if !route_to_pty || state.modifiers.super_down {
        return false;
    }

    let app_cursor = state.tab().app.application_cursor_keys();
    match &key_event.logical_key {
        Key::Character(ch) if state.modifiers.ctrl_down && ch.as_str() == "," => false,
        Key::Named(named) => {
            if let Some(seq) = pty_named_sequence(named, app_cursor) {
                state.send_terminal_input(seq);
                true
            } else {
                false
            }
        }
        Key::Character(ch) if state.modifiers.ctrl_down => {
            send_ctrl_character_to_terminal(state, ch.as_str())
        }
        Key::Character(ch) => {
            let text = key_event.text.as_deref().unwrap_or(ch.as_str());
            if !text.is_empty() && text != "\n" && text != "\r" && text != "\r\n" {
                state.send_terminal_input(text.as_bytes());
            }
            true
        }
        _ => false,
    }
}

fn reset_suggestion_cycle_if_needed(
    state: &mut GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
) -> bool {
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
    cycling
}

fn capture_terminal_selection(state: &GpuRuntimeState) -> SavedSelection {
    SavedSelection {
        anchor: state.tab().selection_anchor,
        end: state.tab().selection_end,
        anchor_scroll: state.tab().selection_anchor_scroll,
        end_scroll: state.tab().selection_end_scroll,
    }
}

fn clear_selection_if_needed(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) {
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
}

fn update_cycling_after_editor_edit(state: &mut GpuRuntimeState, cycling: bool) {
    if !cycling {
        return;
    }
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

fn apply_selected_suggestion(state: &mut GpuRuntimeState) {
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
        let (new_text, new_cursor) = replace_cursor_line(&editor_text, cursor, &full_indented);
        state.tab_mut().app.editor_clear();
        state.tab_mut().app.insert_editor_input(&new_text);
        state.tab_mut().app.set_editor_cursor(new_cursor, false);
    }
    state.tabs[state.active_tab].suggestion_prefix = None;
    state.tabs[state.active_tab].suggestion_index = None;
}

fn handle_tab_key(state: &mut GpuRuntimeState, cycling: bool) {
    if cycling && !state.modifiers.shift_down {
        apply_selected_suggestion(state);
        return;
    }

    let editor_text = state.tab().app.editor_snapshot();
    let cursor = state.tab().app.editor_cursor_offset();
    if !cursor_at_line_end(&editor_text, cursor) {
        return;
    }
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
        match current_idx {
            None | Some(0) => n - 1,
            Some(i) => i - 1,
        }
    } else {
        match current_idx {
            None => 0,
            Some(i) => (i + 1) % n,
        }
    };

    state.tabs[state.active_tab].suggestion_prefix = Some(prefix);
    state.tabs[state.active_tab].suggestion_index = Some(new_idx);
}

fn handle_copy_shortcut(
    state: &mut GpuRuntimeState,
    ch: &str,
    saved_selection: &SavedSelection,
) -> bool {
    if !is_copy_shortcut(state, ch) {
        return false;
    }
    let mut copied_len: usize = 0;
    if let Some((start, end)) = state.tab().app.editor_selection() {
        let editor_text = state.tab().app.editor_snapshot();
        let selected = editor_text[start..end].to_string();
        if !selected.is_empty() {
            copied_len = selected.chars().count();
            state.shell_services.clipboard_set(selected);
        }
    } else if let (Some(anchor), Some(sel_end)) = (saved_selection.anchor, saved_selection.end) {
        let current_scroll = state.tab().scroll_offset as i64;
        let anchor_scroll = saved_selection.anchor_scroll as i64;
        let end_scroll = saved_selection.end_scroll as i64;
        let ar = (anchor.0 as i64 + current_scroll - anchor_scroll).max(0) as usize;
        let er = (sel_end.0 as i64 + current_scroll - end_scroll).max(0) as usize;
        let adjusted_anchor = (ar, anchor.1);
        let adjusted_end = (er, sel_end.1);
        let last_text = state.tab().last_terminal_text.clone();
        let text = extract_selection(&last_text, adjusted_anchor, adjusted_end);
        if !text.is_empty() {
            copied_len = text.chars().count();
            state.shell_services.clipboard_set(text);
        }
    }
    if copied_len > 0 {
        state.push_toast(
            format!("Copied {copied_len} chars"),
            crate::state::ToastKind::Success,
        );
    }
    true
}

fn handle_paste_shortcut(state: &mut GpuRuntimeState, ch: &str) -> bool {
    if !is_paste_shortcut(state, ch) {
        return false;
    }
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
    true
}

fn activate_search_overlay(state: &mut GpuRuntimeState) {
    let tab = state.tab_mut();
    tab.search.active = true;
    if tab.search.query.is_empty()
        && let Some(q) = state.overlays.last_search_query.clone()
    {
        state.tab_mut().search.query = q;
    }
    search::refresh_search(state.tab_mut());
}

fn handle_super_shortcut(state: &mut GpuRuntimeState, ch: &str) -> bool {
    if !state.modifiers.super_down {
        return false;
    }
    match ch.to_ascii_lowercase().as_str() {
        "," => {
            crate::commands::execute_ui_command(
                state,
                crate::commands::CommandId::OpenSettings,
                crate::commands::CommandContext::default(),
            );
        }
        "a" => {
            let end = state.tab().app.editor_snapshot().len();
            state.tab_mut().app.set_editor_cursor(0, false);
            state.tab_mut().app.set_editor_cursor(end, true);
        }
        "f" => activate_search_overlay(state),
        "t" => crate::commands::execute_ui_command(
            state,
            crate::commands::CommandId::NewTab,
            crate::commands::CommandContext::default(),
        ),
        "w" => {
            crate::commands::execute_ui_command(
                state,
                crate::commands::CommandId::CloseTab,
                crate::commands::CommandContext::default(),
            );
        }
        p if state.modifiers.shift_down && p.eq_ignore_ascii_case("p") => {
            open_command_palette(state);
        }
        "[" => {
            state.active_tab = state.active_tab.saturating_sub(1);
            state.push_accessibility_tree();
        }
        "]" => {
            let last = state.tabs.len() - 1;
            state.active_tab = (state.active_tab + 1).min(last);
            state.push_accessibility_tree();
        }
        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
            if let Some(d) = ch.chars().next().and_then(|c| c.to_digit(10)) {
                let idx = (d as usize - 1).min(state.tabs.len() - 1);
                state.active_tab = idx;
                state.push_accessibility_tree();
            }
        }
        _ => {}
    }
    true
}

fn handle_ctrl_shortcut(state: &mut GpuRuntimeState, ch: &str) -> bool {
    if !state.modifiers.ctrl_down {
        return false;
    }
    if ch.eq_ignore_ascii_case("a") {
        let end = state.tab().app.editor_snapshot().len();
        state.tab_mut().app.set_editor_cursor(0, false);
        state.tab_mut().app.set_editor_cursor(end, true);
        return true;
    }
    if ch == "," {
        crate::commands::execute_ui_command(
            state,
            crate::commands::CommandId::OpenSettings,
            crate::commands::CommandContext::default(),
        );
        return true;
    }
    if ch == "f" {
        activate_search_overlay(state);
        return true;
    }
    send_ctrl_character_to_terminal(state, ch);
    true
}

fn handle_character_key(
    state: &mut GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
    cycling: bool,
    saved_selection: &SavedSelection,
    ch: &str,
) {
    if handle_copy_shortcut(state, ch, saved_selection)
        || handle_paste_shortcut(state, ch)
        || handle_super_shortcut(state, ch)
        || handle_ctrl_shortcut(state, ch)
    {
        return;
    }

    let text = key_event.text.as_deref().unwrap_or(ch);
    if !text.is_empty() && text != "\n" && text != "\r" && text != "\r\n" {
        state.tab_mut().app.insert_editor_input(text);
        update_cycling_after_editor_edit(state, cycling);
    }
}

fn handle_named_key(state: &mut GpuRuntimeState, named: &NamedKey, cycling: bool) -> bool {
    if handle_named_key_overlay_and_scroll(state, named, cycling) {
        return true;
    }
    if handle_named_key_editor_ops(state, named, cycling) {
        return true;
    }
    handle_named_key_navigation(state, named, cycling)
}

fn handle_named_key_overlay_and_scroll(
    state: &mut GpuRuntimeState,
    named: &NamedKey,
    cycling: bool,
) -> bool {
    match named {
        NamedKey::Escape => {
            if cycling {
                state.tabs[state.active_tab].suggestion_prefix = None;
                state.tabs[state.active_tab].suggestion_index = None;
            } else {
                state.send_terminal_input(b"\x1b");
            }
            true
        }
        NamedKey::Tab => {
            handle_tab_key(state, cycling);
            true
        }
        NamedKey::PageUp => {
            let max_scroll = state.tab().app.scrollback_len();
            let prev = state.tab().scroll_offset;
            state.tab_mut().scroll_offset = prev.saturating_add(5).min(max_scroll);
            state.push_accessibility_tree();
            true
        }
        NamedKey::PageDown => {
            let prev = state.tab().scroll_offset;
            state.tab_mut().scroll_offset = prev.saturating_sub(5);
            state.push_accessibility_tree();
            true
        }
        _ => false,
    }
}

fn handle_named_key_editor_ops(
    state: &mut GpuRuntimeState,
    named: &NamedKey,
    cycling: bool,
) -> bool {
    match named {
        NamedKey::Enter => {
            if cycling && !state.modifiers.shift_down {
                apply_selected_suggestion(state);
            }
            if state.modifiers.shift_down {
                state.tab_mut().app.insert_editor_input("\n");
            } else {
                state.tab_mut().scroll_offset = 0;
                state.run_editor_command();
            }
            true
        }
        NamedKey::Backspace => {
            state.tab_mut().app.editor_backspace();
            update_cycling_after_editor_edit(state, cycling);
            true
        }
        NamedKey::Delete => {
            state.tab_mut().app.editor_delete_forward();
            true
        }
        NamedKey::ArrowLeft => {
            let extend = state.modifiers.shift_down;
            state.tab_mut().app.editor_move_cursor_left(extend);
            true
        }
        NamedKey::ArrowRight => {
            let extend = state.modifiers.shift_down;
            state.tab_mut().app.editor_move_cursor_right(extend);
            true
        }
        _ => false,
    }
}

fn handle_named_key_navigation(
    state: &mut GpuRuntimeState,
    named: &NamedKey,
    cycling: bool,
) -> bool {
    match named {
        NamedKey::ArrowUp => {
            if cycling {
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
            true
        }
        NamedKey::ArrowDown => {
            if cycling {
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
            true
        }
        NamedKey::Home => {
            if state.modifiers.super_down {
                let tab = state.tab_mut();
                tab.scroll_offset = tab.app.scrollback_len();
                state.push_accessibility_tree();
            } else {
                let extend = state.modifiers.shift_down;
                state.tab_mut().app.set_editor_cursor(0, extend);
            }
            true
        }
        NamedKey::End => {
            if state.modifiers.super_down {
                state.tab_mut().scroll_offset = 0;
                state.push_accessibility_tree();
            } else {
                let extend = state.modifiers.shift_down;
                let end = state.tab().app.editor_snapshot().len();
                state.tab_mut().app.set_editor_cursor(end, extend);
            }
            true
        }
        NamedKey::Space => {
            state.tab_mut().app.insert_editor_input(" ");
            true
        }
        _ => false,
    }
}

fn handle_post_dispatch_key(
    state: &mut GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
    cycling: bool,
    saved_selection: &SavedSelection,
) {
    if let Key::Named(named) = &key_event.logical_key
        && handle_named_key(state, named, cycling)
    {
        return;
    }

    if let Key::Character(ch) = &key_event.logical_key {
        handle_character_key(state, key_event, cycling, saved_selection, ch.as_str());
    }
}

fn handle_search_key(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) {
    let shift = state.modifiers.shift_down;
    let alt = state.modifiers.alt_down;
    let super_ = state.modifiers.super_down;

    match &key_event.logical_key {
        Key::Named(NamedKey::Escape) => close_search_overlay(state),
        Key::Named(NamedKey::Enter) => search_enter(state, shift),
        Key::Named(NamedKey::ArrowLeft) => search_move_left(state, super_, alt, shift),
        Key::Named(NamedKey::ArrowRight) => search_move_right(state, super_, alt, shift),
        Key::Named(NamedKey::ArrowUp) => search::prev_match(state.tab_mut()),
        Key::Named(NamedKey::ArrowDown) => search::next_match(state.tab_mut()),
        Key::Named(NamedKey::Home) => search::search_move_home(state.tab_mut(), shift),
        Key::Named(NamedKey::End) => search::search_move_end(state.tab_mut(), shift),
        Key::Named(NamedKey::Backspace) => search_backspace(state, alt),
        Key::Named(NamedKey::Delete) => search::search_delete_forward(state.tab_mut()),
        Key::Character(ch) => search_character_input(state, key_event, ch.as_str(), alt, super_),
        Key::Named(NamedKey::Space) => search_insert_key_text(state, key_event),
        _ => {}
    }
}

fn close_search_overlay(state: &mut GpuRuntimeState) {
    let query = state.tab().search.query.clone();
    if !query.is_empty() {
        state.overlays.last_search_query = Some(query);
    }
    search::close_search(state.tab_mut());
}

fn search_enter(state: &mut GpuRuntimeState, shift: bool) {
    if shift {
        search::prev_match(state.tab_mut());
    } else {
        search::next_match(state.tab_mut());
    }
}

fn search_move_left(state: &mut GpuRuntimeState, super_: bool, alt: bool, shift: bool) {
    if super_ {
        search::search_move_home(state.tab_mut(), shift);
    } else if alt {
        search::search_move_word_left(state.tab_mut(), shift);
    } else {
        search::search_move_left(state.tab_mut(), shift);
    }
}

fn search_move_right(state: &mut GpuRuntimeState, super_: bool, alt: bool, shift: bool) {
    if super_ {
        search::search_move_end(state.tab_mut(), shift);
    } else if alt {
        search::search_move_word_right(state.tab_mut(), shift);
    } else {
        search::search_move_right(state.tab_mut(), shift);
    }
}

fn search_backspace(state: &mut GpuRuntimeState, alt: bool) {
    if alt {
        search::search_delete_word_backward(state.tab_mut());
    } else {
        search::search_delete_backward(state.tab_mut());
    }
}

fn search_character_input(
    state: &mut GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
    ch: &str,
    alt: bool,
    super_: bool,
) {
    if super_ && handle_search_super_shortcuts(state, ch) {
        return;
    }

    // Use physical_key so checks are layout-independent (e.g. Alt+R on macOS).
    if alt {
        match key_event.physical_key {
            PhysicalKey::Code(KeyCode::KeyR) => {
                let tab = state.tab_mut();
                tab.search.regex_mode = !tab.search.regex_mode;
                search::refresh_search(tab);
            }
            PhysicalKey::Code(KeyCode::KeyC) => {
                let tab = state.tab_mut();
                tab.search.case_sensitive = !tab.search.case_sensitive;
                search::refresh_search(tab);
            }
            _ => {}
        }
        return;
    }

    search_insert_key_text(state, key_event);
}

fn handle_search_super_shortcuts(state: &mut GpuRuntimeState, ch: &str) -> bool {
    match ch {
        "a" | "A" => {
            search::search_select_all(state.tab_mut());
            true
        }
        "c" | "C" => {
            let text = search::search_selected_text(state.tab()).to_owned();
            if !text.is_empty() {
                state.shell_services.clipboard_set(text);
            }
            true
        }
        "x" | "X" => {
            let text = search::search_selected_text(state.tab()).to_owned();
            if !text.is_empty() {
                state.shell_services.clipboard_set(text);
                search::search_delete_backward(state.tab_mut());
            }
            true
        }
        "v" | "V" => {
            if let Some(text) = state.shell_services.clipboard_get() {
                let filtered: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                if !filtered.is_empty() {
                    search::search_insert(state.tab_mut(), &filtered);
                }
            }
            true
        }
        _ => false,
    }
}

fn search_insert_key_text(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) {
    if let Some(text) = key_event.text.as_ref() {
        search::search_insert(state.tab_mut(), text.as_str());
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

// ── Command palette ───────────────────────────────────────────────────────────

/// Build and open the command palette with all available items.
fn open_command_palette(state: &mut GpuRuntimeState) {
    use crate::state::{CommandPaletteState, PaletteAction, PaletteItem};

    let mut items: Vec<PaletteItem> = crate::commands::palette_commands(state)
        .into_iter()
        .map(|(label, cmd)| PaletteItem {
            label,
            action: PaletteAction::Command(cmd),
        })
        .collect();

    // Add theme-switching items.
    for (i, theme) in state.themes_fonts.available_themes.iter().enumerate() {
        items.push(PaletteItem {
            label: format!("Set Theme: {}", theme.name),
            action: PaletteAction::SetTheme(i),
        });
    }
    // Add font items.
    for (i, font) in state.themes_fonts.available_fonts.iter().enumerate() {
        items.push(PaletteItem {
            label: format!("Set Font: {}", font.family),
            action: PaletteAction::SetFont(i),
        });
    }

    for shell in crate::settings::shell_options() {
        if let Some(command) = shell.command {
            items.push(PaletteItem {
                label: format!("New Tab ({})", shell.label),
                action: PaletteAction::NewTabWithShell(command),
            });
        }
    }

    items.sort_by_key(|item| item.label.to_lowercase());

    let n = items.len();
    let filtered: Vec<usize> = (0..n).collect();
    state.open_command_palette_modal(CommandPaletteState {
        query: String::new(),
        cursor_byte: 0,
        all_items: items,
        filtered,
        selected: 0,
        scroll_offset: 0,
    });
}

/// Handle keyboard input while the command palette is open.
fn handle_palette_key(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) {
    match &key_event.logical_key {
        Key::Named(NamedKey::Escape) => {
            state.close_active_modal();
        }
        Key::Named(NamedKey::Enter) => {
            execute_palette_action(state);
        }
        Key::Named(NamedKey::ArrowUp) => {
            if let Some(cp) = state.command_palette.as_mut() {
                cp.move_up();
            }
        }
        Key::Named(NamedKey::ArrowDown) => {
            if let Some(cp) = state.command_palette.as_mut() {
                cp.move_down();
            }
        }
        Key::Named(NamedKey::Backspace) => {
            if let Some(cp) = state.command_palette.as_mut()
                && let Some((byte_start, _)) = cp.query[..cp.cursor_byte].char_indices().next_back()
            {
                cp.query.remove(byte_start);
                cp.cursor_byte = byte_start;
                cp.refilter();
            }
        }
        Key::Character(ch) if !state.modifiers.super_down && !state.modifiers.ctrl_down => {
            let text = ch.as_str();
            if !text.is_empty()
                && !text.contains('\r')
                && !text.contains('\n')
                && let Some(cp) = state.command_palette.as_mut()
            {
                cp.query.insert_str(cp.cursor_byte, text);
                cp.cursor_byte += text.len();
                cp.refilter();
            }
        }
        _ => {}
    }
}

/// Execute the currently selected palette action, then close the palette.
/// Also callable by the pointer handler for click-to-execute.
pub(crate) fn palette_execute_from_pointer(state: &mut GpuRuntimeState) {
    execute_palette_action(state);
}

/// Execute the currently selected palette action, then close the palette.
fn execute_palette_action(state: &mut GpuRuntimeState) {
    let Some(cp) = state.command_palette.take() else {
        return;
    };
    let Some(&item_idx) = cp.filtered.get(cp.selected) else {
        return;
    };
    let action = cp.all_items[item_idx].action.clone();

    use crate::state::PaletteAction;
    match action {
        PaletteAction::Command(cmd) => crate::commands::execute_ui_command(
            state,
            cmd,
            crate::commands::CommandContext::default(),
        ),
        PaletteAction::SetTheme(idx) => {
            if let Some(theme) = state.themes_fonts.available_themes.get(idx).cloned() {
                state.themes_fonts.active_theme_idx = Some(idx);
                crate::settings::apply_theme_file(&mut state.user_config, &theme);
                crate::config::save_config(&state.user_config);
            }
        }
        PaletteAction::SetFont(idx) => {
            if let Some(font) = state.themes_fonts.available_fonts.get(idx).cloned() {
                state.themes_fonts.active_font_idx = idx;
                state.user_config.font.family = if font.family == "(default)" {
                    None
                } else {
                    Some(font.family.clone())
                };
                crate::config::save_config(&state.user_config);
                state.push_toast(
                    format!("Font set to {}", font.family),
                    crate::state::ToastKind::Success,
                );
            }
        }
        PaletteAction::NewTabWithShell(shell) => {
            state.add_new_tab_with_shell(Some(shell.as_str()));
        }
    }
}
