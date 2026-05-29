mod commands;
mod completion;
mod consts;
mod layout;
mod shell;
mod terminal_backend;

use completion::suggestion_matches_frecency;
use layout::LayoutMetrics;
use terminal_backend::TerminalBackend;
mod config;
mod coords;
mod input;
mod launch;
mod settings;
mod snapshot;
mod tab;
mod theme;
pub mod updater;
use app_orchestrator::App;
use clap::Parser;
use config::UserConfig;
use launch::{
    FontEntry, build_app, build_initial_state, load_session, sanitize_terminal_size, save_session,
    spawn_pty,
};
use platform_abstraction::default_shell;
use render_wgpu::{FontConfig, RenderConfig, run_gpu_window_live_with_events_and_window};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use tab::TabState;
use ui::{InputRouter, TabPane, UiConfig, UiState};
use tracing::{debug, warn};

struct UiComponentBridge {
    state: UiState<TerminalBackend>,
}

impl UiComponentBridge {
    fn new(shell: String, config: UiConfig) -> anyhow::Result<Self> {
        const DEFAULT_ROWS: usize = 24;
        const DEFAULT_COLS: usize = 80;
        let app = build_app(DEFAULT_ROWS, DEFAULT_COLS)?;
        let initial_tab = TabPane::new(TerminalBackend::new(app, None), String::new());
        let tab_factory: Box<dyn Fn() -> TabPane<TerminalBackend>> = Box::new(|| {
            let app = build_app(DEFAULT_ROWS, DEFAULT_COLS).unwrap_or_else(|err| {
                tracing::error!(error = %err, "failed to create default tab app");
                App::new(DEFAULT_ROWS, DEFAULT_COLS)
                    .unwrap_or_else(|_| unreachable!("default app size must be valid"))
            });
            TabPane::new(TerminalBackend::new(app, None), String::new())
        });
        let state = UiState::new(shell, config, initial_tab, tab_factory);
        Ok(Self { state })
    }

    fn handle_event(&mut self, event: &render_wgpu::AppWindowEvent) {
        let actions = InputRouter::process(&self.state, event);
        for action in actions {
            self.state.apply_action(action);
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "teletipo", version, about = "Modern terminal/editor prototype")]
struct Cli {
    #[arg(long, default_value_t = 24)]
    rows: usize,

    #[arg(long, default_value_t = 80)]
    cols: usize,

    #[arg(long)]
    shell: Option<String>,

    #[arg(long, help = "Execute a command and exit")]
    exec: Option<String>,

