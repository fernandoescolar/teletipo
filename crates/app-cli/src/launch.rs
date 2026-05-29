use crate::GpuRuntimeState;
use crate::config::{UserConfig, load_config_result};
use crate::tab::{HistoryEntry, PersistentSession, TabSession, TabState};
use crate::theme;
use app_orchestrator::App;
use fontdb::Database;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use terminal_pty::PortablePtySession;

pub(crate) const TERMINAL_ROWS_MIN: usize = 1;
pub(crate) const TERMINAL_ROWS_MAX: usize = 1024;
pub(crate) const TERMINAL_COLS_MIN: usize = 1;
pub(crate) const TERMINAL_COLS_MAX: usize = 4096;

// ── Font discovery ────────────────────────────────────────────────────────────

/// A font family discovered on the machine.
pub(crate) struct FontEntry {
    /// Display family name (e.g. "Hack", "Consolas", "DejaVu Sans Mono").
    pub(crate) family: String,
}

/// Enumerate installed font family names. The first entry is always a
/// synthetic "(default)" item so that index 0 means "no override".
pub(crate) fn enumerate_font_families() -> Vec<FontEntry> {
    let mut db = Database::new();
    db.load_system_fonts();

    let mut families: BTreeSet<String> = BTreeSet::new();
    for face in db.faces() {
        for (name, _) in &face.families {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                families.insert(trimmed.to_owned());
            }
        }
    }

    let mut fonts: Vec<FontEntry> = families
        .into_iter()
        .map(|family| FontEntry { family })
        .collect();
    fonts.insert(
        0,
        FontEntry {
            family: "(default)".to_owned(),
        },
    );
    fonts
}

// ── PTY spawning ──────────────────────────────────────────────────────────────

pub(crate) fn spawn_pty(
    shell: &str,
    rows: u16,
    cols: u16,
    exec: Option<&str>,
    cwd: Option<&str>,
) -> anyhow::Result<(PortablePtySession, bool)> {
    match exec {
        Some(cmd) => {
            // --exec mode: no shell integration (single-shot command).
            #[cfg(target_os = "windows")]
            {
                let pty = PortablePtySession::spawn_command(
                    "powershell.exe",
                    &["-NoProfile", "-Command", cmd],
                    rows,
                    cols,
                    cwd,
                )?;
                Ok((pty, false))
            }
            #[cfg(not(target_os = "windows"))]
            {
                let pty = PortablePtySession::spawn_command(shell, &["-lc", cmd], rows, cols, cwd)?;
                Ok((pty, false))
            }
        }
        None => Ok(PortablePtySession::spawn_shell(shell, rows, cols, cwd)?),
    }
}

// ── Session persistence ───────────────────────────────────────────────────────

pub(crate) fn session_path() -> Option<PathBuf> {
    let dir = dirs::data_local_dir()?.join("teletipo");
    if let Err(err) = fs::create_dir_all(&dir) {
        tracing::warn!(path = %dir.display(), error = %err, "failed to create session directory");
        return None;
    }
    Some(dir.join("session.json"))
}

pub(crate) fn load_session() -> PersistentSession {
    let path = match session_path() {
        Some(p) => p,
        None => return PersistentSession::default(),
    };
    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to read session file",
                );
            }
            return PersistentSession::default();
        }
    };
    match serde_json::from_str(&data) {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "failed to parse session file; ignoring saved session",
            );
            PersistentSession::default()
        }
    }
}

