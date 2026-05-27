mod config;
mod coords;
mod input;
mod launch;
mod settings;
mod snapshot;
mod tab;
mod theme;
pub mod updater;
use config::UserConfig;
use launch::{build_initial_state, FontFile, load_session, save_session, spawn_pty};
use app_orchestrator::App;
use clap::Parser;
use platform_abstraction::default_shell;
use render_wgpu::{run_gpu_window_live_with_events, FontConfig, RenderConfig};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use tab::TabState;

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
}

#[derive(Debug, Default)]
struct SettingsUiState {
    open: bool,
    cursor: usize,
    edit_buf: Option<String>,
    dirty: bool,
    just_saved: bool,
}

struct GpuRuntimeState {
    tabs: Vec<TabState>,
    active_tab: usize,
    /// Shell executable used to spawn new PTY sessions.
    shell: String,
    ctrl_down: bool,
    /// Whether the Super/Command key (⌘ on macOS) is currently held.
    super_down: bool,
    window_width: u32,
    window_height: u32,
    /// Current display scale factor (1.0 on standard, 2.0 on Retina, etc.).
    scale_factor: f64,
    cursor_x: f64,
    cursor_y: f64,
    /// Whether the user is currently dragging the separator bar.
    dragging_separator: bool,
    /// Whether the user is currently dragging the terminal scrollbar thumb.
    dragging_terminal_scrollbar: bool,
    /// Whether the user is currently dragging the editor scrollbar thumb.
    dragging_editor_scrollbar: bool,
    /// Time and dimensions of the last PTY resize, shown as an overlay for 1 s.
    last_resize: Option<(Instant, u16, u16)>,
    /// Whether the Shift key is currently held.
    shift_down: bool,
    /// Actual physical-pixel cell dimensions from the renderer font (updated on Resized).
    cell_w: f32,
    cell_h: f32,
    /// Tab drag-and-drop state.
    tab_drag: Option<usize>,         // index of the tab being dragged
    tab_drag_start_x: f64,           // cursor x at the moment the drag began
    /// Context menu opened by right-clicking a tab. (tab_idx, menu_x_px, menu_y_px)
    tab_context_menu: Option<(usize, f64, f64)>,
    /// Currently highlighted item inside the open context menu (0-3).
    tab_context_hover: Option<usize>,
    /// Loaded user configuration (applied per frame to the render snapshot).
    user_config: UserConfig,
    /// All theme files discovered at startup (sorted by name).
    available_themes: Vec<theme::ThemeFile>,
    /// Index into `available_themes` of the currently active preset, or `None`
    /// when the user is using custom colors.
    active_theme_idx: Option<usize>,
    /// All font files discovered at startup (index 0 = "(default)").
    available_fonts: Vec<FontFile>,
    /// Index into `available_fonts` of the currently selected font.
    /// 0 means "(default)", i.e. no font path override.
    active_font_idx: usize,
    /// Receiver for the background update-check result (consumed once after the
    /// check completes; set to `None` afterwards).
    update_rx: Option<std::sync::mpsc::Receiver<Option<String>>>,
    /// Set to `Some(version)` once a newer release is detected on GitHub.
    pending_update: Option<String>,
    /// Settings overlay interaction state.
    settings: SettingsUiState,
}

impl GpuRuntimeState {
    fn tab(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    fn tab_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    /// Height of the tab bar in pixels (always one cell row — tab bar is always visible).
    fn tab_bar_h(&self) -> f32 {
        self.cell_h
    }

    /// Pump PTY output for ALL tabs; returns `true` if the active tab received data.
    fn pump_all_ptys(&mut self) -> bool {
        let mut active_had_data = false;
        let active = self.active_tab;
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            let Some(mut pty) = tab.pty.take() else { continue };
            let had_data = tab.app.pump_pty_once(&mut pty).map(|n| n > 0).unwrap_or(false);
            tab.pty = Some(pty);
            if i == active && had_data {
                active_had_data = true;
            }
        }
        active_had_data
    }

