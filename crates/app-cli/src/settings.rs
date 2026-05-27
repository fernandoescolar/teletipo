use crate::config::{save_config, SETTINGS_FIELDS};
use crate::GpuRuntimeState;
use render_wgpu::{SettingsItem, SettingsOverlay};
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

/// Build a `SettingsOverlay` snapshot from the current `GpuRuntimeState`.
/// Returns `None` when the settings panel is closed.
pub(crate) fn build_settings_overlay(state: &GpuRuntimeState) -> Option<SettingsOverlay> {
    if !state.settings.open {
        return None;
    }
    let mut items: Vec<SettingsItem> = Vec::new();
    let mut last_section = "";
    for field in SETTINGS_FIELDS {
        if field.section != last_section {
            last_section = field.section;
            items.push(SettingsItem {
                is_header: true,
                is_selectable: false,
                key: format!("[{}]", field.section),
                value: String::new(),
            });
        }
        let is_select = field.key == "theme"
            || (field.section == "font" && field.key == "path");
        let value = if field.section == "font" && field.key == "path" {
            state.available_fonts
                .get(state.active_font_idx)
                .map(|f| f.name.clone())
                .unwrap_or_else(|| "(default)".to_owned())
        } else {
            state.user_config.get_field(field.section, field.key)
        };
        items.push(SettingsItem {
            is_header: false,
            is_selectable: is_select,
            key: field.key.to_owned(),
            value,
        });
    }
    Some(SettingsOverlay {
        items,
        cursor: state.settings.cursor,
        editing: state.settings.edit_buf.clone(),
        just_saved: state.settings.just_saved,
    })
}

/// Handle a key event while the settings overlay is open.
/// Returns `true` if the event was consumed (caller should `return`).
/// Must only be called when `state.settings.open` is true.
pub(crate) fn handle_settings_key(
    state: &mut GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
) -> bool {
    if key_event.state != ElementState::Pressed {
        return true; // consume non-press events too while settings is open
    }
    let n_fields = SETTINGS_FIELDS.len();
    match &key_event.logical_key {
        Key::Named(NamedKey::Escape) => {
            if state.settings.edit_buf.is_some() {
                state.settings.edit_buf = None;
            } else {
                state.settings.open = false;
            }
        }
        Key::Named(NamedKey::ArrowUp) => {
            state.settings.cursor = state.settings.cursor.saturating_sub(1);
            state.settings.edit_buf = None;
        }
        Key::Named(NamedKey::ArrowDown) => {
            if state.settings.cursor + 1 < n_fields {
                state.settings.cursor += 1;
            }
            state.settings.edit_buf = None;
        }
        Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowRight) => {
            let is_right = matches!(&key_event.logical_key, Key::Named(NamedKey::ArrowRight));
            let field = &SETTINGS_FIELDS[state.settings.cursor];
            if field.key == "theme" && !state.available_themes.is_empty() {
                let n = state.available_themes.len();
                let cur = state.active_theme_idx.unwrap_or(0);
                let next = if is_right {
                    (cur + 1) % n
                } else if cur == 0 {
                    n - 1
                } else {
                    cur - 1
                };
                state.active_theme_idx = Some(next);
                let tf = state.available_themes[next].clone();
                apply_theme_file(&mut state.user_config, &tf);
                state.settings.dirty = true;
            } else if field.section == "font" && field.key == "path"
                && !state.available_fonts.is_empty()
            {
                let n = state.available_fonts.len();
                let cur = state.active_font_idx;
                let next = if is_right {
                    (cur + 1) % n
                } else if cur == 0 {
                    n - 1
                } else {
                    cur - 1
                };
                state.active_font_idx = next;
                state.user_config.font.path = if next == 0 {
                    None
                } else {
                    Some(state.available_fonts[next].path.clone())
                };
                state.settings.dirty = true;
            }
        }
        Key::Named(NamedKey::Enter) => {
            let idx = state.settings.cursor;
            let field = &SETTINGS_FIELDS[idx];
            let is_select = field.key == "theme"
                || (field.section == "font" && field.key == "path");
            if is_select {
                // no-op — cycle with ← →
            } else if let Some(buf) = state.settings.edit_buf.take() {
                if state.user_config.set_field(field.section, field.key, &buf) {
                    state.settings.dirty = true;
                }
            } else {
                let current = state.user_config.get_field(field.section, field.key);
                state.settings.edit_buf = Some(current);
            }
        }
        Key::Named(NamedKey::Backspace) => {
            if let Some(ref mut buf) = state.settings.edit_buf {
                buf.pop();
            }
        }
        Key::Character(ch) if state.super_down => {
            if ch.as_str() == "s" {
                if let Some(buf) = state.settings.edit_buf.take() {
                    let field = &SETTINGS_FIELDS[state.settings.cursor];
                    let is_select = field.key == "theme"
                        || (field.section == "font" && field.key == "path");
                    if !is_select {
                        if state.user_config.set_field(field.section, field.key, &buf) {
                            state.settings.dirty = true;
                        }
                    }
                }
                save_config(&state.user_config);
                state.settings.dirty = false;
                state.settings.just_saved = true;
                state.settings.open = false;
            }
        }
        Key::Character(_) | Key::Named(NamedKey::Space) => {
            if let Some(ref mut buf) = state.settings.edit_buf {
                if let Some(text) = key_event.text.as_ref() {
                    buf.push_str(text.as_str());
                }
            }
        }
        _ => {}
    }
    true // all keys consumed while settings is open
}

/// Record the active theme by name in the persisted config.
pub(crate) fn apply_theme_file(cfg: &mut crate::config::UserConfig, tf: &crate::theme::ThemeFile) {
    cfg.active_theme = Some(tf.name.clone());
}
