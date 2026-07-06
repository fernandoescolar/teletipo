use crate::GpuRuntimeState;
use crate::coords::{
    clamp_editor_scroll, current_line_prefix, cursor_at_line_end, editor_cursor_row_col,
    editor_line_end_offset, editor_line_start_offset, editor_row_col_to_offset, extract_selection,
    line_leading_spaces, replace_cursor_line,
};
use crate::search;
use crate::settings;
use platform_abstraction::{
    AppWindowEvent, InputState, KeyCode, KeyboardEvent, LogicalKey, NamedKey, PhysicalKey,
};

pub(super) fn handle_event(state: &mut GpuRuntimeState, event: AppWindowEvent) {
    let AppWindowEvent::KeyboardInput(key_event) = &event else {
        if let AppWindowEvent::ImeCommit(text) = event
            && !text.is_empty()
            && text != "\r"
            && text != "\n"
        {
            if is_pty_mode(state) {
                // In PTY mode the program is responsible for interpreting the
                // input; send as-is.
                state.send_terminal_input(text.as_bytes());
            } else {
                // In editor mode strip control characters (same policy as paste).
                let safe: String = text
                    .chars()
                    .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
                    .collect();
                if !safe.is_empty() {
                    state.tab_mut().app.insert_editor_input(&safe);
                }
            }
        }
        return;
    };

    if key_event.state != InputState::Pressed {
        return;
    }

    if try_user_keybinding(state, key_event) {
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

fn handle_pre_dispatch(state: &mut GpuRuntimeState, key_event: &KeyboardEvent) -> bool {
    if state.keybindings_panel.open {
        crate::keybindings_ui::handle_keybindings_key(state, key_event);
        return true;
    }
    if state.settings.open {
        settings::handle_settings_key(state, key_event);
        return true;
    }
    if state.command_palette.is_some() {
        crate::palette::handle_key(state, key_event);
        return true;
    }
    if state.tab().search.active {
        handle_search_key(state, key_event);
        return true;
    }
    if state.tab().copy_mode.active {
        return crate::input::copy_mode::handle_copy_mode_key(state, key_event);
    }
    if handle_non_macos_ctrl_shortcuts(state, key_event) {
        return true;
    }

    // Windows often intercepts Win+Shift+P, so keep Ctrl+Shift+P as a
    // reliable cross-platform way to open the command palette.
    if let LogicalKey::Character(ch) = &key_event.logical_key
        && state.modifiers.ctrl_down
        && state.modifiers.shift_down
        && ch.as_str().eq_ignore_ascii_case("p")
    {
        crate::palette::open(state);
        return true;
    }
    false
}

#[cfg(not(target_os = "macos"))]
fn handle_non_macos_ctrl_shortcuts(state: &mut GpuRuntimeState, key_event: &KeyboardEvent) -> bool {
    if let LogicalKey::Character(ch) = &key_event.logical_key
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
            LogicalKey::Named(NamedKey::PageUp) => {
                state.active_tab = state.active_tab.saturating_sub(1);
                state.push_accessibility_tree();
                return true;
            }
            LogicalKey::Named(NamedKey::PageDown) => {
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

#[cfg(target_os = "macos")]
fn handle_non_macos_ctrl_shortcuts(
    _state: &mut GpuRuntimeState,
    _key_event: &KeyboardEvent,
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

/// Returns `true` when keyboard input should go straight to the PTY.
/// In this mode all bytes (including control chars and escape sequences) are
/// legitimate — the running program or shell is in charge.
/// When `false` the inline editor is active and only editor-level actions
/// should be performed; nothing should be written to the PTY unexpectedly.
pub(crate) fn is_pty_mode(state: &GpuRuntimeState) -> bool {
    state.tab().app.is_alternate_screen()
        || (state.tab().command_running && !state.tab().editor_unlocked)
}

fn try_route_to_pty(state: &mut GpuRuntimeState, key_event: &KeyboardEvent) -> bool {
    let is_alternate = state.tab().app.is_alternate_screen();
    let command_running = state.tab().command_running;
    let editor_unlocked = state.tab().editor_unlocked;

    // Ctrl+N while a foreground command is running (but not in alternate screen)
    // toggles the editor unlocked state so the user can prepare the next command.
    if command_running
        && !is_alternate
        && state.modifiers.ctrl_down
        && !state.modifiers.super_down
        && matches!(&key_event.logical_key, LogicalKey::Character(ch) if ch.as_str() == "n")
    {
        let active = state.active_tab;
        state.tabs[active].editor_unlocked = !editor_unlocked;
        return true;
    }

    let route_to_pty = is_pty_mode(state);
    if !route_to_pty || state.modifiers.super_down {
        return false;
    }

    let app_cursor = state.tab().app.application_cursor_keys();
    let kitty_flags = state.tab().app.kitty_keyboard_flags();

    // Kitty keyboard protocol: if any flags are active, encode every key as
    // CSI u so the app can distinguish modifiers, key-up events, etc.
    if kitty_flags != 0
        && let Some(seq) = kitty_encode(key_event, &state.modifiers, kitty_flags, app_cursor)
    {
        state.send_terminal_input(seq.as_bytes());
        return true;
    }

    match &key_event.logical_key {
        LogicalKey::Character(ch) if state.modifiers.ctrl_down && ch.as_str() == "," => false,
        LogicalKey::Character(ch)
            if state.modifiers.ctrl_down
                && state.modifiers.shift_down
                && ch.as_str().eq_ignore_ascii_case("c") =>
        {
            false
        }
        LogicalKey::Character(ch)
            if state.modifiers.ctrl_down
                && state.modifiers.shift_down
                && ch.as_str().eq_ignore_ascii_case("v") =>
        {
            false
        }
        LogicalKey::Named(named) => {
            if matches!(named, NamedKey::Paste) {
                return false;
            }
            if let Some(seq) = pty_named_sequence(named, app_cursor) {
                state.send_terminal_input(seq);
                true
            } else {
                false
            }
        }
        LogicalKey::Character(ch) if state.modifiers.ctrl_down => {
            send_ctrl_character_to_terminal(state, ch.as_str())
        }
        LogicalKey::Character(ch) => {
            let text = key_event.text.as_deref().unwrap_or(ch.as_str());
            if !text.is_empty() && text != "\n" && text != "\r" && text != "\r\n" {
                state.send_terminal_input(text.as_bytes());
            }
            true
        }
    }
}

/// Encode a key event in kitty keyboard protocol CSI u format.
/// Returns `None` when the key cannot be represented (e.g. bare modifier keys).
fn kitty_encode(
    key_event: &KeyboardEvent,
    mods: &crate::ModifierState,
    kitty_flags: u32,
    _app_cursor: bool,
) -> Option<String> {
    use platform_abstraction::{LogicalKey, NamedKey};
    // Kitty modifier bitmask: Shift=1, Alt=2, Ctrl=4, Super=8
    let mut mod_bits: u32 = 0;
    if mods.shift_down {
        mod_bits |= 1;
    }
    if mods.alt_down {
        mod_bits |= 2;
    }
    if mods.ctrl_down {
        mod_bits |= 4;
    }
    if mods.super_down {
        mod_bits |= 8;
    }
    let modifier_param = mod_bits + 1; // kitty adds 1 to the bitmask

    // Map named keys to their kitty codepoints.
    let codepoint: u32 = match &key_event.logical_key {
        LogicalKey::Named(NamedKey::Enter) => 13,
        LogicalKey::Named(NamedKey::Escape) => 27,
        LogicalKey::Named(NamedKey::Tab) => 9,
        LogicalKey::Named(NamedKey::Backspace) => 127,
        LogicalKey::Named(NamedKey::Space) => 32,
        LogicalKey::Named(NamedKey::ArrowUp) => 57352,
        LogicalKey::Named(NamedKey::ArrowDown) => 57353,
        LogicalKey::Named(NamedKey::ArrowLeft) => 57354,
        LogicalKey::Named(NamedKey::ArrowRight) => 57355,
        LogicalKey::Named(NamedKey::Home) => 57356,
        LogicalKey::Named(NamedKey::End) => 57357,
        LogicalKey::Named(NamedKey::PageUp) => 57358,
        LogicalKey::Named(NamedKey::PageDown) => 57359,
        LogicalKey::Named(NamedKey::Insert) => 57360,
        LogicalKey::Named(NamedKey::Delete) => 57361,
        LogicalKey::Named(NamedKey::F1) => 57364,
        LogicalKey::Named(NamedKey::F2) => 57365,
        LogicalKey::Named(NamedKey::F3) => 57366,
        LogicalKey::Named(NamedKey::F4) => 57367,
        LogicalKey::Named(NamedKey::F5) => 57368,
        LogicalKey::Named(NamedKey::F6) => 57369,
        LogicalKey::Named(NamedKey::F7) => 57370,
        LogicalKey::Named(NamedKey::F8) => 57371,
        LogicalKey::Named(NamedKey::F9) => 57372,
        LogicalKey::Named(NamedKey::F10) => 57373,
        LogicalKey::Named(NamedKey::F11) => 57374,
        LogicalKey::Named(NamedKey::F12) => 57375,
        LogicalKey::Character(ch) => {
            // Use the Unicode codepoint of the character
            if let Some(c) = ch.chars().next() {
                c as u32
            } else {
                return None;
            }
        }
        _ => return None,
    };

    // Bit 1 (report_event_types): include key-up events
    let report_types = kitty_flags & 2 != 0;
    let event_type: u32 = if key_event.state == InputState::Released {
        if !report_types {
            return None; // don't send key-up unless requested
        }
        3 // release
    } else {
        1 // press
    };

    // Build: \x1b[<codepoint>;<modifier>:<event_type>u
    if modifier_param == 1 && event_type == 1 {
        // No modifiers, press — shortest form: \x1b[<cp>u
        Some(format!("\x1b[{codepoint}u"))
    } else if event_type == 1 {
        // Modifier only: \x1b[<cp>;<mod>u
        Some(format!("\x1b[{codepoint};{modifier_param}u"))
    } else {
        // Full form: \x1b[<cp>;<mod>:<event>u
        Some(format!("\x1b[{codepoint};{modifier_param}:{event_type}u"))
    }
}

fn reset_suggestion_cycle_if_needed(
    state: &mut GpuRuntimeState,
    key_event: &KeyboardEvent,
) -> bool {
    // Any key other than Tab/Shift+Tab ends the suggestion-cycling session so
    // that subsequent ghost-text lookups start fresh from the new editor text.
    // Exception: Up/Down and Esc are allowed to handle the dropdown themselves.
    let is_tab = matches!(&key_event.logical_key, LogicalKey::Named(NamedKey::Tab));
    let is_nav = matches!(
        &key_event.logical_key,
        LogicalKey::Named(NamedKey::ArrowUp)
            | LogicalKey::Named(NamedKey::ArrowDown)
            | LogicalKey::Named(NamedKey::ArrowRight)
    );
    let is_esc = matches!(&key_event.logical_key, LogicalKey::Named(NamedKey::Escape));
    let is_enter = matches!(&key_event.logical_key, LogicalKey::Named(NamedKey::Enter));
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

fn clear_selection_if_needed(state: &mut GpuRuntimeState, key_event: &KeyboardEvent) {
    let is_modifier_key = matches!(
        &key_event.logical_key,
        LogicalKey::Named(
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
        &state.shell,
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
        &state.shell,
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

fn apply_ghost_suggestion_if_visible(state: &mut GpuRuntimeState) -> bool {
    let editor_text = state.tab().app.editor_snapshot();
    let cursor = state.tab().app.editor_cursor_offset();
    if !cursor_at_line_end(&editor_text, cursor) {
        return false;
    }

    let prefix = current_line_prefix(&editor_text, cursor);
    if prefix.is_empty() {
        return false;
    }

    let active = state.active_tab;
    let matches = crate::suggestion_matches_frecency(
        &state.tabs[active].history,
        &state.tabs[active].history_entries,
        prefix,
        &state.tabs[active].cwd,
        &state.shell,
    );
    let Some(first) = matches.first() else {
        return false;
    };
    if !first.starts_with(prefix) {
        return false;
    }

    state.tabs[active].suggestion_prefix = Some(prefix.to_owned());
    state.tabs[active].suggestion_index = Some(0);
    apply_selected_suggestion(state);
    true
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
        &state.shell,
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
            let route_to_pty = state.tab().app.is_alternate_screen()
                || (state.tab().command_running && !state.tab().editor_unlocked);
            if route_to_pty {
                if state.tab().app.bracketed_paste() {
                    // Bracketed paste wraps the text so the shell treats it as
                    // literal input; no further sanitization needed.
                    let bracketed = format!("\x1b[200~{normalized}\x1b[201~");
                    state.send_terminal_input(bracketed.as_bytes());
                } else {
                    // No bracketed paste: strip control characters that could
                    // trigger unintended shell actions (ESC sequences, ^C, EOF…).
                    // Keep \n and \t which are legitimate in multi-line pastes.
                    let safe: String = normalized
                        .chars()
                        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
                        .collect();
                    state.send_terminal_input(safe.as_bytes());
                }
            } else {
                // Inline editor: strip control characters that have no meaning
                // as text (keep \n for multi-line and \t for indentation).
                let safe: String = normalized
                    .chars()
                    .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
                    .collect();
                state.tab_mut().app.insert_editor_input(&safe);
            }
        }
    }
    true
}

/// Public wrapper called by `execute_ui_command(CommandId::Copy)`.
pub(crate) fn execute_copy(state: &mut GpuRuntimeState) {
    // `handle_copy_shortcut` checks `is_copy_shortcut(state, "c")` which
    // requires super_down on macOS. Bypass the guard by invoking the copy
    // path directly via the existing function; it's the cleanest factored unit.
    let saved_selection = capture_terminal_selection(state);
    // Temporarily simulate super_down so the copy guard passes.
    let prev = state.modifiers.super_down;
    state.modifiers.super_down = true;
    handle_copy_shortcut(state, "c", &saved_selection);
    state.modifiers.super_down = prev;
}

/// Public wrapper called by `execute_ui_command(CommandId::Paste)`.
pub(crate) fn execute_paste(state: &mut GpuRuntimeState) {
    handle_paste_shortcut(state, "v");
}

/// Public wrapper called by `execute_ui_command(CommandId::ZoomIn/ZoomOut)`.
/// `delta` is +1.0 or -1.0.
pub(crate) fn execute_zoom(state: &mut GpuRuntimeState, delta: f32) {
    let new_size = (state.user_config.font.size + delta)
        .clamp(crate::config::FONT_SIZE_MIN, crate::config::FONT_SIZE_MAX);
    state
        .user_config
        .set_field("font", "size", &format!("{new_size:.1}"));
    crate::config::save_config(&state.user_config);
}

/// Returns `true` and fires the matching command if any user keybinding matches
/// the current key event + modifier state. Must be called at the top of the
/// main keyboard dispatch before the built-in shortcut logic.
pub(crate) fn try_user_keybinding(state: &mut GpuRuntimeState, key_event: &KeyboardEvent) -> bool {
    use platform_abstraction::{LogicalKey, NamedKey};
    if state.user_config.keybindings.is_empty() {
        return false;
    }
    for binding in state.user_config.keybindings.clone() {
        // Match modifiers
        let wants_cmd = binding
            .modifiers
            .iter()
            .any(|m| m.eq_ignore_ascii_case("Cmd") || m.eq_ignore_ascii_case("Super"));
        let wants_ctrl = binding
            .modifiers
            .iter()
            .any(|m| m.eq_ignore_ascii_case("Ctrl"));
        let wants_shift = binding
            .modifiers
            .iter()
            .any(|m| m.eq_ignore_ascii_case("Shift"));
        let wants_alt = binding
            .modifiers
            .iter()
            .any(|m| m.eq_ignore_ascii_case("Alt") || m.eq_ignore_ascii_case("Option"));
        if wants_cmd != state.modifiers.super_down
            || wants_ctrl != state.modifiers.ctrl_down
            || wants_shift != state.modifiers.shift_down
            || wants_alt != state.modifiers.alt_down
        {
            continue;
        }
        // Match key
        let key_str = binding.key.trim();
        let matched = match &key_event.logical_key {
            LogicalKey::Character(ch) => ch.as_str().eq_ignore_ascii_case(key_str),
            LogicalKey::Named(NamedKey::Enter) => {
                key_str.eq_ignore_ascii_case("Return") || key_str.eq_ignore_ascii_case("Enter")
            }
            LogicalKey::Named(NamedKey::Escape) => key_str.eq_ignore_ascii_case("Escape"),
            LogicalKey::Named(NamedKey::Tab) => key_str.eq_ignore_ascii_case("Tab"),
            LogicalKey::Named(NamedKey::Backspace) => key_str.eq_ignore_ascii_case("BackSpace"),
            LogicalKey::Named(NamedKey::Space) => key_str.eq_ignore_ascii_case("Space"),
            LogicalKey::Named(NamedKey::F1) => key_str.eq_ignore_ascii_case("F1"),
            LogicalKey::Named(NamedKey::F2) => key_str.eq_ignore_ascii_case("F2"),
            LogicalKey::Named(NamedKey::F3) => key_str.eq_ignore_ascii_case("F3"),
            LogicalKey::Named(NamedKey::F4) => key_str.eq_ignore_ascii_case("F4"),
            LogicalKey::Named(NamedKey::F5) => key_str.eq_ignore_ascii_case("F5"),
            LogicalKey::Named(NamedKey::F6) => key_str.eq_ignore_ascii_case("F6"),
            LogicalKey::Named(NamedKey::F7) => key_str.eq_ignore_ascii_case("F7"),
            LogicalKey::Named(NamedKey::F8) => key_str.eq_ignore_ascii_case("F8"),
            LogicalKey::Named(NamedKey::F9) => key_str.eq_ignore_ascii_case("F9"),
            LogicalKey::Named(NamedKey::F10) => key_str.eq_ignore_ascii_case("F10"),
            LogicalKey::Named(NamedKey::F11) => key_str.eq_ignore_ascii_case("F11"),
            LogicalKey::Named(NamedKey::F12) => key_str.eq_ignore_ascii_case("F12"),
            _ => false,
        };
        if matched && let Some(cmd) = crate::commands::CommandId::from_name(&binding.action) {
            crate::commands::execute_ui_command(
                state,
                cmd,
                crate::commands::CommandContext::default(),
            );
            return true;
        }
    }
    false
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
        "z" => {
            if state.modifiers.shift_down {
                state.tab_mut().app.editor_redo();
            } else {
                state.tab_mut().app.editor_undo();
            }
        }
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
            crate::palette::open(state);
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
        "=" | "+" => {
            let new_size = (state.user_config.font.size + 1.0).min(crate::config::FONT_SIZE_MAX);
            state
                .user_config
                .set_field("font", "size", &format!("{new_size:.1}"));
        }
        "-" => {
            let new_size = (state.user_config.font.size - 1.0).max(crate::config::FONT_SIZE_MIN);
            state
                .user_config
                .set_field("font", "size", &format!("{new_size:.1}"));
        }
        "0" => {
            state.user_config.set_field("font", "size", "14.0");
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
    if ch.eq_ignore_ascii_case("z") {
        if state.modifiers.shift_down {
            state.tab_mut().app.editor_redo();
        } else {
            state.tab_mut().app.editor_undo();
        }
        return true;
    }
    if ch.eq_ignore_ascii_case("y") {
        state.tab_mut().app.editor_redo();
        return true;
    }

    // While the inline editor is active (no command running, not in alternate
    // screen), Ctrl+C/D/U/W should not leak to the PTY — the shell would print
    // "^C", move the cursor, and redraw the prompt, causing the editor overlay
    // to render at the wrong column on the next keystroke.  Handle them locally
    // instead, mirroring standard readline behaviour.
    let in_editor = !state.tab().command_running && !state.tab().app.is_alternate_screen();
    if in_editor {
        if ch.eq_ignore_ascii_case("c") {
            // Ctrl+C: discard the current editor line (same as readline).
            state.tab_mut().app.editor_clear();
            return true;
        }
        if ch.eq_ignore_ascii_case("u") {
            // Ctrl+U: delete from cursor to start of line.
            state.tab_mut().app.editor_delete_to_line_start();
            return true;
        }
        if ch.eq_ignore_ascii_case("k") {
            // Ctrl+K: delete from cursor to end of line.
            state.tab_mut().app.editor_delete_to_line_end();
            return true;
        }
        if ch.eq_ignore_ascii_case("w") {
            // Ctrl+W: delete the word before the cursor.
            state.tab_mut().app.editor_delete_word_backward();
            return true;
        }
        if ch.eq_ignore_ascii_case("d") {
            // Ctrl+D on empty editor: do nothing (no EOF to the shell).
            return true;
        }
        // For any other Ctrl+key while in editor mode, do nothing rather than
        // accidentally mutating PTY/shell state.
        return true;
    }

    send_ctrl_character_to_terminal(state, ch);
    true
}

fn handle_character_key(
    state: &mut GpuRuntimeState,
    key_event: &KeyboardEvent,
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
            if state.overlays.pending_update.is_some() {
                state.overlays.pending_update = None;
            } else if cycling {
                state.tabs[state.active_tab].suggestion_prefix = None;
                state.tabs[state.active_tab].suggestion_index = None;
            } else if is_pty_mode(state) {
                // Only forward ESC to the PTY when a command is running.
                // In editor mode ESC has no meaningful action and sending \x1b
                // could corrupt the shell's parser state.
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
            if !state.modifiers.shift_down {
                if cycling {
                    apply_selected_suggestion(state);
                    return true;
                }
                if apply_ghost_suggestion_if_visible(state) {
                    return true;
                }
            }
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
                    &state.shell,
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
                    &state.shell,
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
            let extend = state.modifiers.shift_down;
            let text = state.tab().app.editor_snapshot();
            let offset = state.tab().app.editor_cursor_offset();
            let target = if state.modifiers.ctrl_down || state.modifiers.super_down {
                0
            } else {
                editor_line_start_offset(&text, offset)
            };
            state.tab_mut().app.set_editor_cursor(target, extend);
            true
        }
        NamedKey::End => {
            let extend = state.modifiers.shift_down;
            let text = state.tab().app.editor_snapshot();
            let offset = state.tab().app.editor_cursor_offset();
            let target = if state.modifiers.ctrl_down || state.modifiers.super_down {
                text.len()
            } else {
                editor_line_end_offset(&text, offset)
            };
            state.tab_mut().app.set_editor_cursor(target, extend);
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
    key_event: &KeyboardEvent,
    cycling: bool,
    saved_selection: &SavedSelection,
) {
    if let LogicalKey::Named(named) = &key_event.logical_key
        && handle_named_key(state, named, cycling)
    {
        return;
    }

    if let LogicalKey::Character(ch) = &key_event.logical_key {
        handle_character_key(state, key_event, cycling, saved_selection, ch.as_str());
    }
}

fn handle_search_key(state: &mut GpuRuntimeState, key_event: &KeyboardEvent) {
    let shift = state.modifiers.shift_down;
    let alt = state.modifiers.alt_down;
    let super_ = state.modifiers.super_down;

    match &key_event.logical_key {
        LogicalKey::Named(NamedKey::Escape) => close_search_overlay(state),
        LogicalKey::Named(NamedKey::Enter) => search_enter(state, shift),
        LogicalKey::Named(NamedKey::ArrowLeft) => search_move_left(state, super_, alt, shift),
        LogicalKey::Named(NamedKey::ArrowRight) => search_move_right(state, super_, alt, shift),
        LogicalKey::Named(NamedKey::ArrowUp) => search::prev_match(state.tab_mut()),
        LogicalKey::Named(NamedKey::ArrowDown) => search::next_match(state.tab_mut()),
        LogicalKey::Named(NamedKey::Home) => search::search_move_home(state.tab_mut(), shift),
        LogicalKey::Named(NamedKey::End) => search::search_move_end(state.tab_mut(), shift),
        LogicalKey::Named(NamedKey::Backspace) => search_backspace(state, alt),
        LogicalKey::Named(NamedKey::Delete) => search::search_delete_forward(state.tab_mut()),
        LogicalKey::Character(ch) => {
            search_character_input(state, key_event, ch.as_str(), alt, super_)
        }
        LogicalKey::Named(NamedKey::Space) => search_insert_key_text(state, key_event),
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
    key_event: &KeyboardEvent,
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

fn search_insert_key_text(state: &mut GpuRuntimeState, key_event: &KeyboardEvent) {
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
/// Execute the currently selected palette action, then close the palette.
/// Also callable by the pointer handler for click-to-execute.
pub(crate) fn palette_execute_from_pointer(state: &mut GpuRuntimeState) {
    crate::palette::execute_from_pointer(state);
}
