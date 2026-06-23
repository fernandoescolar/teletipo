use crate::config::{KeyBinding, save_config};
use crate::runtime::GpuRuntimeState;
use render_wgpu::{KeybindingRow, KeybindingsOverlay};
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

/// Built-in default key combos shown when no user binding exists for an action.
/// These reflect the hardcoded shortcuts in `input/keyboard.rs`.
/// macOS uses Cmd; Windows/Linux use Ctrl.
#[cfg(target_os = "macos")]
pub(crate) const DEFAULT_BINDINGS: &[(&str, &str)] = &[
    ("new_tab", "Cmd+T"),
    ("close_tab", "Cmd+W"),
    ("move_tab_left", "Cmd+["),
    ("move_tab_right", "Cmd+]"),
    ("copy", "Cmd+C"),
    ("paste", "Cmd+V"),
    ("zoom_in", "Cmd++"),
    ("zoom_out", "Cmd+-"),
    ("open_settings", "Cmd+,"),
    ("open_command_palette", "Cmd+Shift+P"),
];

#[cfg(not(target_os = "macos"))]
pub(crate) const DEFAULT_BINDINGS: &[(&str, &str)] = &[
    ("new_tab", "Ctrl+T"),
    ("close_tab", "Ctrl+W"),
    ("move_tab_left", "Ctrl+["),
    ("move_tab_right", "Ctrl+]"),
    ("copy", "Ctrl+C"),
    ("paste", "Ctrl+V"),
    ("zoom_in", "Ctrl++"),
    ("zoom_out", "Ctrl+-"),
    ("open_settings", "Ctrl+,"),
    ("open_command_palette", "Ctrl+Shift+P"),
];

/// All actions that can be assigned a custom keybinding.
/// Order determines display order in the panel.
pub(crate) const BINDABLE_ACTIONS: &[(&str, &str)] = &[
    ("new_tab", "New Tab"),
    ("close_tab", "Close Tab"),
    ("move_tab_left", "Move Tab Left"),
    ("move_tab_right", "Move Tab Right"),
    ("jump_to_prev_prompt", "Jump to Prev Prompt"),
    ("jump_to_next_prompt", "Jump to Next Prompt"),
    ("copy", "Copy"),
    ("paste", "Paste"),
    ("clear", "Clear Screen"),
    ("zoom_in", "Zoom In"),
    ("zoom_out", "Zoom Out"),
    ("open_settings", "Open Settings"),
    ("open_command_palette", "Open Command Palette"),
    ("open_config_in_editor", "Open Config in Editor"),
];

/// Format a `KeyBinding` as a human-readable combo string (e.g. `"Cmd+Shift+T"`).
fn format_combo(binding: &KeyBinding) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for m in &["Cmd", "Super", "Ctrl", "Alt", "Option", "Shift"] {
        if binding.modifiers.iter().any(|s| s.eq_ignore_ascii_case(m)) {
            let label = if m.eq_ignore_ascii_case("super") {
                "Cmd"
            } else if m.eq_ignore_ascii_case("option") {
                "Alt"
            } else {
                m
            };
            if !parts.contains(&label) {
                parts.push(label);
            }
        }
    }
    let key_lower = binding.key.to_lowercase();
    let key_label = match key_lower.as_str() {
        "return" | "enter" => "Enter",
        "backspace" => "Backspace",
        "escape" => "Esc",
        "space" => "Space",
        "tab" => "Tab",
        _ => binding.key.as_str(),
    };
    if parts.is_empty() {
        key_label.to_owned()
    } else {
        format!("{}+{}", parts.join("+"), key_label)
    }
}

/// Find the first binding for `action_id`, preferring user config then falling
/// back to the built-in default combo (shown in dimmed style by the renderer).
fn find_binding(state: &GpuRuntimeState, action_id: &str) -> Option<(String, bool)> {
    if let Some(b) = state
        .user_config
        .keybindings
        .iter()
        .find(|b| b.action.eq_ignore_ascii_case(action_id))
    {
        return Some((format_combo(b), false)); // false = not a default
    }
    // Fall back to built-in default
    DEFAULT_BINDINGS
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(action_id))
        .map(|(_, combo)| (combo.to_string(), true)) // true = is a default
}

