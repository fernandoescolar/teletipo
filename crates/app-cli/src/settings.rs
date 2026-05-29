use crate::config::{SETTINGS_FIELDS, save_config};
use crate::consts::SEARCH_MAX_VISIBLE;
use crate::runtime::GpuRuntimeState;
use render_wgpu::{SettingsItem, SettingsOverlay};
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

#[derive(Debug, Default)]
pub(crate) struct SettingsUiState {
    pub(crate) open: bool,
    pub(crate) cursor: usize,
    pub(crate) edit_buf: Option<String>,
    pub(crate) dirty: bool,
    pub(crate) just_saved: bool,
    /// When `Some`, the focused searchable field is in type-to-filter mode.
    pub(crate) search_buf: Option<String>,
    /// Highlighted index within the current `search_matches` list.
    pub(crate) search_selected: usize,
    /// First visible index in the search dropdown (scroll offset).
    pub(crate) search_scroll_offset: usize,
}

/// Returns the increment step for numeric fields, or `None` if the field is
/// not numeric.
fn numeric_step(section: &str, key: &str) -> Option<f32> {
    match (section, key) {
        ("font", "size") => Some(0.5),
        ("padding", "horizontal") | ("padding", "vertical") => Some(1.0),
        ("terminal", "scrollback_lines") => Some(500.0),
        _ => None,
    }
}

/// Build a `SettingsOverlay` snapshot from the current `GpuRuntimeState`.
/// Returns `None` when the settings panel is closed.
pub(crate) fn build_settings_overlay(state: &GpuRuntimeState) -> Option<SettingsOverlay> {
    if !state.settings.open {
        return None;
    }
    let mut items: Vec<SettingsItem> = Vec::new();
    if let Some(ref err) = state.config_error {
        items.push(SettingsItem {
            is_header: true,
            is_selectable: false,
            is_searchable: false,
            key: format!("[startup] config error: {err}"),
            value: String::new(),
        });
    }
    let mut last_section = "";
    for field in SETTINGS_FIELDS {
        if field.section != last_section {
            last_section = field.section;
            items.push(SettingsItem {
                is_header: true,
                is_selectable: false,
                is_searchable: false,
                key: format!("[{}]", field.section),
                value: String::new(),
            });
        }
        // Font family and theme: searchable selectors (Enter = search mode, ← → = cycle).
        // Numeric fields: selectable so they display "← N →" and ← → increments.
        // Everything else: free-text via Enter.
        let is_searchable =
            (field.section == "font" && field.key == "family") || field.key == "theme";
        let is_selectable = is_searchable || numeric_step(field.section, field.key).is_some();
        let value = if field.section == "font" && field.key == "family" {
            state
                .themes_fonts
                .available_fonts
                .get(state.themes_fonts.active_font_idx)
                .map(|f| f.family.clone())
                .unwrap_or_else(|| "(default)".to_owned())
        } else {
            state.user_config.get_field(field.section, field.key)
        };
        items.push(SettingsItem {
            is_header: false,
            is_selectable,
            is_searchable,
            key: field.key.to_owned(),
            value,
        });
    }

    // Compute search matches when in search mode (font family or theme).
    let (search_matches, search_selected, search_scroll_offset) =
        if let Some(ref buf) = state.settings.search_buf {
            let q = buf.to_lowercase();
            let field = &SETTINGS_FIELDS[state.settings.cursor];
            let matches: Vec<String> = if field.section == "font" && field.key == "family" {
                state
                    .themes_fonts
                    .available_fonts
                    .iter()
                    .filter(|f| f.family.to_lowercase().contains(&q))
                    .map(|f| f.family.clone())
                    .collect()
            } else if field.key == "theme" {
                state
                    .themes_fonts
                    .available_themes
                    .iter()
                    .filter(|t| t.name.to_lowercase().contains(&q))
                    .map(|t| t.name.clone())
                    .collect()
            } else {
                vec![]
            };
            (
                matches,
                state.settings.search_selected,
                state.settings.search_scroll_offset,
            )
        } else {
            (vec![], 0, 0)
        };

    Some(SettingsOverlay {
        items,
        cursor: state.settings.cursor,
        editing: state.settings.edit_buf.clone(),
        just_saved: state.settings.just_saved,
        search_buf: state.settings.search_buf.clone(),
        search_matches,
        search_selected,
        search_scroll_offset,
    })
}

