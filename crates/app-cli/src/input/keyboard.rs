use crate::GpuRuntimeState;
use crate::coords::{
    clamp_editor_scroll, current_line_prefix, cursor_at_line_end, editor_cursor_row_col,
    editor_line_end_offset, editor_line_start_offset, editor_row_col_to_offset, extract_selection,
    line_leading_spaces, replace_cursor_line,
};
use crate::search;
use crate::settings;
use render_model::AppWindowEvent;
use winit::event::ElementState;
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

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

    if key_event.state != ElementState::Pressed {
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

fn handle_pre_dispatch(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) -> bool {
    if state.keybindings_panel.open {
        crate::keybindings_ui::handle_keybindings_key(state, key_event);
        return true;
    }
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
    if state.tab().copy_mode.active {
        return crate::input::copy_mode::handle_copy_mode_key(state, key_event);
    }
    if handle_non_macos_ctrl_shortcuts(state, key_event) {
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

#[cfg(not(target_os = "macos"))]
fn handle_non_macos_ctrl_shortcuts(
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

#[cfg(target_os = "macos")]
fn handle_non_macos_ctrl_shortcuts(
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

/// Returns `true` when keyboard input should go straight to the PTY.
/// In this mode all bytes (including control chars and escape sequences) are
/// legitimate — the running program or shell is in charge.
/// When `false` the inline editor is active and only editor-level actions
/// should be performed; nothing should be written to the PTY unexpectedly.
pub(crate) fn is_pty_mode(state: &GpuRuntimeState) -> bool {
    state.tab().app.is_alternate_screen()
        || (state.tab().command_running && !state.tab().editor_unlocked)
}

fn try_route_to_pty(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) -> bool {
    let is_alternate = state.tab().app.is_alternate_screen();
    let command_running = state.tab().command_running;
    let editor_unlocked = state.tab().editor_unlocked;

    // Ctrl+N while a foreground command is running (but not in alternate screen)
    // toggles the editor unlocked state so the user can prepare the next command.
    if command_running
        && !is_alternate
        && state.modifiers.ctrl_down
        && !state.modifiers.super_down
        && matches!(&key_event.logical_key, Key::Character(ch) if ch.as_str() == "n")
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
        Key::Character(ch) if state.modifiers.ctrl_down && ch.as_str() == "," => false,
        Key::Character(ch)
            if state.modifiers.ctrl_down
                && state.modifiers.shift_down
                && ch.as_str().eq_ignore_ascii_case("v") =>
        {
            false
        }
        Key::Named(named) => {
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

/// Encode a key event in kitty keyboard protocol CSI u format.
/// Returns `None` when the key cannot be represented (e.g. bare modifier keys).
fn kitty_encode(
    key_event: &winit::event::KeyEvent,
    mods: &crate::ModifierState,
    kitty_flags: u32,
    _app_cursor: bool,
) -> Option<String> {
    use winit::keyboard::{Key, NamedKey};
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
        Key::Named(NamedKey::Enter) => 13,
        Key::Named(NamedKey::Escape) => 27,
        Key::Named(NamedKey::Tab) => 9,
        Key::Named(NamedKey::Backspace) => 127,
        Key::Named(NamedKey::Space) => 32,
        Key::Named(NamedKey::ArrowUp) => 57352,
        Key::Named(NamedKey::ArrowDown) => 57353,
        Key::Named(NamedKey::ArrowLeft) => 57354,
        Key::Named(NamedKey::ArrowRight) => 57355,
        Key::Named(NamedKey::Home) => 57356,
        Key::Named(NamedKey::End) => 57357,
        Key::Named(NamedKey::PageUp) => 57358,
        Key::Named(NamedKey::PageDown) => 57359,
        Key::Named(NamedKey::Insert) => 57360,
        Key::Named(NamedKey::Delete) => 57361,
        Key::Named(NamedKey::F1) => 57364,
        Key::Named(NamedKey::F2) => 57365,
        Key::Named(NamedKey::F3) => 57366,
        Key::Named(NamedKey::F4) => 57367,
        Key::Named(NamedKey::F5) => 57368,
        Key::Named(NamedKey::F6) => 57369,
        Key::Named(NamedKey::F7) => 57370,
        Key::Named(NamedKey::F8) => 57371,
        Key::Named(NamedKey::F9) => 57372,
        Key::Named(NamedKey::F10) => 57373,
        Key::Named(NamedKey::F11) => 57374,
        Key::Named(NamedKey::F12) => 57375,
        Key::Character(ch) => {
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
    let event_type: u32 = if key_event.state == winit::event::ElementState::Released {
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
    key_event: &winit::event::KeyEvent,
) -> bool {
    // Any key other than Tab/Shift+Tab ends the suggestion-cycling session so
    // that subsequent ghost-text lookups start fresh from the new editor text.
    // Exception: Up/Down and Esc are allowed to handle the dropdown themselves.
    let is_tab = matches!(&key_event.logical_key, Key::Named(NamedKey::Tab));
    let is_nav = matches!(
        &key_event.logical_key,
        Key::Named(NamedKey::ArrowUp)
            | Key::Named(NamedKey::ArrowDown)
            | Key::Named(NamedKey::ArrowRight)
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
pub(crate) fn try_user_keybinding(
    state: &mut GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
) -> bool {
    use winit::keyboard::{Key, NamedKey};
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
            Key::Character(ch) => ch.as_str().eq_ignore_ascii_case(key_str),
            Key::Named(NamedKey::Enter) => {
                key_str.eq_ignore_ascii_case("Return") || key_str.eq_ignore_ascii_case("Enter")
            }
            Key::Named(NamedKey::Escape) => key_str.eq_ignore_ascii_case("Escape"),
            Key::Named(NamedKey::Tab) => key_str.eq_ignore_ascii_case("Tab"),
            Key::Named(NamedKey::Backspace) => key_str.eq_ignore_ascii_case("BackSpace"),
            Key::Named(NamedKey::Space) => key_str.eq_ignore_ascii_case("Space"),
            Key::Named(NamedKey::F1) => key_str.eq_ignore_ascii_case("F1"),
            Key::Named(NamedKey::F2) => key_str.eq_ignore_ascii_case("F2"),
            Key::Named(NamedKey::F3) => key_str.eq_ignore_ascii_case("F3"),
            Key::Named(NamedKey::F4) => key_str.eq_ignore_ascii_case("F4"),
            Key::Named(NamedKey::F5) => key_str.eq_ignore_ascii_case("F5"),
            Key::Named(NamedKey::F6) => key_str.eq_ignore_ascii_case("F6"),
            Key::Named(NamedKey::F7) => key_str.eq_ignore_ascii_case("F7"),
            Key::Named(NamedKey::F8) => key_str.eq_ignore_ascii_case("F8"),
            Key::Named(NamedKey::F9) => key_str.eq_ignore_ascii_case("F9"),
            Key::Named(NamedKey::F10) => key_str.eq_ignore_ascii_case("F10"),
            Key::Named(NamedKey::F11) => key_str.eq_ignore_ascii_case("F11"),
            Key::Named(NamedKey::F12) => key_str.eq_ignore_ascii_case("F12"),
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
pub(crate) fn open_command_palette(state: &mut GpuRuntimeState) {
    use crate::state::CommandPaletteState;

    let mut primary = build_palette_primary(state);
    primary.sort_by_key(|item| item.label.to_lowercase());

    let mut secondary = build_palette_secondary(state);
    secondary.sort_by_key(|item| item.label.to_lowercase());

    let primary_len = primary.len();
    let mut items = primary;
    items.extend(secondary);

    let default_filtered: Vec<usize> = (0..primary_len).collect();
    let filtered = default_filtered.clone();

    state.open_command_palette_modal(CommandPaletteState {
        query: String::new(),
        cursor_byte: 0,
        all_items: items,
        default_filtered,
        filtered,
        selected: 0,
        scroll_offset: 0,
        sub_prompt: None,
    });
}

fn build_palette_primary(state: &GpuRuntimeState) -> Vec<crate::state::PaletteItem> {
    use crate::state::{PaletteAction, PaletteItem};

    let mut items: Vec<PaletteItem> = crate::commands::palette_commands(state)
        .into_iter()
        .map(|(label, cmd)| PaletteItem {
            label,
            action: PaletteAction::Command(cmd),
        })
        .collect();

    items.push(PaletteItem {
        label: "SSH → New connection…".to_owned(),
        action: PaletteAction::OpenSshPrompt,
    });

    let active = state.active_tab;
    let headers: &[(&str, &str, bool)] = &[
        (
            "Set Theme…",
            "Set Theme: ",
            !state.themes_fonts.available_themes.is_empty(),
        ),
        (
            "Set Font…",
            "Set Font: ",
            !state.themes_fonts.available_fonts.is_empty(),
        ),
        (
            "New Tab (shell)…",
            "New Tab (",
            crate::settings::shell_options()
                .iter()
                .any(|s| s.command.is_some()),
        ),
        ("SSH → host…", "SSH → ", !state.ssh_hosts.is_empty()),
        ("Switch Tab…", "Tab ", state.tabs.len() > 1),
        (
            "History…",
            "History: ",
            !state.tabs[active].history_entries.is_empty(),
        ),
    ];
    for &(label, prefix, enabled) in headers {
        if enabled {
            items.push(PaletteItem {
                label: label.to_owned(),
                action: PaletteAction::FilterByPrefix(prefix.to_owned()),
            });
        }
    }

    items
}

fn build_palette_secondary(state: &GpuRuntimeState) -> Vec<crate::state::PaletteItem> {
    use crate::state::{PaletteAction, PaletteItem};
    use std::cmp::Reverse;

    let active = state.active_tab;
    let mut items: Vec<PaletteItem> = Vec::new();

    for (i, theme) in state.themes_fonts.available_themes.iter().enumerate() {
        items.push(PaletteItem {
            label: format!("Set Theme: {}", theme.name),
            action: PaletteAction::SetTheme(i),
        });
    }
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
    for host in &state.ssh_hosts {
        items.push(PaletteItem {
            label: format!("SSH → {}", host.name),
            action: PaletteAction::NewSshTab(host.ssh_command()),
        });
    }
    for (i, tab) in state.tabs.iter().enumerate() {
        if i == active {
            continue;
        }
        let label = if tab.cwd.is_empty() {
            format!("Tab {}", i + 1)
        } else {
            format!("Tab {}: {}", i + 1, tab.cwd)
        };
        items.push(PaletteItem {
            label,
            action: PaletteAction::SwitchToTab(i),
        });
    }

    // Command history: frecency-sorted, deduped, capped at 100.
    let tab = &state.tabs[active];
    let mut history_scored: Vec<(&str, u64)> = tab
        .history_entries
        .iter()
        .map(|e| {
            let score = (e.count as u64)
                .saturating_mul(1_000_000)
                .saturating_add(e.last_used_secs);
            (e.cmd.as_str(), score)
        })
        .collect();
    history_scored.sort_by_key(|&(_, score)| Reverse(score));

    let mut seen = std::collections::HashSet::new();
    for (cmd, _) in history_scored.into_iter().take(100) {
        if seen.insert(cmd) {
            items.push(PaletteItem {
                label: format!("History: {cmd}"),
                action: PaletteAction::InsertHistoryCommand(cmd.to_owned()),
            });
        }
    }

    items
}

/// Handle keyboard input while the command palette is open.
fn handle_palette_key(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) {
    // Sub-prompt mode: all navigation is disabled; only text input, Enter, and Escape work.
    if state
        .command_palette
        .as_ref()
        .is_some_and(|cp| cp.sub_prompt.is_some())
    {
        match &key_event.logical_key {
            Key::Named(NamedKey::Escape) => {
                // Go back to the normal palette instead of closing entirely.
                open_command_palette(state);
            }
            Key::Named(NamedKey::Enter) => {
                execute_palette_action(state);
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(cp) = state.command_palette.as_mut()
                    && let Some((byte_start, _)) =
                        cp.query[..cp.cursor_byte].char_indices().next_back()
                {
                    cp.query.remove(byte_start);
                    cp.cursor_byte = byte_start;
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
                }
            }
            _ => {}
        }
        return;
    }

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

    // Sub-prompt mode: Enter confirms the typed destination and opens a new SSH tab.
    if let Some(ref kind) = cp.sub_prompt {
        use crate::state::SubPrompt;
        match kind {
            SubPrompt::Ssh => {
                let dest = cp.query.trim().to_owned();
                if !dest.is_empty() {
                    let cmd = format!("ssh {dest}");
                    state.add_new_tab_with_exec(&cmd);
                }
            }
        }
        // Always close the palette after confirming (modal was already taken).
        if state.overlays.active_modal == Some(crate::state::ModalOverlay::CommandPalette) {
            state.overlays.active_modal = None;
        }
        return;
    }

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
        PaletteAction::NewSshTab(cmd) => {
            state.add_new_tab_with_exec(&cmd);
        }
        PaletteAction::OpenSshPrompt => {
            state.open_command_palette_modal(crate::state::CommandPaletteState {
                query: String::new(),
                cursor_byte: 0,
                default_filtered: cp.default_filtered,
                all_items: cp.all_items,
                filtered: cp.filtered,
                selected: cp.selected,
                scroll_offset: cp.scroll_offset,
                sub_prompt: Some(crate::state::SubPrompt::Ssh),
            });
            // Palette is now open in sub-prompt mode — do not close.
        }
        PaletteAction::SwitchToTab(idx) => {
            if idx < state.tabs.len() {
                state.active_tab = idx;
            }
        }
        PaletteAction::InsertHistoryCommand(cmd) => {
            let tab = &mut state.tabs[state.active_tab];
            tab.app.editor_clear();
            tab.app.insert_editor_input(&cmd);
            tab.history_index = None;
            tab.editor_scroll_offset = 0;
            tab.editor_horizontal_scroll_offset = 0;
        }
        PaletteAction::FilterByPrefix(prefix) => {
            // Reopen the palette with the prefix pre-filled so the user sees
            // only items from that category. The palette stays open.
            let cursor_byte = prefix.len();
            let mut new_cp = crate::state::CommandPaletteState {
                query: prefix,
                cursor_byte,
                default_filtered: cp.default_filtered,
                all_items: cp.all_items,
                filtered: Vec::new(),
                selected: 0,
                scroll_offset: 0,
                sub_prompt: None,
            };
            new_cp.refilter();
            state.open_command_palette_modal(new_cp);
        }
    }
}