    #[command(subcommand)]
    command: Option<commands::Commands>,
}

#[derive(Debug, Default)]
struct SettingsUiState {
    open: bool,
    cursor: usize,
    edit_buf: Option<String>,
    dirty: bool,
    just_saved: bool,
    /// When `Some`, the focused searchable field is in type-to-filter mode.
    search_buf: Option<String>,
    /// Highlighted index within the current `search_matches` list.
    search_selected: usize,
    /// First visible index in the search dropdown (scroll offset).
    search_scroll_offset: usize,
}

/// Currently held keyboard modifier keys, refreshed on every
/// [`winit::event::WindowEvent::ModifiersChanged`].
#[derive(Default)]
pub(crate) struct ModifierState {
    pub(crate) ctrl_down: bool,
    /// Whether the Super/Command key (⌘ on macOS) is currently held.
    pub(crate) super_down: bool,
    pub(crate) shift_down: bool,
}

/// State for the various pointer-driven drag interactions (separator,
/// scrollbars, tab reorder). All fields default to a neutral "no drag in
/// progress" state.
#[derive(Default)]
pub(crate) struct DragState {
    /// Whether the user is currently dragging the separator bar.
    pub(crate) dragging_separator: bool,
    /// Whether the user is currently dragging the terminal scrollbar thumb.
    pub(crate) dragging_terminal_scrollbar: bool,
    /// Whether the user is currently dragging the editor scrollbar thumb.
    pub(crate) dragging_editor_scrollbar: bool,
    /// Index of the tab being dragged, if any.
    pub(crate) tab_drag: Option<usize>,
    /// Cursor x position at the moment the tab drag began.
    pub(crate) tab_drag_start_x: f64,
}

/// Last known cursor position and held mouse button. Updated on
/// [`winit::event::WindowEvent::CursorMoved`] and `MouseInput`.
#[derive(Default)]
pub(crate) struct CursorState {
    pub(crate) cursor_x: f64,
    pub(crate) cursor_y: f64,
    /// Which mouse button (0=left, 1=mid, 2=right) is currently held, for
    /// motion-reporting passthrough to the PTY (modes 1002/1003).
    pub(crate) mouse_btn_held: Option<u8>,
}

/// Window geometry plus the renderer's reported per-cell physical pixel size.
/// Updated on `Resized`, `WindowMoved`, and `ScaleFactorChanged` events.
pub(crate) struct LayoutState {
    pub(crate) window_width: u32,
    pub(crate) window_height: u32,
    /// Last known window top-left position in physical pixels.
    pub(crate) window_x: i32,
    pub(crate) window_y: i32,
    /// Current display scale factor (1.0 on standard, 2.0 on Retina, etc.).
    pub(crate) scale_factor: f64,
    /// Actual physical-pixel cell dimensions from the renderer font.
    pub(crate) cell_w: f32,
    pub(crate) cell_h: f32,
}

/// Transient UI overlay state: cursor blink, BEL flash, last-resize hint,
/// update-available banner, and right-click tab context menu.
pub(crate) struct OverlayState {
    /// Time and dimensions of the last PTY resize, shown as an overlay for 1 s.
    pub(crate) last_resize: Option<(Instant, u16, u16)>,
    /// Context menu opened by right-clicking a tab. (tab_idx, menu_x_px, menu_y_px)
    pub(crate) tab_context_menu: Option<(usize, f64, f64)>,
    /// Currently highlighted item inside the open context menu (0-3).
    pub(crate) tab_context_hover: Option<usize>,
    /// Status banner for the last background update check.
    pub(crate) pending_update: Option<UpdateBanner>,
    /// When `Some`, flash the terminal background as a visual BEL indicator
    /// until the contained `Instant`.
    pub(crate) bell_flash_until: Option<Instant>,
    /// Time the cursor blink half-cycle last toggled.
    pub(crate) cursor_blink_last: Instant,
    /// `true` = cursor visible (on-phase); `false` = cursor hidden (off-phase).
    pub(crate) cursor_blink_phase: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum UpdateBanner {
    Available(String),
    Failed(String),
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            last_resize: None,
            tab_context_menu: None,
            tab_context_hover: None,
            pending_update: None,
            bell_flash_until: None,
            cursor_blink_last: Instant::now(),
            cursor_blink_phase: true,
        }
    }
}

/// Theme and font catalogues discovered at startup plus the index of the
/// currently active preset (if any).
#[derive(Default)]
pub(crate) struct ThemeFontState {
    /// All theme files discovered at startup (sorted by name).
    pub(crate) available_themes: Vec<theme::ThemeFile>,
    /// Index into `available_themes` of the currently active preset, or `None`
    /// when the user is using custom colors.
    pub(crate) active_theme_idx: Option<usize>,
    /// All font families discovered at startup (index 0 = "(default)").
    pub(crate) available_fonts: Vec<FontEntry>,
    /// Index into `available_fonts` of the currently selected font.
    /// 0 means "(default)", i.e. no font family override.
    pub(crate) active_font_idx: usize,
}

struct GpuRuntimeState {
    tabs: Vec<TabState>,
    active_tab: usize,
    /// Shell executable used to spawn new PTY sessions.
    shell: String,
    modifiers: ModifierState,
    layout: LayoutState,
    cursor: CursorState,
    drag: DragState,
    overlays: OverlayState,
    /// Loaded user configuration (applied per frame to the render snapshot).
    user_config: UserConfig,
    themes_fonts: ThemeFontState,
    /// Receiver for the background update-check result (consumed once after the
    /// check completes; set to `None` afterwards).
    update_rx: Option<std::sync::mpsc::Receiver<Result<Option<String>, String>>>,
    /// Settings overlay interaction state.
    settings: SettingsUiState,
    /// Set to `true` when the last shell session ends so the window closes.
    should_exit: bool,
    /// Abstraction over host-OS capabilities (clipboard today). Boxed so tests
    /// can swap in a [`shell::NullShell`].
    shell_services: Box<dyn shell::AppShell>,
}