/// Handle a key event while the settings overlay is open.
/// Returns `true` if the event was consumed (caller should `return`).
/// Must only be called when `state.settings.open` is true.
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // overlay dispatcher: flat match on every key
pub(crate) fn handle_settings_key(
    state: &mut GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
) -> bool {
    if key_event.state != ElementState::Pressed {
        return true; // consume non-press events too while settings is open
    }
    let n_fields = SETTINGS_FIELDS.len();

    // ── Search mode: type-to-filter for the font family picker ───────────────
    if state.settings.search_buf.is_some() {
        match &key_event.logical_key {
            Key::Named(NamedKey::Escape) => {
                // Cancel search — restore the original family selection.
                state.settings.search_buf = None;
                state.settings.search_selected = 0;
                state.settings.search_scroll_offset = 0;
            }
            Key::Named(NamedKey::Enter) => {
                // Confirm the highlighted match.
                confirm_search_selection(state);
            }
            Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowLeft) => {
                // Compute current matches count inline (cheap, just filtering).
                let n = search_match_count(state);
                if n > 0 && state.settings.search_selected > 0 {
                    state.settings.search_selected -= 1;
                    clamp_search_scroll(state, n);
                }
            }
            Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowRight) => {
                let n = search_match_count(state);
                if n > 0 && state.settings.search_selected + 1 < n {
                    state.settings.search_selected += 1;
                    clamp_search_scroll(state, n);
                }
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(ref mut buf) = state.settings.search_buf {
                    buf.pop();
                    state.settings.search_selected = 0;
                    state.settings.search_scroll_offset = 0;
                }
            }
            Key::Character(_) | Key::Named(NamedKey::Space) => {
                if let Some(text) = key_event.text.as_ref()
                    && let Some(ref mut buf) = state.settings.search_buf
                {
                    buf.push_str(text.as_str());
                    state.settings.search_selected = 0;
                    state.settings.search_scroll_offset = 0;
                }
            }
            _ => {}
        }
        return true;
    }

    // ── Normal settings mode ──────────────────────────────────────────────────
    match &key_event.logical_key {
        Key::Named(NamedKey::Escape) => {
            if state.settings.edit_buf.is_some() {
                state.settings.edit_buf = None;
            } else {
                // Autosave on close.
                if state.settings.dirty {
                    save_config(&state.user_config);
                    state.settings.dirty = false;
                    state.settings.just_saved = true;
                }
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
            if field.key == "theme" && !state.themes_fonts.available_themes.is_empty() {
                let n = state.themes_fonts.available_themes.len();
                let cur = state.themes_fonts.active_theme_idx.unwrap_or(0);
                let next = if is_right {
                    (cur + 1) % n
                } else if cur == 0 {
                    n - 1
                } else {
                    cur - 1
                };
                state.themes_fonts.active_theme_idx = Some(next);
                let tf = state.themes_fonts.available_themes[next].clone();
                apply_theme_file(&mut state.user_config, &tf);
                state.settings.dirty = true;
            } else if field.section == "font"
                && field.key == "family"
                && !state.themes_fonts.available_fonts.is_empty()
            {
                let n = state.themes_fonts.available_fonts.len();
                let cur = state.themes_fonts.active_font_idx;
                let next = if is_right {
                    (cur + 1) % n
                } else if cur == 0 {
                    n - 1
                } else {
                    cur - 1
                };
                state.themes_fonts.active_font_idx = next;
                state.user_config.font.family = if next == 0 {
                    None
                } else {
                    Some(state.themes_fonts.available_fonts[next].family.clone())
                };
                state.settings.dirty = true;
            } else if let Some(step) = numeric_step(field.section, field.key) {
                // Numeric field: increment / decrement by step.
                let raw = state.user_config.get_field(field.section, field.key);
                let current: f32 = raw.parse().unwrap_or(0.0);
                let delta = if is_right { step } else { -step };
                let new_val = if step.fract() == 0.0 {
                    // Integer step (padding, scrollback_lines): write without decimal point.
                    format!("{}", (current + delta).max(0.0) as u32)
                } else {
                    // Fractional step (font size): keep one decimal place.
                    format!("{:.1}", (current + delta).max(0.0))
                };
                if state
                    .user_config
                    .set_field(field.section, field.key, &new_val)
                {
                    state.settings.dirty = true;
                }
            }
        }
        Key::Named(NamedKey::Enter) => {
            let idx = state.settings.cursor;
            let field = &SETTINGS_FIELDS[idx];
            let is_searchable =
                (field.section == "font" && field.key == "family") || field.key == "theme";
            if is_searchable {
                // Activate type-to-filter search mode (font family and theme).
                state.settings.search_buf = Some(String::new());
                state.settings.search_selected = 0;
                state.settings.search_scroll_offset = 0;
            } else if let Some(buf) = state.settings.edit_buf.take() {
                // Confirm any open edit buffer (numeric and free-text fields).
                if state.user_config.set_field(field.section, field.key, &buf) {
                    state.settings.dirty = true;
                }
            } else {
                // Open edit mode — clear placeholder text for better UX.
                let current = state.user_config.get_field(field.section, field.key);
                let current = if current == "(auto)" || current == "(default)" {
                    String::new()
                } else {
                    current
                };
                state.settings.edit_buf = Some(current);
            }
        }
        Key::Named(NamedKey::Backspace) => {
            if let Some(ref mut buf) = state.settings.edit_buf {
                buf.pop();
            }
        }
        Key::Character(ch) if state.modifiers.super_down && ch.as_str() == "s" => {
            // Explicit Cmd+S: flush any open edit buffer then save.
            if let Some(buf) = state.settings.edit_buf.take() {
                let field = &SETTINGS_FIELDS[state.settings.cursor];
                let is_selectable = field.key == "theme"
                    || (field.section == "font" && field.key == "family")
                    || numeric_step(field.section, field.key).is_some();
                if !is_selectable && !state.user_config.set_field(field.section, field.key, &buf) {
                    tracing::warn!(
                        section = field.section,
                        key = field.key,
                        value = %buf,
                        "rejected invalid setting value"
                    );
                }
            }
            save_config(&state.user_config);
            state.settings.dirty = false;
            state.settings.just_saved = true;
            state.settings.open = false;
        }
        Key::Character(_) | Key::Named(NamedKey::Space) => {
            if let Some(ref mut buf) = state.settings.edit_buf
                && let Some(text) = key_event.text.as_ref()
            {
                buf.push_str(text.as_str());
            }
        }
        _ => {}
    }
    true // all keys consumed while settings is open
}

