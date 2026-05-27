use crate::config::load_config;
use crate::tab::{PersistentSession, TabSession, TabState};
use crate::theme;
use crate::GpuRuntimeState;
use app_orchestrator::App;
use std::fs;
use std::path::PathBuf;
use terminal_pty::PortablePtySession;

// ── Font discovery ────────────────────────────────────────────────────────────

/// A font file discovered on the machine.
pub(crate) struct FontFile {
    /// Display name derived from the file stem (e.g. "Hack-Regular").
    pub(crate) name: String,
    /// Absolute path to the font file.
    pub(crate) path: String,
}

/// Scan the platform's standard font directories and return every TTF/OTF/TTC
/// file found, sorted by display name.  The first entry is always a synthetic
/// "(default)" entry (empty path) so that index 0 means "no override".
pub(crate) fn enumerate_fonts() -> Vec<FontFile> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join("Library/Fonts"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".fonts"));
            dirs.push(home.join(".local/share/fonts"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(win) = std::env::var("WINDIR") {
            dirs.push(PathBuf::from(win).join("Fonts"));
        }
        if let Some(local) = dirs::data_local_dir() {
            dirs.push(local.join("Microsoft").join("Windows").join("Fonts"));
        }
    }

    let mut fonts: Vec<FontFile> = Vec::new();
    for dir in &dirs {
        scan_font_dir(dir, &mut fonts);
    }
    fonts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    fonts.insert(0, FontFile { name: "(default)".to_owned(), path: String::new() });
    fonts
}

fn scan_font_dir(dir: &PathBuf, out: &mut Vec<FontFile>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_font_dir(&path, out);
        } else if let Some(ext) = path.extension() {
            let ext_lower = ext.to_string_lossy().to_lowercase();
            if matches!(ext_lower.as_str(), "ttf" | "otf" | "ttc") {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                out.push(FontFile { name, path: path.to_string_lossy().into_owned() });
            }
        }
    }
}

// ── PTY spawning ──────────────────────────────────────────────────────────────

pub(crate) fn spawn_pty(
    shell: &str,
    rows: u16,
    cols: u16,
    exec: Option<&str>,
    cwd: Option<&str>,
) -> anyhow::Result<PortablePtySession> {
    match exec {
        Some(cmd) => {
            #[cfg(target_os = "windows")]
            {
                PortablePtySession::spawn_command(
                    "powershell.exe",
                    &["-NoProfile", "-Command", cmd],
                    rows,
                    cols,
                    cwd,
                )
            }
            #[cfg(not(target_os = "windows"))]
            {
                PortablePtySession::spawn_command(shell, &["-lc", cmd], rows, cols, cwd)
            }
        }
        None => PortablePtySession::spawn_shell(shell, rows, cols, cwd),
    }
}

// ── Session persistence ───────────────────────────────────────────────────────

pub(crate) fn session_path() -> Option<PathBuf> {
    let dir = dirs::data_local_dir()?.join("teletipo");
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("session.json"))
}

pub(crate) fn load_session() -> PersistentSession {
    let path = match session_path() {
        Some(p) => p,
        None => return PersistentSession::default(),
    };
    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return PersistentSession::default(),
    };
    serde_json::from_str(&data).unwrap_or_default()
}

pub(crate) fn save_session(state: &GpuRuntimeState) {
    let Some(path) = session_path() else { return };

    fn trim_output(s: &str) -> String {
        let t: String = s.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
        t.trim_end().to_string()
    }

    let tab_sessions: Vec<TabSession> = state.tabs.iter().map(|tab| TabSession {
        terminal_output: trim_output(&tab.app.terminal_ansi_snapshot()),
        history: tab.history.clone(),
        split_ratio: tab.split_ratio,
        cwd: tab.cwd.clone(),
    }).collect();

    let active = &state.tabs[state.active_tab];
    let session = PersistentSession {
        window_width: state.window_width,
        window_height: state.window_height,
        tabs: tab_sessions,
        split_ratio: active.split_ratio,
        history: active.history.clone(),
        terminal_output: trim_output(&active.app.terminal_ansi_snapshot()),
    };
    if let Ok(json) = serde_json::to_string_pretty(&session) {
        let _ = fs::write(path, json);
    }
}