    fn send_terminal_input(&mut self, bytes: &[u8]) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        let Some(mut pty) = tab.pty.take() else { return };
        let _ = tab.app.send_pty_input(&mut pty, bytes);
        tab.pty = Some(pty);
    }

    fn run_editor_command(&mut self) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        let text = tab.app.editor_snapshot();
        if !text.is_empty() {
            tab.history.push(text);
            tab.history_index = None;
            tab.saved_input = String::new();
        }
        let Some(mut pty) = tab.pty.take() else { return };
        let _ = tab.app.run_editor_command(&mut pty, false);
        tab.pty = Some(pty);
        tab.scroll_offset = 0;
        tab.editor_scroll_offset = 0;
    }

    fn history_prev(&mut self) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        if tab.history.is_empty() { return; }
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
        if let Some(pty) = tab.pty.as_mut() { pty.resize(rows, cols); }
        tab.app.resize_terminal(rows as usize, cols as usize);
        let max_scroll = tab.app.scrollback_len();
        if tab.scroll_offset > max_scroll { tab.scroll_offset = max_scroll; }
    }

    /// Resize every tab after a window resize. Each tab uses its own split_ratio.
    fn resize_all_tabs(&mut self) {
        let tab_bar_h = self.tab_bar_h();
        let available_h = self.window_height as f32 - tab_bar_h;
        let pad_h = self.user_config.padding.horizontal as f32;
        let pad_v = self.user_config.padding.vertical as f32;
        let cols = ((self.window_width as f32 - 2.0 * pad_h) / self.cell_w).max(1.0) as u16;
        let n = self.tabs.len();
        for i in 0..n {
            let term_h = (available_h * self.tabs[i].split_ratio - 2.0 * pad_v).max(self.cell_h);
            let rows = (term_h / self.cell_h).max(1.0) as u16;
            self.resize_tab(i, rows, cols);
        }
    }

    fn add_new_tab(&mut self) {
        let split_ratio = self.tab().split_ratio;
        // After adding the new tab there will be at least 2 tabs, so the tab bar
        // will appear and steal one cell row — account for that when sizing the PTY.
        let tab_bar_h = self.cell_h; // will be > 1 after push
        let available_h = self.window_height as f32 - tab_bar_h;
        let pad_h = self.user_config.padding.horizontal as f32;
        let pad_v = self.user_config.padding.vertical as f32;
        let cols = ((self.window_width as f32 - 2.0 * pad_h) / self.cell_w).max(1.0) as u16;
        let term_h = (available_h * split_ratio - 2.0 * pad_v).max(self.cell_h);
        let rows = (term_h / self.cell_h).max(1.0) as u16;
        let app = App::new(rows as usize, cols as usize).expect("valid size");
        let active_cwd = self.tab().cwd.clone();
        let pty = spawn_pty(&self.shell, rows, cols, None, Some(&active_cwd)).ok();
        self.tabs.push(TabState {
            app,
            pty,
            scroll_offset: 0,
            editor_scroll_offset: 0,
            history: vec![],
            history_index: None,
            saved_input: String::new(),
            split_ratio,
            selection_anchor: None,
            selection_end: None,
            is_selecting: false,
            is_selecting_editor: false,
            last_terminal_text: String::new(),
            term_row_count: rows as usize,
            cwd: std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
        });
        self.active_tab = self.tabs.len() - 1;
    }

    /// Close the tab at `idx`. No-op when there is only one tab.
    fn close_tab(&mut self, idx: usize) {
        if self.tabs.len() == 1 { return; }
        self.tabs.remove(idx); // PTY is dropped → SIGHUP sent to shell
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Move the tab at `from` so that it ends up just before `insert_before`.
    /// Indices are clamped; calling with a no-op position is safe.
    fn move_tab_to(&mut self, from: usize, insert_before: usize) {
        let n = self.tabs.len();
        if from >= n { return; }
        let insert_before = insert_before.min(n);
        // No movement needed.
        if insert_before == from || insert_before == from + 1 { return; }
        let tab = self.tabs.remove(from);
        // After the remove the insertion index shifts if we are moving rightward.
        let actual = if insert_before > from { insert_before - 1 } else { insert_before };
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


pub fn run(update_rx: std::sync::mpsc::Receiver<Option<String>>) {
    let cli = Cli::parse();
    let shell = cli.shell.unwrap_or_else(default_shell);

    // ── GPU path ─────────────────────────────────────────────────────────────
    let session = load_session();
    let window_width = session.window_width;
    let window_height = session.window_height;
    let state = Rc::new(RefCell::new(build_initial_state(
        cli.rows,
        cli.cols,
        cli.exec.as_deref(),
        &shell,
        session,
        update_rx,
    )));
    let (initial_font_path, initial_font_size) = {
        let s = state.borrow();
        (s.user_config.font.path.clone(), s.user_config.font.size)
    };
    let state_for_frames = Rc::clone(&state);
    let state_for_events = Rc::clone(&state);

    if let Err(err) = run_gpu_window_live_with_events(
        move || {
            let mut state = state_for_frames.borrow_mut();
            snapshot::build_snapshot(&mut state)
        },
        move |event| {
            let mut state = state_for_events.borrow_mut();
            input::handle_event(&mut state, event);
        },
        RenderConfig {
            initial_size: Some((window_width, window_height)),
            font: FontConfig {
                font_path: initial_font_path,
                font_size: initial_font_size,
            },
            ..RenderConfig::default()
        },
    ) {
        eprintln!("failed to start gpu backend: {err}");
    }

    // Persist session state so the next run can restore it.
    save_session(&state.borrow());
}