impl GpuRuntimeState {
    fn tab(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    fn tab_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    /// Height of the tab bar in pixels. Hidden when only one tab is open.
    fn tab_bar_h(&self) -> f32 {
        if self.tabs.len() > 1 {
            self.layout.cell_h
        } else {
            0.0
        }
    }

    /// Pump PTY output for ALL tabs; returns `true` if the active tab received data.
    fn pump_all_ptys(&mut self) -> bool {
        let mut active_had_data = false;
        let active = self.active_tab;
        let mut dead_tabs: Vec<usize> = Vec::new();
        let mut exit_codes: Vec<(usize, i32)> = Vec::new();
        let mut resize_tabs: Vec<usize> = Vec::new();
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            let Some(mut pty) = tab.pty.take() else {
                continue;
            };
            let had_data = tab
                .app
                .pump_pty_once(&mut pty)
                .map(|n| n > 0)
                .unwrap_or(false);
            // Send any pending DSR responses (e.g. \x1b[row;colR) back to the PTY.
            for response in tab.app.drain_pending_responses() {
                if let Err(err) = tab.app.send_pty_input(&mut pty, response.as_bytes()) {
                    warn!(error = %err, response = %response, "failed to send pending response to pty");
                }
            }
            let is_dead = match pty.try_wait() {
                Ok(status) => status.is_some(),
                Err(err) => {
                    debug!(error = %err, "failed to query pty child status");
                    false
                }
            };
            tab.pty = Some(pty);
            if i == active && had_data {
                active_had_data = true;
            }
            if tab.app.take_bell() && self.user_config.terminal.bell {
                self.overlays.bell_flash_until =
                    Some(Instant::now() + std::time::Duration::from_millis(150));
            }
            let now_fullscreen = tab.app.is_alternate_screen();
            if now_fullscreen != tab.was_terminal_fullscreen {
                tab.was_terminal_fullscreen = now_fullscreen;
                if now_fullscreen {
                    tab.pre_fullscreen_split_ratio = tab.split_ratio;
                    tab.split_ratio = 1.0;
                    tab.scroll_offset = 0;
                    tab.is_selecting = false;
                    tab.is_selecting_editor = false;
                } else {
                    tab.split_ratio = tab.pre_fullscreen_split_ratio.clamp(0.2, 0.85);
                }
                resize_tabs.push(i);
            }
            if let Some(code) = tab.app.take_last_exit_code() {
                exit_codes.push((i, code));
            }
            if is_dead {
                dead_tabs.push(i);
            }
        }
        // Commit or discard pending commands based on shell-reported exit codes.
        for (idx, code) in exit_codes {
            if code == 0 {
                self.commit_pending_cmd(idx);
            } else {
                self.tabs[idx].pending_cmd = None;
            }
        }
        if !resize_tabs.is_empty() {
            let lm = LayoutMetrics::new(
                self.layout.window_width,
                self.layout.window_height,
                self.tab_bar_h(),
                self.layout.cell_w,
                self.layout.cell_h,
                self.user_config.padding.horizontal as f32,
                self.user_config.padding.vertical as f32,
            );
            let cols = lm.cols();
            for i in resize_tabs {
                if i >= self.tabs.len() {
                    continue;
                }
                let rows = lm.term_rows(self.tabs[i].split_ratio);
                self.resize_tab(i, rows, cols);
            }
        }
        // Close tabs whose shell exited; if it is the last tab, quit the app.
        for &idx in dead_tabs.iter().rev() {
            if self.tabs.len() == 1 {
                self.should_exit = true;
            } else {
                self.close_tab(idx);
            }
        }
        active_had_data
    }