pub(crate) fn save_session(state: &GpuRuntimeState) {
    let Some(path) = session_path() else { return };

    fn trim_output(s: &str) -> String {
        let t: String = s
            .lines()
            .map(|l| l.trim_end())
            .collect::<Vec<_>>()
            .join("\n");
        t.trim_end().to_string()
    }

    let tab_sessions: Vec<TabSession> = state
        .tabs
        .iter()
        .map(|tab| TabSession {
            terminal_output: trim_output(&tab.app.terminal_ansi_snapshot()),
            history: tab.history.clone(),
            split_ratio: tab.split_ratio,
            cwd: tab.cwd.clone(),
            history_entries: tab.history_entries.clone(),
        })
        .collect();

    let active = &state.tabs[state.active_tab];
    // Store logical pixels (physical ÷ scale_factor) so window.rs can restore the
    // size correctly with LogicalSize::new(…) on the next launch.
    let logical_w = (state.layout.window_width as f64 / state.layout.scale_factor).round() as u32;
    let logical_h = (state.layout.window_height as f64 / state.layout.scale_factor).round() as u32;
    let session = PersistentSession {
        window_width: logical_w,
        window_height: logical_h,
        window_x: Some(state.layout.window_x),
        window_y: Some(state.layout.window_y),
        tabs: tab_sessions,
        split_ratio: active.split_ratio,
        history: active.history.clone(),
        terminal_output: trim_output(&active.app.terminal_ansi_snapshot()),
    };
    if let Ok(json) = serde_json::to_string_pretty(&session) {
        if let Err(err) = fs::write(&path, json) {
            tracing::warn!(path = %path.display(), error = %err, "failed to save session file");
        }
    }
}