pub(crate) fn build_initial_state(
    rows: usize,
    cols: usize,
    exec: Option<&str>,
    shell: &str,
    session: PersistentSession,
) -> GpuRuntimeState {
    let window_width = session.window_width;
    let window_height = session.window_height;

    let saved_tabs: Vec<TabSession> = if !session.tabs.is_empty() {
        session.tabs
    } else {
        vec![TabSession {
            terminal_output: session.terminal_output,
            history: session.history,
            split_ratio: session.split_ratio,
            cwd: String::new(),
        }]
    };

    let mut tabs: Vec<TabState> = Vec::new();
    for (i, saved) in saved_tabs.into_iter().enumerate() {
        let mut app = App::new(rows, cols).expect("valid terminal size");
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
        let pty = if i == 0 {
            match spawn_pty(shell, rows as u16, cols as u16, exec, restore_cwd) {
                Ok(p) => Some(p),
                Err(err) => {
                    app.feed_terminal(format!("PTY unavailable: {err}\n").as_bytes());
                    None
                }
            }
        } else {
            spawn_pty(shell, rows as u16, cols as u16, None, restore_cwd).ok()
        };
        tabs.push(TabState {
            app,
            pty,
            scroll_offset: 0,
            editor_scroll_offset: 0,
            history: saved.history,
            history_index: None,
            saved_input: String::new(),
            split_ratio: saved.split_ratio,
            selection_anchor: None,
            selection_end: None,
            is_selecting: false,
            is_selecting_editor: false,
            last_terminal_text: String::new(),
            term_row_count: rows,
            cwd: initial_cwd,
        });
    }

    let mut state = GpuRuntimeState {
        tabs,
        active_tab: 0,
        shell: shell.to_owned(),
        ctrl_down: false,
        super_down: false,
        window_width,
        window_height,
        scale_factor: 1.0,
        cursor_x: 0.0,
        cursor_y: 0.0,
        dragging_separator: false,
        dragging_terminal_scrollbar: false,
        dragging_editor_scrollbar: false,
        last_resize: None,
        shift_down: false,
        cell_w: 8.4,
        cell_h: 16.8,
        tab_drag: None,
        tab_drag_start_x: 0.0,
        tab_context_menu: None,
        tab_context_hover: None,
        user_config: load_config(),
        available_themes: {
            theme::install_default_themes();
            theme::load_themes()
        },
        active_theme_idx: None,
        available_fonts: enumerate_fonts(),
        active_font_idx: 0,
        settings: crate::SettingsUiState::default(),
    };

    state.active_theme_idx = state
        .user_config
        .active_theme
        .as_ref()
        .and_then(|name| state.available_themes.iter().position(|t| &t.name == name));
    state.active_font_idx = state
        .user_config
        .font
        .path
        .as_ref()
        .and_then(|p| state.available_fonts.iter().position(|f| &f.path == p))
        .unwrap_or(0);

    state
}

// ── Context menu ──────────────────────────────────────────────────────────────

/// Execute the selected item from the tab context menu.
/// `tab_idx` is the tab the menu was opened for; `item` is 0-3.
pub(crate) fn execute_context_menu_item(
    state: &mut GpuRuntimeState,
    tab_idx: usize,
    item: usize,
) {
    match item {
        0 => state.add_new_tab(),
        1 => state.close_tab(tab_idx),
        2 => state.move_tab_to(tab_idx, tab_idx.saturating_sub(1)),
        3 => state.move_tab_to(tab_idx, tab_idx + 2),
        _ => {}
    }
}