/// Build a `KeybindingsOverlay` from current runtime state. Returns `None` when the
/// panel is closed.
pub(crate) fn build_keybindings_overlay(state: &GpuRuntimeState) -> Option<KeybindingsOverlay> {
    if !state.keybindings_panel.open {
        return None;
    }
    let rows = BINDABLE_ACTIONS
        .iter()
        .map(|(id, label)| {
            let (binding, is_default) = find_binding(state, id)
                .map(|(b, d)| (Some(b), d))
                .unwrap_or((None, false));
            KeybindingRow {
                action_id: id.to_string(),
                label: label.to_string(),
                binding,
                is_default,
            }
        })
        .collect();
    Some(KeybindingsOverlay {
        rows,
        cursor: state.keybindings_panel.cursor,
        scroll_offset: state.keybindings_panel.scroll_offset,
        recording: state.keybindings_panel.recording,
        just_saved: state.keybindings_panel.just_saved,
        visible_rows: VISIBLE_ROWS,
    })
}

/// Open the keybindings panel, resetting transient state.
pub(crate) fn open_keybindings_panel(state: &mut GpuRuntimeState) {
    state.keybindings_panel.open = true;
    state.keybindings_panel.recording = false;
    state.keybindings_panel.just_saved = false;
    state.overlays.active_modal = Some(crate::state::ModalOverlay::Keybindings);
}

/// Close the keybindings panel.
pub(crate) fn close_keybindings_panel(state: &mut GpuRuntimeState) {
    state.keybindings_panel.open = false;
    state.keybindings_panel.recording = false;
    state.overlays.active_modal = None;
}

/// How many rows are visible at once in the panel (before scrolling kicks in).
pub(crate) const VISIBLE_ROWS: usize = 12;

fn clamp_scroll(state: &mut GpuRuntimeState) {
    let n = BINDABLE_ACTIONS.len();
    let cursor = state.keybindings_panel.cursor;
    let offset = &mut state.keybindings_panel.scroll_offset;
    if cursor < *offset {
        *offset = cursor;
    } else if cursor >= *offset + VISIBLE_ROWS {
        *offset = cursor + 1 - VISIBLE_ROWS;
    }
    *offset = (*offset).min(n.saturating_sub(VISIBLE_ROWS));
}

/// Derive modifiers + key name from a key event for saving as a new binding.
fn binding_from_key_event(
    state: &GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
) -> Option<KeyBinding> {
    let key_str = match &key_event.logical_key {
        Key::Character(ch) => ch.to_string().to_uppercase(),
        Key::Named(NamedKey::Enter) => "Return".to_owned(),
        Key::Named(NamedKey::Escape) => "Escape".to_owned(),
        Key::Named(NamedKey::Tab) => "Tab".to_owned(),
        Key::Named(NamedKey::Backspace) => "BackSpace".to_owned(),
        Key::Named(NamedKey::Space) => "Space".to_owned(),
        Key::Named(NamedKey::ArrowUp) => "Up".to_owned(),
        Key::Named(NamedKey::ArrowDown) => "Down".to_owned(),
        Key::Named(NamedKey::ArrowLeft) => "Left".to_owned(),
        Key::Named(NamedKey::ArrowRight) => "Right".to_owned(),
        Key::Named(NamedKey::F1) => "F1".to_owned(),
        Key::Named(NamedKey::F2) => "F2".to_owned(),
        Key::Named(NamedKey::F3) => "F3".to_owned(),
        Key::Named(NamedKey::F4) => "F4".to_owned(),
        Key::Named(NamedKey::F5) => "F5".to_owned(),
        Key::Named(NamedKey::F6) => "F6".to_owned(),
        Key::Named(NamedKey::F7) => "F7".to_owned(),
        Key::Named(NamedKey::F8) => "F8".to_owned(),
        Key::Named(NamedKey::F9) => "F9".to_owned(),
        Key::Named(NamedKey::F10) => "F10".to_owned(),
        Key::Named(NamedKey::F11) => "F11".to_owned(),
        Key::Named(NamedKey::F12) => "F12".to_owned(),
        _ => return None,
    };
    let mut modifiers = Vec::new();
    if state.modifiers.super_down {
        modifiers.push("Cmd".to_owned());
    }
    if state.modifiers.ctrl_down {
        modifiers.push("Ctrl".to_owned());
    }
    if state.modifiers.alt_down {
        modifiers.push("Alt".to_owned());
    }
    if state.modifiers.shift_down {
        modifiers.push("Shift".to_owned());
    }
    Some(KeyBinding {
        key: key_str,
        modifiers,
        action: String::new(), // filled in by caller
    })
}