pub(crate) fn build_initial_state(
    rows: usize,
    cols: usize,
    exec: Option<&str>,
    shell: &str,
    session: PersistentSession,
    update_rx: std::sync::mpsc::Receiver<Result<Option<String>, String>>,
) -> anyhow::Result<GpuRuntimeState> {
    let (rows, cols) = sanitize_terminal_size(rows, cols);
    let window_width = session.window_width;
    let window_height = session.window_height;
    let window_x = session.window_x.unwrap_or(0);
    let window_y = session.window_y.unwrap_or(0);

    let saved_tabs: Vec<TabSession> = if !session.tabs.is_empty() {
        session.tabs
    } else {
        vec![TabSession {
            terminal_output: session.terminal_output,
            history: session.history,
            split_ratio: session.split_ratio,
            cwd: String::new(),
            history_entries: vec![],
        }]
    };

    let mut tabs: Vec<TabState> = Vec::new();
    for (i, saved) in saved_tabs.into_iter().enumerate() {
        let mut app = build_app(rows, cols)?;
        for line in saved.terminal_output.lines() {
            app.feed_terminal(line.as_bytes());
            app.feed_terminal(b"\r\n");
        }
        let initial_cwd: String = if !saved.cwd.is_empty() {
            saved.cwd.clone()
        } else {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let restore_cwd = if initial_cwd.is_empty() {
            None
        } else {
            Some(initial_cwd.as_str())
        };
        let (pty, integration) = if i == 0 {
            match spawn_pty(shell, rows as u16, cols as u16, exec, restore_cwd) {
                Ok((p, integ)) => (Some(p), integ),
                Err(err) => {
                    app.feed_terminal(format!("PTY unavailable: {err}\n").as_bytes());
                    (None, false)
                }
            }
        } else {
            spawn_pty(shell, rows as u16, cols as u16, None, restore_cwd)
                .map(|(p, integ)| (Some(p), integ))
                .unwrap_or((None, false))
        };
        tabs.push(TabState {
            app,
            pty,
            scroll_offset: 0,
            editor_scroll_offset: 0,
            history: saved.history.clone(),
            history_index: None,
            saved_input: String::new(),
            split_ratio: saved.split_ratio,
            was_terminal_fullscreen: false,
            pre_fullscreen_split_ratio: saved.split_ratio,
            selection_anchor: None,
            selection_anchor_scroll: 0,
            selection_end: None,
            selection_end_scroll: 0,
            is_selecting: false,
            is_selecting_editor: false,
            last_terminal_text: String::new(),
            term_row_count: rows,
            cwd: initial_cwd,
            suggestion_prefix: None,
            suggestion_index: None,
            // Backward compat: if no frecency data is stored, seed each history
            // entry with count=1 and an artificial age so older entries rank lower.
            history_entries: if saved.history_entries.is_empty() {
                saved
                    .history
                    .iter()
                    .enumerate()
                    .map(|(i, cmd)| HistoryEntry {
                        cmd: cmd.clone(),
                        count: 1,
                        last_used_secs: i as u64,
                    })
                    .collect()
            } else {
                saved.history_entries.clone()
            },
            pending_cmd: None,
            shell_integration: integration,
        });
    }

    let (user_config, config_error) = match load_config_result() {
        Ok(cfg) => (cfg, None),
        Err(err) => {
            tracing::warn!(error = %err, "failed to load config; using defaults");
            (UserConfig::default(), Some(err.to_string()))
        }
    };

    let mut state = GpuRuntimeState {
        tabs,
        active_tab: 0,
        shell: shell.to_owned(),
        modifiers: crate::ModifierState::default(),
        layout: crate::LayoutState {
            window_width,
            window_height,
            window_x,
            window_y,
            scale_factor: 1.0,
            cell_w: 8.4,
            cell_h: 16.8,
        },
        cursor: crate::CursorState::default(),
        drag: crate::DragState::default(),
        overlays: crate::OverlayState::default(),
        user_config,
        config_error,
        themes_fonts: crate::ThemeFontState {
            available_themes: {
                theme::install_default_themes();
                theme::load_themes()
            },
            active_theme_idx: None,
            available_fonts: enumerate_font_families(),
            active_font_idx: 0,
        },
        update_rx: Some(update_rx),
        settings: crate::SettingsUiState::default(),
        should_exit: false,
        shell_services: Box::new(crate::shell::SystemShell::new()),
    };

    state.themes_fonts.active_theme_idx =
        state.user_config.active_theme.as_ref().and_then(|name| {
            state
                .themes_fonts
                .available_themes
                .iter()
                .position(|t| &t.name == name)
        });
    state.themes_fonts.active_font_idx = state
        .user_config
        .font
        .family
        .as_ref()
        .and_then(|family| {
            state
                .themes_fonts
                .available_fonts
                .iter()
                .position(|f| &f.family == family)
        })
        .unwrap_or(0);

    Ok(state)
}

pub(crate) fn sanitize_terminal_size(rows: usize, cols: usize) -> (usize, usize) {
    let safe_rows = rows.clamp(TERMINAL_ROWS_MIN, TERMINAL_ROWS_MAX);
    let safe_cols = cols.clamp(TERMINAL_COLS_MIN, TERMINAL_COLS_MAX);
    if safe_rows != rows || safe_cols != cols {
        tracing::warn!(
            requested_rows = rows,
            requested_cols = cols,
            safe_rows,
            safe_cols,
            "terminal size out of bounds; clamped to safe range"
        );
    }
    (safe_rows, safe_cols)
}

pub(crate) fn build_app(rows: usize, cols: usize) -> anyhow::Result<App> {
    let (safe_rows, safe_cols) = sanitize_terminal_size(rows, cols);
    App::new(safe_rows, safe_cols).map_err(|err| {
        anyhow::anyhow!(
            "failed to initialize app with terminal size {}x{}: {}",
            safe_rows,
            safe_cols,
            err
        )
    })
}

// ── Context menu ──────────────────────────────────────────────────────────────

/// Execute the selected item from the tab context menu.
/// `tab_idx` is the tab the menu was opened for; `item` is 0-3.
pub(crate) fn execute_context_menu_item(state: &mut GpuRuntimeState, tab_idx: usize, item: usize) {
    match item {
        0 => state.add_new_tab(),
        1 => state.close_tab(tab_idx),
        2 => state.move_tab_to(tab_idx, tab_idx.saturating_sub(1)),
        3 => state.move_tab_to(tab_idx, tab_idx + 2),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_terminal_size_keeps_in_range_values() {
        assert_eq!(sanitize_terminal_size(24, 80), (24, 80));
    }

    #[test]
    fn sanitize_terminal_size_clamps_low_values() {
        assert_eq!(sanitize_terminal_size(0, 0), (TERMINAL_ROWS_MIN, TERMINAL_COLS_MIN));
    }

    #[test]
    fn sanitize_terminal_size_clamps_high_values() {
        assert_eq!(
            sanitize_terminal_size(usize::MAX, usize::MAX),
            (TERMINAL_ROWS_MAX, TERMINAL_COLS_MAX)
        );
    }
}
