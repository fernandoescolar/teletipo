use crate::config::{SETTINGS_FIELDS, save_config};
use crate::consts::SEARCH_MAX_VISIBLE;
use crate::runtime::GpuRuntimeState;
use render_glow::{SettingsItem, SettingsOverlay};
use std::path::PathBuf;
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
        ("terminal", "opacity") => Some(0.05),
        _ => None,
    }
}

fn is_bool_field(section: &str, key: &str) -> bool {
    matches!(
        (section, key),
        ("terminal", "bell") | ("terminal", "restore_session")
    )
}

#[derive(Clone)]
pub(crate) struct ShellOption {
    pub(crate) label: String,
    pub(crate) command: Option<String>,
}

fn shell_option(label: impl Into<String>, command: impl Into<String>) -> ShellOption {
    ShellOption {
        label: label.into(),
        command: Some(command.into()),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn shell_option_label_from_path(path: &str) -> String {
    PathBuf::from(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_owned()
}

pub(crate) fn shell_options() -> Vec<ShellOption> {
    let mut out = vec![ShellOption {
        label: "(auto)".to_owned(),
        command: None,
    }];

    #[cfg(target_os = "windows")]
    {
        out.extend([
            shell_option("PowerShell", "powershell.exe"),
            shell_option("Command Prompt (cmd)", "cmd.exe"),
            shell_option("WSL", "wsl.exe"),
        ]);

        let git_bash_candidates = [
            std::env::var("ProgramFiles")
                .ok()
                .map(|p| PathBuf::from(p).join("Git").join("bin").join("bash.exe")),
            std::env::var("ProgramFiles(x86)")
                .ok()
                .map(|p| PathBuf::from(p).join("Git").join("bin").join("bash.exe")),
            std::env::var("LocalAppData").ok().map(|p| {
                PathBuf::from(p)
                    .join("Programs")
                    .join("Git")
                    .join("bin")
                    .join("bash.exe")
            }),
        ];
        let git_bash = git_bash_candidates
            .into_iter()
            .flatten()
            .find(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Git\bin\bash.exe"));
        out.push(ShellOption {
            label: "Git Bash".to_owned(),
            command: Some(git_bash.to_string_lossy().into_owned()),
        });
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let mut candidates: Vec<ShellOption> = Vec::new();

        if let Ok(shell) = std::env::var("SHELL")
            && !shell.trim().is_empty()
        {
            let shell = shell.trim().to_owned();
            candidates.push(shell_option(shell_option_label_from_path(&shell), shell));
        }

        let common_shells = [
            ("zsh", "/bin/zsh"),
            ("bash", "/bin/bash"),
            ("sh", "/bin/sh"),
            ("fish", "/usr/bin/fish"),
            ("fish", "/usr/local/bin/fish"),
            ("fish", "/opt/homebrew/bin/fish"),
        ];
        for (label, path) in common_shells {
            let path = PathBuf::from(path);
            if path.exists() {
                let command = path.to_string_lossy().into_owned();
                if candidates
                    .iter()
                    .all(|opt| opt.command.as_deref() != Some(command.as_str()))
                {
                    candidates.push(shell_option(label, command));
                }
            }
        }

        candidates.sort_by_key(|item| item.label.to_lowercase());
        out.extend(candidates);
    }

    out
}

pub(crate) fn shell_label_for_command(shell: Option<&str>) -> String {
    let Some(shell) = shell else {
        return "(auto)".to_owned();
    };
    let normalized = shell.trim().to_ascii_lowercase();
    for opt in shell_options() {
        if let Some(cmd) = opt.command
            && cmd.eq_ignore_ascii_case(&normalized)
        {
            return opt.label;
        }
    }
    shell.to_owned()
}

pub(crate) fn apply_shell_choice(state: &mut GpuRuntimeState, command: Option<&str>) {
    let selected = command
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);
    state.user_config.terminal.shell = selected.clone();
    state.shell = selected.unwrap_or_else(platform_abstraction::default_shell);
    state.settings.dirty = true;
}

fn build_field_items(state: &GpuRuntimeState) -> Vec<SettingsItem> {
    let mut items: Vec<SettingsItem> = Vec::new();
    items.push(SettingsItem {
        is_header: true,
        is_selectable: false,
        is_searchable: false,
        is_action: false,
        key: format!("[app] version: v{}", env!("CARGO_PKG_VERSION")),
        value: String::new(),
    });
    if let Some(ref err) = state.config_error {
        items.push(SettingsItem {
            is_header: true,
            is_selectable: false,
            is_searchable: false,
            is_action: false,
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
                is_action: false,
                key: format!("[{}]", field.section),
                value: String::new(),
            });
        }
        let is_searchable = (field.section == "font" && field.key == "family")
            || field.key == "theme"
            || (field.section == "terminal" && field.key == "shell");
        let is_selectable = is_searchable
            || numeric_step(field.section, field.key).is_some()
            || is_bool_field(field.section, field.key);
        let value = if field.section == "font" && field.key == "family" {
            state
                .themes_fonts
                .available_fonts
                .get(state.themes_fonts.active_font_idx)
                .map(|f| f.family.clone())
                .unwrap_or_else(|| "(default)".to_owned())
        } else if field.section == "terminal" && field.key == "shell" {
            shell_label_for_command(state.user_config.terminal.shell.as_deref())
        } else {
            state.user_config.get_field(field.section, field.key)
        };
        items.push(SettingsItem {
            is_header: false,
            is_selectable,
            is_searchable,
            is_action: false,
            key: field.key.to_owned(),
            value,
        });
    }
    items.push(SettingsItem {
        is_header: true,
        is_selectable: false,
        is_searchable: false,
        is_action: false,
        key: "[actions]".to_owned(),
        value: String::new(),
    });
    items.push(SettingsItem {
        is_header: false,
        is_selectable: true,
        is_searchable: false,
        is_action: true,
        key: "Open Config in Editor".to_owned(),
        value: String::new(),
    });
    items.push(SettingsItem {
        is_header: false,
        is_selectable: true,
        is_searchable: false,
        is_action: true,
        key: "Reveal Config in Finder".to_owned(),
        value: String::new(),
    });
    items.push(SettingsItem {
        is_header: false,
        is_selectable: true,
        is_searchable: false,
        is_action: true,
        key: "Open Keybindings".to_owned(),
        value: String::new(),
    });
    items
}

fn compute_search_matches(state: &GpuRuntimeState) -> (Vec<String>, usize, usize) {
    let Some(ref buf) = state.settings.search_buf else {
        return (vec![], 0, 0);
    };
    if state.settings.cursor >= SETTINGS_FIELDS.len() {
        return (vec![], 0, 0);
    }
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
    } else if field.section == "terminal" && field.key == "shell" {
        shell_options()
            .into_iter()
            .filter(|s| s.label.to_lowercase().contains(&q))
            .map(|s| s.label)
            .collect()
    } else {
        vec![]
    };
    (
        matches,
        state.settings.search_selected,
        state.settings.search_scroll_offset,
    )
}

/// Build a `SettingsOverlay` snapshot from the current `GpuRuntimeState`.
/// Returns `None` when the settings panel is closed.
pub(crate) fn build_settings_overlay(state: &GpuRuntimeState) -> Option<SettingsOverlay> {
    if !state.settings.open {
        return None;
    }
    let items = build_field_items(state);
    let (search_matches, search_selected, search_scroll_offset) = compute_search_matches(state);
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
pub(crate) fn handle_settings_key(
    state: &mut GpuRuntimeState,
    key_event: &winit::event::KeyEvent,
) -> bool {
    if key_event.state != ElementState::Pressed {
        return true; // consume non-press events too while settings is open
    }

    // ── Search mode: type-to-filter for the font family picker ───────────────
    if state.settings.search_buf.is_some() {
        handle_settings_search_key(state, key_event);
        return true;
    }

    // ── Normal settings mode ──────────────────────────────────────────────────
    handle_settings_normal_key(state, key_event);
    true // all keys consumed while settings is open
}

/// Handle a key event while the settings search dropdown is active.
fn handle_settings_search_key(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) {
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
}

/// Handle a key event in the normal (non-search) settings mode.
fn handle_settings_normal_key(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) {
    // +3 for the three action rows at the end
    let n_fields = SETTINGS_FIELDS.len() + 3;
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
                state.close_active_modal();
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
            handle_settings_arrow_lr(state, key_event);
        }
        Key::Named(NamedKey::Enter) => {
            handle_settings_enter(state);
        }
        Key::Named(NamedKey::Backspace) => {
            if let Some(ref mut buf) = state.settings.edit_buf {
                buf.pop();
            }
        }
        Key::Character(ch) if state.modifiers.super_down && ch.as_str() == "s" => {
            handle_settings_save(state);
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
}

/// Handle left/right arrow in normal settings mode (cycle field values).
fn handle_settings_arrow_lr(state: &mut GpuRuntimeState, key_event: &winit::event::KeyEvent) {
    let is_right = matches!(&key_event.logical_key, Key::Named(NamedKey::ArrowRight));
    if state.settings.cursor >= SETTINGS_FIELDS.len() {
        return; // action rows don't respond to arrow left/right
    }
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
    } else if field.section == "terminal" && field.key == "shell" {
        let options = shell_options();
        if !options.is_empty() {
            let cur = options
                .iter()
                .position(|opt| {
                    shell_label_for_command(state.user_config.terminal.shell.as_deref())
                        == opt.label
                })
                .unwrap_or(0);
            let next = if is_right {
                (cur + 1) % options.len()
            } else if cur == 0 {
                options.len() - 1
            } else {
                cur - 1
            };
            apply_shell_choice(state, options[next].command.as_deref());
        }
    } else if is_bool_field(field.section, field.key) {
        let raw = state.user_config.get_field(field.section, field.key);
        let current = matches!(raw.to_lowercase().as_str(), "on" | "true" | "1");
        let new_val = if current { "off" } else { "on" };
        if state
            .user_config
            .set_field(field.section, field.key, new_val)
        {
            state.settings.dirty = true;
        }
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

/// Handle Enter in normal settings mode (activate, confirm, or open edit).
fn handle_settings_enter(state: &mut GpuRuntimeState) {
    let idx = state.settings.cursor;
    // Handle action rows (beyond SETTINGS_FIELDS).
    let action_base = SETTINGS_FIELDS.len();
    if idx == action_base {
        crate::commands::execute_ui_command(
            state,
            crate::commands::CommandId::OpenConfigInEditor,
            crate::commands::CommandContext::default(),
        );
        return;
    } else if idx == action_base + 1 {
        crate::commands::execute_ui_command(
            state,
            crate::commands::CommandId::RevealConfigInFinder,
            crate::commands::CommandContext::default(),
        );
        return;
    } else if idx == action_base + 2 {
        // Close settings first, then open keybindings.
        state.close_settings_modal();
        crate::commands::execute_ui_command(
            state,
            crate::commands::CommandId::OpenKeybindings,
            crate::commands::CommandContext::default(),
        );
        return;
    }
    let field = &SETTINGS_FIELDS[idx];
    let is_searchable = (field.section == "font" && field.key == "family")
        || field.key == "theme"
        || (field.section == "terminal" && field.key == "shell");
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

/// Handle Cmd+S in normal settings mode (flush edit buffer and save).
fn handle_settings_save(state: &mut GpuRuntimeState) {
    if let Some(buf) = state.settings.edit_buf.take()
        && state.settings.cursor < SETTINGS_FIELDS.len()
    {
        let field = &SETTINGS_FIELDS[state.settings.cursor];
        let is_selectable = field.key == "theme"
            || (field.section == "font" && field.key == "family")
            || (field.section == "terminal" && field.key == "shell")
            || numeric_step(field.section, field.key).is_some()
            || is_bool_field(field.section, field.key);
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
    state.close_active_modal();
}

// ── Search helpers ────────────────────────────────────────────────────────────

/// Count the number of items matching the current search buffer for the focused field.
fn search_match_count(state: &GpuRuntimeState) -> usize {
    if state.settings.cursor >= SETTINGS_FIELDS.len() {
        return 0;
    }
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
    } else if field.section == "terminal" && field.key == "shell" {
        shell_options()
            .into_iter()
            .filter(|s| s.label.to_lowercase().contains(&buf))
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
    if state.settings.cursor >= SETTINGS_FIELDS.len() {
        return;
    }
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
    } else if field.section == "terminal" && field.key == "shell" {
        let matches: Vec<ShellOption> = shell_options()
            .into_iter()
            .filter(|s| s.label.to_lowercase().contains(&buf))
            .collect();
        if let Some(shell) = matches.get(state.settings.search_selected) {
            apply_shell_choice(state, shell.command.as_deref());
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