/// Handle a key event while the keybindings panel is open.
/// Returns `true` if the event was consumed.
pub(crate) fn handle_keybindings_key(
    state: &mut GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
) -> bool {
    if key_event.state != ElementState::Pressed {
        return true;
    }

    // ── Recording mode: capture the next key combo ────────────────────────────
    if state.keybindings_panel.recording {
        match &key_event.logical_key {
            Key::Named(NamedKey::Escape) => {
                // Cancel recording without changing anything.
                state.keybindings_panel.recording = false;
            }
            _ => {
                if let Some(mut binding) = binding_from_key_event(state, key_event) {
                    let action_idx = state.keybindings_panel.cursor;
                    if let Some((action_id, _)) = BINDABLE_ACTIONS.get(action_idx) {
                        binding.action = action_id.to_string();
                        // Remove any existing binding for this action.
                        state
                            .user_config
                            .keybindings
                            .retain(|b| !b.action.eq_ignore_ascii_case(action_id));
                        state.user_config.keybindings.push(binding);
                        save_config(&state.user_config);
                        state.keybindings_panel.just_saved = true;
                    }
                    state.keybindings_panel.recording = false;
                }
                // If binding_from_key_event returned None (e.g. bare modifier), stay recording.
            }
        }
        return true;
    }

    // ── Normal navigation mode ────────────────────────────────────────────────
    let n = BINDABLE_ACTIONS.len();
    match &key_event.logical_key {
        Key::Named(NamedKey::Escape) => {
            close_keybindings_panel(state);
        }
        Key::Named(NamedKey::ArrowUp) => {
            state.keybindings_panel.just_saved = false;
            if state.keybindings_panel.cursor > 0 {
                state.keybindings_panel.cursor -= 1;
                clamp_scroll(state);
            }
        }
        Key::Named(NamedKey::ArrowDown) => {
            state.keybindings_panel.just_saved = false;
            if state.keybindings_panel.cursor + 1 < n {
                state.keybindings_panel.cursor += 1;
                clamp_scroll(state);
            }
        }
        Key::Named(NamedKey::Enter) => {
            // Start recording a new binding for the highlighted action.
            state.keybindings_panel.recording = true;
            state.keybindings_panel.just_saved = false;
        }
        Key::Named(NamedKey::Backspace) | Key::Named(NamedKey::Delete) => {
            // Remove the binding for the highlighted action.
            let action_idx = state.keybindings_panel.cursor;
            if let Some((action_id, _)) = BINDABLE_ACTIONS.get(action_idx) {
                let before = state.user_config.keybindings.len();
                state
                    .user_config
                    .keybindings
                    .retain(|b| !b.action.eq_ignore_ascii_case(action_id));
                if state.user_config.keybindings.len() < before {
                    save_config(&state.user_config);
                    state.keybindings_panel.just_saved = true;
                }
            }
        }
        _ => {}
    }
    true
}
