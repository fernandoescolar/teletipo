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
#[derive(Clone)]
pub(crate) struct FontEntry {
    /// Display family name (e.g. "Hack", "Consolas", "DejaVu Sans Mono").
    pub(crate) family: String,
}

/// Enumerate installed font family names. The first entry is always a
/// synthetic "(default)" item so that index 0 means "no override".
#[tracing::instrument]
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

#[tracing::instrument(skip(shell, exec, cwd, waker))]
pub(crate) fn spawn_pty(
    shell: &str,
    rows: u16,
    cols: u16,
    exec: Option<&str>,
    cwd: Option<&str>,
    waker: Option<terminal_pty::Waker>,
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
                    waker,
                )?;
                Ok((pty, false))
            }
            #[cfg(not(target_os = "windows"))]
            {
                let pty = PortablePtySession::spawn_command(
                    shell,
                    &["-lc", cmd],
                    rows,
                    cols,
                    cwd,
                    waker,
                )?;
                Ok((pty, false))
            }
        }
        None => Ok(PortablePtySession::spawn_shell(
            shell, rows, cols, cwd, waker,
        )?),
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

#[tracing::instrument]
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
            terminal_output: trim_output(tab.app.terminal_ansi_snapshot().as_str()),
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
        terminal_output: trim_output(active.app.terminal_ansi_snapshot().as_str()),
    };
    if let Ok(json) = serde_json::to_string_pretty(&session) {
        // PERF-2: hand the write to a detached worker so shutdown isn't blocked
        // on a slow disk; join with a short timeout so failures still surface.
        let handle = std::thread::spawn(move || {
            if let Err(err) = fs::write(&path, json) {
                tracing::warn!(path = %path.display(), error = %err, "failed to save session file");
            }
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        loop {
            if handle.is_finished() {
                if let Err(err) = handle.join() {
                    tracing::warn!(?err, "session writer thread panicked");
                }
                break;
            }
            if std::time::Instant::now() >= deadline {
                tracing::warn!("session writer did not finish within 500ms; detaching");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

/// Pick which tab sessions to restore based on user config.
fn resolve_saved_tabs(session: PersistentSession, restore_session: bool) -> Vec<TabSession> {
    if restore_session {
        if !session.tabs.is_empty() {
            return session.tabs;
        }
        return vec![TabSession {
            terminal_output: session.terminal_output,
            history: session.history,
            split_ratio: session.split_ratio,
            cwd: String::new(),
            history_entries: vec![],
        }];
    }
    // Restore only history and split ratio; discard terminal content and extra tabs.
    let (history, split_ratio, history_entries) = if !session.tabs.is_empty() {
        let t = &session.tabs[0];
        (t.history.clone(), t.split_ratio, t.history_entries.clone())
    } else {
        (session.history, session.split_ratio, vec![])
    };
    vec![TabSession {
        terminal_output: String::new(),
        history,
        split_ratio,
        cwd: String::new(),
        history_entries,
    }]
}

/// Compute rows/cols for the initial PTY based on window dimensions and default cell metrics.
fn initial_terminal_size(
    window_width: u32,
    window_height: u32,
    first_split_ratio: f32,
    pad_h: f32,
    pad_v: f32,
) -> (usize, usize) {
    // Default cell metrics must match those used in GpuRuntimeState to avoid a
    // SIGWINCH-triggered prompt redraw immediately after startup.
    const DEFAULT_CELL_W: f32 = 8.4;
    const DEFAULT_CELL_H: f32 = 16.8;
    let lm = crate::layout::LayoutMetrics::new(
        window_width,
        window_height,
        0.0,
        DEFAULT_CELL_W,
        DEFAULT_CELL_H,
        pad_h,
        pad_v,
    );
    sanitize_terminal_size(lm.term_rows(first_split_ratio) as usize, lm.cols() as usize)
}

pub(crate) fn build_initial_state(
    exec: Option<&str>,
    launch_cwd: Option<&str>,
    shell: &str,
    session: PersistentSession,
    update_rx: std::sync::mpsc::Receiver<Result<Option<String>, String>>,
) -> anyhow::Result<GpuRuntimeState> {
    let window_width = session.window_width;
    let window_height = session.window_height;
    let window_x = session.window_x.unwrap_or(0);
    let window_y = session.window_y.unwrap_or(0);

    let (user_config, config_error) = match load_config_result() {
        Ok(cfg) => (cfg, None),
        Err(err) => {
            tracing::warn!(error = %err, "failed to load config; using defaults");
            (UserConfig::default(), Some(err.to_string()))
        }
    };
    let effective_shell = user_config
        .terminal
        .shell
        .clone()
        .unwrap_or_else(|| shell.to_owned());

    let saved_tabs = resolve_saved_tabs(session, user_config.terminal.restore_session);
    let first_split_ratio = saved_tabs.first().map(|t| t.split_ratio).unwrap_or(0.7);
    let (rows, cols) = initial_terminal_size(
        window_width,
        window_height,
        first_split_ratio,
        user_config.padding.horizontal as f32,
        user_config.padding.vertical as f32,
    );
    let tabs = build_tabs(saved_tabs, rows, cols, &effective_shell, exec, launch_cwd)?;

    let mut state = GpuRuntimeState {
        tabs,
        active_tab: 0,
        shell: effective_shell,
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
        update_last_checked: std::time::Instant::now(),
        settings: crate::SettingsUiState::default(),
        keybindings_panel: crate::state::KeybindingsUiState::default(),
        command_palette: None,
        ssh_hosts: crate::ssh::load_ssh_hosts(),
        window_focused: true,
        last_session_save: std::time::Instant::now(),
        should_exit: false,
        last_editor_disabled: false,
        shell_services: Box::new(crate::shell::SystemShell::new()),
        pty_waker: None,
    };

    apply_theme_and_font_selection(&mut state);
    if let Some(ref err) = state.config_error {
        use std::time::Duration;
        state.overlays.toasts.push_back(crate::state::Toast::new(
            format!("Config error: {err}"),
            crate::state::ToastKind::Error,
            Duration::from_secs(8),
        ));
    }

    Ok(state)
}

/// Parameters bundle for building a single tab, factored out to avoid a
/// too-many-arguments violation.
struct TabBuildParams<'a> {
    effective_shell: &'a str,
    exec: Option<&'a str>,
    shared_history: &'a [String],
    shared_entries: &'a [HistoryEntry],
}

fn build_tabs(
    saved_tabs: Vec<TabSession>,
    rows: usize,
    cols: usize,
    effective_shell: &str,
    exec: Option<&str>,
    launch_cwd: Option<&str>,
) -> anyhow::Result<Vec<TabState>> {
    // Process shared history and entries across all tabs
    let (shared_history, shared_entries) = process_shared_tab_data(&saved_tabs);

    let params = TabBuildParams {
        effective_shell,
        exec,
        shared_history: &shared_history,
        shared_entries: &shared_entries,
    };

    // Apply shared data to each tab
    let mut tabs: Vec<TabState> = Vec::new();
    for (i, saved) in saved_tabs.into_iter().enumerate() {
        let tab_state = build_single_tab(i, &saved, rows, cols, &params, launch_cwd)?;
        tabs.push(tab_state);
    }

    Ok(tabs)
}

/// Process shared history and entries across all tabs to ensure consistency
fn process_shared_tab_data(saved_tabs: &[TabSession]) -> (Vec<String>, Vec<HistoryEntry>) {
    // History belongs to the command editor, not to one tab. Older sessions
    // stored separate histories, so use the most complete timeline and recover
    // commands found only in another tab before giving every tab the same view.
    let mut shared_history = saved_tabs
        .iter()
        .max_by_key(|tab| tab.history.len())
        .map(|tab| tab.history.clone())
        .unwrap_or_default();
    for tab in saved_tabs {
        for command in &tab.history {
            if !shared_history.iter().any(|saved| saved == command) {
                shared_history.push(command.clone());
            }
        }
    }

    let mut shared_entries: Vec<HistoryEntry> = Vec::new();
    for tab in saved_tabs {
        for entry in &tab.history_entries {
            if let Some(saved) = shared_entries
                .iter_mut()
                .find(|saved| saved.cmd == entry.cmd)
            {
                saved.count = saved.count.max(entry.count);
                saved.last_used_secs = saved.last_used_secs.max(entry.last_used_secs);
            } else {
                shared_entries.push(entry.clone());
            }
        }
    }

    (shared_history, shared_entries)
}

/// Build a single tab with its associated PTY and application state
fn build_single_tab(
    index: usize,
    saved: &TabSession,
    rows: usize,
    cols: usize,
    params: &TabBuildParams<'_>,
    launch_cwd: Option<&str>,
) -> anyhow::Result<TabState> {
    let mut app = build_app(rows, cols)?;

    // Feed terminal output
    for line in saved.terminal_output.lines() {
        app.feed_terminal(line.as_bytes());
        app.feed_terminal(b"\r\n");
    }

    // Determine current working directory
    let initial_cwd: String = if let Some(cli_cwd) = launch_cwd {
        cli_cwd.to_owned()
    } else if !saved.cwd.is_empty() {
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

    // Spawn PTY session
    let (pty, integration) = if index == 0 {
        match spawn_pty(
            params.effective_shell,
            rows as u16,
            cols as u16,
            params.exec,
            restore_cwd,
            None,
        ) {
            Ok((p, integ)) => (Some(p), integ),
            Err(err) => {
                app.feed_terminal(format!("PTY unavailable: {err}\n").as_bytes());
                (None, false)
            }
        }
    } else {
        spawn_pty(
            params.effective_shell,
            rows as u16,
            cols as u16,
            None,
            restore_cwd,
            None,
        )
        .map(|(p, integ)| (Some(p), integ))
        .unwrap_or((None, false))
    };

    // Build and return the tab state
    Ok(TabState {
        app,
        pty,
        scroll_offset: 0,
        editor_scroll_offset: 0,
        editor_horizontal_scroll_offset: 0,
        history: params.shared_history.to_vec(),
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
            params.shared_entries.to_vec()
        },
        pending_cmd: None,
        command_blocks: Vec::new(),
        current_block: None,
        next_block_id: 1,
        shell_integration: integration,
        search: crate::search::SearchState::default(),
        copy_mode: crate::tab::CopyModeState::default(),
        command_running: false,
        editor_unlocked: false,
        command_start_time: None,
        unread_output: false,
        bell_pending: false,
        a11y_screen_version: 0,
        suppress_until: None,
        spawned_at: std::time::Instant::now(),
    })
}

fn apply_theme_and_font_selection(state: &mut GpuRuntimeState) {
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
/// `tab_idx` is the tab the menu was opened for; `item` is the index into
/// [`crate::command_registry::tab_context_menu_commands`] (same order the
/// menu was built with).
pub(crate) fn execute_context_menu_item(state: &mut GpuRuntimeState, tab_idx: usize, item: usize) {
    use crate::commands::{CommandContext, execute_ui_command};
    if let Some(def) = crate::command_registry::tab_context_menu_commands().get(item) {
        execute_ui_command(
            state,
            def.id,
            CommandContext {
                tab_idx: Some(tab_idx),
            },
        );
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
        assert_eq!(
            sanitize_terminal_size(0, 0),
            (TERMINAL_ROWS_MIN, TERMINAL_COLS_MIN)
        );
    }

    #[test]
    fn sanitize_terminal_size_clamps_high_values() {
        assert_eq!(
            sanitize_terminal_size(usize::MAX, usize::MAX),
            (TERMINAL_ROWS_MAX, TERMINAL_COLS_MAX)
        );
    }
}