    fn send_terminal_input(&mut self, bytes: &[u8]) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        let Some(mut pty) = tab.pty.take() else {
            return;
        };
        if let Err(err) = tab.app.send_pty_input(&mut pty, bytes) {
            warn!(error = %err, byte_len = bytes.len(), "failed to send terminal input to pty");
        }
        tab.pty = Some(pty);
    }

    /// Commit `pending_cmd` (if any) for `tab_idx` to history.
    /// Called when the shell reports exit code 0 via OSC 133.
    fn commit_pending_cmd(&mut self, tab_idx: usize) {
        let tab = &mut self.tabs[tab_idx];
        let Some(text) = tab.pending_cmd.take() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        // Deduplicate.
        tab.history.retain(|e| e.trim() != text.as_str());
        // Upsert frecency entry.
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let entry_idx = tab
            .history_entries
            .iter()
            .position(|e| e.cmd.trim() == text.as_str());
        if let Some(idx) = entry_idx {
            tab.history_entries[idx].count += 1;
            tab.history_entries[idx].last_used_secs = now_secs;
        } else {
            tab.history_entries.push(tab::HistoryEntry {
                cmd: text.clone(),
                count: 1,
                last_used_secs: now_secs,
            });
        }
        tab.history.push(text);
    }

    fn run_editor_command(&mut self) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        let text = tab.app.editor_snapshot();
        let text = text.trim().to_string();
        if !text.is_empty() {
            if tab.shell_integration {
                // Defer: save to history only after the shell reports exit code 0.
                tab.pending_cmd = Some(text);
            } else {
                // No integration – save immediately (original behaviour).
                tab.history.retain(|e| e.trim() != text.as_str());
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let entry_idx = tab
                    .history_entries
                    .iter()
                    .position(|e| e.cmd.trim() == text.as_str());
                if let Some(idx) = entry_idx {
                    tab.history_entries[idx].count += 1;
                    tab.history_entries[idx].last_used_secs = now_secs;
                } else {
                    tab.history_entries.push(tab::HistoryEntry {
                        cmd: text.clone(),
                        count: 1,
                        last_used_secs: now_secs,
                    });
                }
                tab.history.push(text);
            }
        }
        // Always reset navigation state regardless of integration mode.
        tab.history_index = None;
        tab.saved_input = String::new();
        tab.suggestion_prefix = None;
        tab.suggestion_index = None;
        let Some(mut pty) = tab.pty.take() else {
            return;
        };
        let _ = tab.app.run_editor_command(&mut pty, false);
        tab.pty = Some(pty);
        tab.scroll_offset = 0;
        tab.editor_scroll_offset = 0;
    }

    fn history_prev(&mut self) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        if tab.history.is_empty() {
            return;
        }
        let new_idx = match tab.history_index {
            None => {
                tab.saved_input = tab.app.editor_snapshot();
                tab.history.len() - 1
            }
            Some(0) => return,
            Some(i) => i - 1,
        };
        tab.history_index = Some(new_idx);
        let entry = tab.history[new_idx].clone();
        tab.app.editor_clear();
        tab.app.insert_editor_input(&entry);
        tab.editor_scroll_offset = 0;
    }

    fn history_next(&mut self) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        let Some(idx) = tab.history_index else { return };
        if idx + 1 < tab.history.len() {
            let new_idx = idx + 1;
            tab.history_index = Some(new_idx);
            let entry = tab.history[new_idx].clone();
            tab.app.editor_clear();
            tab.app.insert_editor_input(&entry);
            tab.editor_scroll_offset = 0;
        } else {
            tab.history_index = None;
            let saved = tab.saved_input.clone();
            tab.app.editor_clear();
            tab.app.insert_editor_input(&saved);
        }
    }

    fn resize_tab(&mut self, idx: usize, rows: u16, cols: u16) {
        let tab = &mut self.tabs[idx];
        if let Some(pty) = tab.pty.as_mut() {
            pty.resize(rows, cols);
        }
        tab.app.resize_terminal(rows as usize, cols as usize);
        let max_scroll = tab.app.scrollback_len();
        if tab.scroll_offset > max_scroll {
            tab.scroll_offset = max_scroll;
        }
    }

    /// Resize every tab after a window resize. Each tab uses its own split_ratio.
    fn resize_all_tabs(&mut self) {
        let lm = LayoutMetrics::new(
            self.layout.window_width,
            self.layout.window_height,
            self.tab_bar_h(),
            self.layout.cell_w,
            self.layout.cell_h,
            self.user_config.padding.horizontal as f32,
            self.user_config.padding.vertical as f32,
        );
        let cols = lm.cols();
        let n = self.tabs.len();
        for i in 0..n {
            let rows = lm.term_rows(self.tabs[i].split_ratio);
            self.resize_tab(i, rows, cols);
        }
    }

    fn add_new_tab(&mut self) {
        let split_ratio = self.tab().split_ratio;
        // After adding the new tab there will be at least 2 tabs, so the tab bar
        // will appear and steal one cell row — account for that when sizing the PTY.
        let lm = LayoutMetrics::new(
            self.layout.window_width,
            self.layout.window_height,
            self.layout.cell_h, // tab_bar_h = cell_h (will be visible after push)
            self.layout.cell_w,
            self.layout.cell_h,
            self.user_config.padding.horizontal as f32,
            self.user_config.padding.vertical as f32,
        );
        let cols = lm.cols();
        let rows = lm.term_rows(split_ratio);
        let app = match build_app(rows as usize, cols as usize) {
            Ok(app) => app,
            Err(err) => {
                tracing::error!(error = %err, rows, cols, "failed to add tab");
                return;
            }
        };
        let active_cwd = self.tab().cwd.clone();
        let (pty, integration) = spawn_pty(&self.shell, rows, cols, None, Some(&active_cwd))
            .map(|(p, i)| (Some(p), i))
            .unwrap_or((None, false));
        self.tabs.push(TabState {
            app,
            pty,
            scroll_offset: 0,
            editor_scroll_offset: 0,
            history: vec![],
            history_index: None,
            saved_input: String::new(),
            split_ratio,
            was_terminal_fullscreen: false,
            pre_fullscreen_split_ratio: split_ratio,
            selection_anchor: None,
            selection_anchor_scroll: 0,
            selection_end: None,
            selection_end_scroll: 0,
            is_selecting: false,
            is_selecting_editor: false,
            last_terminal_text: String::new(),
            term_row_count: rows as usize,
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            suggestion_prefix: None,
            suggestion_index: None,
            history_entries: vec![],
            pending_cmd: None,
            shell_integration: integration,
        });
        self.active_tab = self.tabs.len() - 1;
    }

    /// Close the tab at `idx`. No-op when there is only one tab.
    fn close_tab(&mut self, idx: usize) {
        if self.tabs.len() == 1 {
            return;
        }
        self.tabs.remove(idx); // PTY is dropped → SIGHUP sent to shell
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Move the tab at `from` so that it ends up just before `insert_before`.
    /// Indices are clamped; calling with a no-op position is safe.
    fn move_tab_to(&mut self, from: usize, insert_before: usize) {
        let n = self.tabs.len();
        if from >= n {
            return;
        }
        let insert_before = insert_before.min(n);
        // No movement needed.
        if insert_before == from || insert_before == from + 1 {
            return;
        }
        let tab = self.tabs.remove(from);
        // After the remove the insertion index shifts if we are moving rightward.
        let actual = if insert_before > from {
            insert_before - 1
        } else {
            insert_before
        };
        self.tabs.insert(actual, tab);
        // Adjust active_tab so the same tab remains active.
        if self.active_tab == from {
            self.active_tab = actual;
        } else if from < actual {
            // Tabs in (from, actual] shifted one step left.
            if self.active_tab > from && self.active_tab <= actual {
                self.active_tab -= 1;
            }
        } else {
            // Tabs in [actual, from) shifted one step right.
            if self.active_tab >= actual && self.active_tab < from {
                self.active_tab += 1;
            }
        }
    }
}