// ── Search helpers ────────────────────────────────────────────────────────────

/// Count the number of items matching the current search buffer for the focused field.
fn search_match_count(state: &GpuRuntimeState) -> usize {
    let buf = state
        .settings
        .search_buf
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    let field = &SETTINGS_FIELDS[state.settings.cursor];
    if field.section == "font" && field.key == "family" {
        state
            .themes_fonts
            .available_fonts
            .iter()
            .filter(|f| f.family.to_lowercase().contains(&buf))
            .count()
    } else if field.key == "theme" {
        state
            .themes_fonts
            .available_themes
            .iter()
            .filter(|t| t.name.to_lowercase().contains(&buf))
            .count()
    } else {
        0
    }
}

/// Ensure `search_scroll_offset` keeps `search_selected` in the visible window.
fn clamp_search_scroll(state: &mut GpuRuntimeState, n_matches: usize) {
    let sel = state.settings.search_selected;
    let off = &mut state.settings.search_scroll_offset;
    if sel >= *off + SEARCH_MAX_VISIBLE {
        *off = sel.saturating_sub(SEARCH_MAX_VISIBLE - 1);
    } else if sel < *off {
        *off = sel;
    }
    // Also clamp offset so we don't scroll past the end.
    let max_off = n_matches.saturating_sub(SEARCH_MAX_VISIBLE);
    *off = (*off).min(max_off);
}

/// Confirm the currently highlighted search result for the focused field (font family or theme).
fn confirm_search_selection(state: &mut GpuRuntimeState) {
    let buf = state
        .settings
        .search_buf
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
    let field = &SETTINGS_FIELDS[state.settings.cursor];
    if field.section == "font" && field.key == "family" {
        let matches: Vec<String> = state
            .themes_fonts
            .available_fonts
            .iter()
            .filter(|f| f.family.to_lowercase().contains(&buf))
            .map(|f| f.family.clone())
            .collect();
        if let Some(family) = matches.get(state.settings.search_selected)
            && let Some(idx) = state
                .themes_fonts
                .available_fonts
                .iter()
                .position(|f| &f.family == family)
        {
            state.themes_fonts.active_font_idx = idx;
            state.user_config.font.family = if idx == 0 { None } else { Some(family.clone()) };
            state.settings.dirty = true;
        }
    } else if field.key == "theme" {
        let matches: Vec<String> = state
            .themes_fonts
            .available_themes
            .iter()
            .filter(|t| t.name.to_lowercase().contains(&buf))
            .map(|t| t.name.clone())
            .collect();
        if let Some(theme_name) = matches.get(state.settings.search_selected)
            && let Some(idx) = state
                .themes_fonts
                .available_themes
                .iter()
                .position(|t| &t.name == theme_name)
        {
            state.themes_fonts.active_theme_idx = Some(idx);
            let tf = state.themes_fonts.available_themes[idx].clone();
            apply_theme_file(&mut state.user_config, &tf);
            state.settings.dirty = true;
        }
    }
    state.settings.search_buf = None;
    state.settings.search_selected = 0;
    state.settings.search_scroll_offset = 0;
}

/// Record the active theme by name in the persisted config.
pub(crate) fn apply_theme_file(cfg: &mut crate::config::UserConfig, tf: &crate::theme::ThemeFile) {
    cfg.active_theme = Some(tf.name.clone());
}