pub fn run(
    update_rx: std::sync::mpsc::Receiver<Result<Option<String>, String>>,
) -> std::process::ExitCode {
    let cli = Cli::parse();
    if let Some(cmd) = cli.command {
        return commands::dispatch(cmd);
    }
    let shell = cli.shell.unwrap_or_else(default_shell);

    // ── GPU path ─────────────────────────────────────────────────────────────
    let session = load_session();
    // Clamp to sane logical-pixel bounds — guards against session files that
    // previously stored physical pixels and grew beyond GPU texture limits.
    let window_width = session.window_width.clamp(400, 3840);
    let window_height = session.window_height.clamp(300, 2160);
    let window_pos = match (session.window_x, session.window_y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    let (rows, cols) = sanitize_terminal_size(cli.rows, cli.cols);
    let state = match build_initial_state(
        rows,
        cols,
        cli.exec.as_deref(),
        &shell,
        session,
        update_rx,
    ) {
        Ok(state) => Rc::new(RefCell::new(state)),
        Err(err) => {
            tracing::error!(error = %err, "failed to initialize runtime state");
            return std::process::ExitCode::FAILURE;
        }
    };
    let ui_bridge = Rc::new(RefCell::new({
        let s = state.borrow();
        match UiComponentBridge::new(
            shell.clone(),
            UiConfig {
                padding_horizontal: s.user_config.padding.horizontal as f32,
                padding_vertical: s.user_config.padding.vertical as f32,
                active_theme_idx: s.themes_fonts.active_theme_idx,
                active_font_idx: s.themes_fonts.active_font_idx,
                font_size: s.user_config.font.size,
                font_family: s.user_config.font.family.clone(),
                terminal_shell: s.user_config.terminal.shell.clone(),
                terminal_scrollback_lines: s.user_config.terminal.scrollback_lines,
                terminal_bell: s.user_config.terminal.bell,
                active_theme: s.user_config.active_theme.clone(),
                available_themes: s
                    .themes_fonts
                    .available_themes
                    .iter()
                    .map(|t| t.name.clone())
                    .collect(),
                available_fonts: s
                    .themes_fonts
                    .available_fonts
                    .iter()
                    .map(|f| f.family.clone())
                    .collect(),
            },
        ) {
            Ok(bridge) => bridge,
            Err(err) => {
                tracing::error!(error = %err, "failed to initialize UI bridge");
                return std::process::ExitCode::FAILURE;
            }
        }
    }));
    let (initial_font_family, initial_font_size) = {
        let s = state.borrow();
        (s.user_config.font.family.clone(), s.user_config.font.size)
    };
    let state_for_frames = Rc::clone(&state);
    let state_for_events = Rc::clone(&state);
    let state_for_window = Rc::clone(&state);
    let bridge_for_events = Rc::clone(&ui_bridge);

    if let Err(err) = run_gpu_window_live_with_events_and_window(
        move || {
            let mut state = state_for_frames.borrow_mut();
            snapshot::build_snapshot(&mut state)
        },
        move |event| {
            bridge_for_events.borrow_mut().handle_event(&event);
            let mut state = state_for_events.borrow_mut();
            input::handle_event(&mut state, event);
        },
        move |window| {
            state_for_window
                .borrow_mut()
                .shell_services
                .install_window(window);
        },
        RenderConfig {
            initial_size: Some((window_width, window_height)),
            initial_position: window_pos,
            font: FontConfig {
                font_family: initial_font_family,
                font_size: initial_font_size,
            },
            ..RenderConfig::default()
        },
    ) {
        tracing::error!(error = %err, "failed to start gpu backend");
    }

    // Persist session state so the next run can restore it.
    save_session(&state.borrow());
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod input_smoke_tests {
    //! Headless smoke tests that drive [`input::handle_event`] against a
    //! [`shell::NullShell`]-backed [`GpuRuntimeState`] without spawning a PTY
    //! or opening a window.

    use super::*;
    use render_wgpu::AppWindowEvent;

    /// Construct a minimal [`GpuRuntimeState`] with a single PTY-less tab and
    /// the provided [`shell::AppShell`] implementation. Suitable for tests that
    /// only need to exercise event dispatch — no window, no shell process.
    fn build_test_state(shell_services: Box<dyn shell::AppShell>) -> GpuRuntimeState {
        let app = app_orchestrator::App::new(24, 80).expect("valid terminal size");
        let tab = tab::TabState {
            app,
            pty: None,
            scroll_offset: 0,
            editor_scroll_offset: 0,
            history: Vec::new(),
            history_index: None,
            saved_input: String::new(),
            split_ratio: 0.5,
            was_terminal_fullscreen: false,
            pre_fullscreen_split_ratio: 0.5,
            selection_anchor: None,
            selection_anchor_scroll: 0,
            selection_end: None,
            selection_end_scroll: 0,
            is_selecting: false,
            is_selecting_editor: false,
            last_terminal_text: String::new(),
            term_row_count: 24,
            cwd: String::new(),
            suggestion_prefix: None,
            suggestion_index: None,
            history_entries: Vec::new(),
            pending_cmd: None,
            shell_integration: false,
        };
        GpuRuntimeState {
            tabs: vec![tab],
            active_tab: 0,
            shell: "/bin/sh".to_owned(),
            modifiers: ModifierState::default(),
            layout: LayoutState {
                window_width: 800,
                window_height: 600,
                window_x: 0,
                window_y: 0,
                scale_factor: 1.0,
                cell_w: 8.0,
                cell_h: 16.0,
            },
            cursor: CursorState::default(),
            drag: DragState::default(),
            overlays: OverlayState::default(),
            user_config: config::UserConfig::default(),
            themes_fonts: ThemeFontState::default(),
            update_rx: None,
            settings: SettingsUiState::default(),
            should_exit: false,
            shell_services,
        }
    }

    #[test]
    fn cursor_moved_event_updates_cursor_state() {
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        input::handle_event(
            &mut state,
            AppWindowEvent::CursorMoved {
                x: 123.5,
                y: 456.25,
            },
        );
        assert_eq!(state.cursor.cursor_x, 123.5);
        assert_eq!(state.cursor.cursor_y, 456.25);
    }

    #[test]
    fn window_moved_event_updates_layout() {
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        input::handle_event(&mut state, AppWindowEvent::WindowMoved { x: 42, y: -7 });
        assert_eq!(state.layout.window_x, 42);
        assert_eq!(state.layout.window_y, -7);
    }

    #[test]
    fn modifiers_changed_event_updates_modifier_state() {
        use winit::keyboard::ModifiersState;
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        let mods = ModifiersState::SUPER | ModifiersState::SHIFT;
        input::handle_event(&mut state, AppWindowEvent::ModifiersChanged(mods));
        assert!(state.modifiers.super_down);
        assert!(state.modifiers.shift_down);
        assert!(!state.modifiers.ctrl_down);
    }

    #[test]
    fn ime_commit_event_inserts_text_into_editor() {
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        input::handle_event(&mut state, AppWindowEvent::ImeCommit("héllo".to_owned()));
        assert!(state.tab().app.editor_snapshot().contains("héllo"));
    }

    #[test]
    fn resized_event_updates_layout_dimensions() {
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        input::handle_event(
            &mut state,
            AppWindowEvent::Resized {
                width: 1024,
                height: 768,
                scale_factor: 2.0,
                cell_w: 10.0,
                cell_h: 20.0,
            },
        );
        assert_eq!(state.layout.window_width, 1024);
        assert_eq!(state.layout.window_height, 768);
        assert_eq!(state.layout.scale_factor, 2.0);
        assert_eq!(state.layout.cell_w, 10.0);
        assert_eq!(state.layout.cell_h, 20.0);
    }

    #[test]
    fn null_shell_clipboard_roundtrip_via_state() {
        // Verifies the NullShell wired through `shell_services` is reachable
        // from within state and round-trips clipboard text correctly — the
        // contract the Cmd+C / Cmd+V keyboard handlers rely on.
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        state.shell_services.clipboard_set("smoke-test".to_owned());
        assert_eq!(
            state.shell_services.clipboard_get().as_deref(),
            Some("smoke-test"),
        );
    }
}
