use crate::config::UserConfig;
use crate::input;
use crate::launch::{build_app, spawn_pty};
use crate::layout::LayoutMetrics;
use crate::settings::SettingsUiState;
use crate::shell;
use crate::snapshot;
use crate::state::{
    CursorState, DragState, LayoutState, ModalOverlay, ModifierState, OverlayState, ThemeFontState,
};
use crate::tab::{HistoryEntry, TabState};
use platform_abstraction::WindowControl;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use tracing::{debug, warn};

#[derive(Clone)]
pub(crate) struct EventCtx {
    state: Rc<RefCell<GpuRuntimeState>>,
}

impl EventCtx {
    pub(crate) fn new(state: Rc<RefCell<GpuRuntimeState>>) -> Self {
        Self { state }
    }

    pub(crate) fn build_snapshot(&self) -> render_wgpu::RenderSnapshot {
        let frame_start = Instant::now();
        let mut state = self.state.borrow_mut();
        let snapshot = snapshot::build_snapshot(&mut state);
        ::metrics::histogram!("frame_us").record(frame_start.elapsed().as_secs_f64() * 1_000_000.0);
        snapshot
    }

    pub(crate) fn handle_event(&self, event: render_wgpu::AppWindowEvent) {
        let mut state = self.state.borrow_mut();
        input::handle_event(&mut state, event);
    }

    pub(crate) fn install_window(&self, window: Box<dyn WindowControl>) {
        self.state
            .borrow_mut()
            .shell_services
            .install_window(window);
    }
}

pub(crate) struct GpuRuntimeState {
    pub(crate) tabs: Vec<TabState>,
    pub(crate) active_tab: usize,
    /// Shell executable used to spawn new PTY sessions.
    pub(crate) shell: String,
    pub(crate) modifiers: ModifierState,
    pub(crate) layout: LayoutState,
    pub(crate) cursor: CursorState,
    pub(crate) drag: DragState,
    pub(crate) overlays: OverlayState,
    /// Loaded user configuration (applied per frame to the render snapshot).
    pub(crate) user_config: UserConfig,
    /// Startup config error, if the config file could not be loaded cleanly.
    pub(crate) config_error: Option<String>,
    pub(crate) themes_fonts: ThemeFontState,
    /// Receiver for the background update-check result (consumed once after the
    /// check completes; set to `None` afterwards).
    pub(crate) update_rx: Option<std::sync::mpsc::Receiver<Result<Option<String>, String>>>,
    /// Settings overlay interaction state.
    pub(crate) settings: SettingsUiState,
    /// Command palette overlay (Cmd+Shift+P). `None` when the palette is closed.
    pub(crate) command_palette: Option<crate::state::CommandPaletteState>,
    /// Set to `true` when the last shell session ends so the window closes.
    pub(crate) should_exit: bool,
    /// Abstraction over host-OS capabilities (clipboard today). Boxed so tests
    /// can swap in a [`shell::NullShell`].
    pub(crate) shell_services: Box<dyn shell::AppShell>,
}

impl GpuRuntimeState {
    pub(crate) fn tab(&self) -> &TabState {
        &self.tabs[self.active_tab]
    }

    pub(crate) fn tab_mut(&mut self) -> &mut TabState {
        &mut self.tabs[self.active_tab]
    }

    /// Open the settings modal and close any other active modal.
    pub(crate) fn open_settings_modal(&mut self) {
        self.command_palette = None;
        self.settings.open = true;
        self.settings.cursor = 0;
        self.settings.edit_buf = None;
        self.overlays.active_modal = Some(ModalOverlay::Settings);
    }

    /// Open the command palette modal and close any other active modal.
    pub(crate) fn open_command_palette_modal(&mut self, cp: crate::state::CommandPaletteState) {
        self.settings.open = false;
        self.settings.search_buf = None;
        self.settings.edit_buf = None;
        self.command_palette = Some(cp);
        self.overlays.active_modal = Some(ModalOverlay::CommandPalette);
    }

    /// Close the settings modal if open.
    pub(crate) fn close_settings_modal(&mut self) {
        self.settings.open = false;
        if self.overlays.active_modal == Some(ModalOverlay::Settings) {
            self.overlays.active_modal = None;
        }
    }

    /// Close the command palette modal if open.
    pub(crate) fn close_command_palette_modal(&mut self) {
        self.command_palette = None;
        if self.overlays.active_modal == Some(ModalOverlay::CommandPalette) {
            self.overlays.active_modal = None;
        }
    }

    /// Close whichever modal is currently active.
    pub(crate) fn close_active_modal(&mut self) {
        match self.overlays.active_modal {
            Some(ModalOverlay::Settings) => self.close_settings_modal(),
            Some(ModalOverlay::CommandPalette) => self.close_command_palette_modal(),
            None => {}
        }
    }

    /// Height of the tab bar in pixels. Hidden when only one tab is open.
    pub(crate) fn tab_bar_h(&self) -> f32 {
        if self.tabs.len() > 1 {
            self.layout.cell_h
        } else {
            0.0
        }
    }

    /// Pump PTY output for ALL tabs; returns `true` if the active tab received data.
    #[allow(clippy::too_many_lines)] // sequential housekeeping over every tab
    pub(crate) fn pump_all_ptys(&mut self) -> bool {
        let mut active_had_data = false;
        let active = self.active_tab;
        let mut dead_tabs: Vec<usize> = Vec::new();
        let mut exit_codes: Vec<(usize, i32)> = Vec::new();
        let mut resize_tabs: Vec<usize> = Vec::new();
        let mut pty_status: Option<String> = None;
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
            if i != active && had_data {
                tab.unread_output = true;
            }
            // Refresh the command-running flag via a cheap `tcgetpgrp` on
            // the PTY master.  True when the shell has spawned a foreground
            // child (vim, sudo, a script, …) and is waiting for it to exit.
            if let Some(ref pty_ref) = tab.pty {
                tab.command_running = pty_ref.foreground_child_running();
            } else {
                tab.command_running = false;
            }
            if tab.app.take_bell() && self.user_config.terminal.bell {
                self.overlays.bell_flash_until =
                    Some(Instant::now() + std::time::Duration::from_millis(150));
                if i != active {
                    tab.bell_pending = true;
                }
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
        // Commit pending commands for every shell-reported exit code so failed
        // commands remain available in history for quick retry.
        for (idx, code) in exit_codes {
            self.finalize_pending_cmd(idx, code);
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
            if pty_status.is_none() {
                pty_status = Some(if self.tabs.len() == 1 {
                    "PTY closed".to_owned()
                } else {
                    format!("PTY closed in tab {}", idx + 1)
                });
            }
            if self.tabs.len() == 1 {
                self.should_exit = true;
            } else {
                self.close_tab(idx);
            }
        }
        if let Some(message) = pty_status {
            self.overlays.pty_status = Some((Instant::now(), message));
        }
        active_had_data
    }

    pub(crate) fn send_terminal_input(&mut self, bytes: &[u8]) {
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
    /// Called when the shell reports an exit code via OSC 133.
    pub(crate) fn finalize_pending_cmd(&mut self, tab_idx: usize, _exit_code: i32) {
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
            tab.history_entries.push(HistoryEntry {
                cmd: text.clone(),
                count: 1,
                last_used_secs: now_secs,
            });
        }
        tab.history.push(text);
    }

    pub(crate) fn run_editor_command(&mut self) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        let text = tab.app.editor_snapshot();
        let text = text.trim().to_string();
        if !text.is_empty() {
            if tab.shell_integration {
                // Defer: save to history only after the shell reports exit code 0.
                tab.pending_cmd = Some(text);
            } else {
                // No integration - save immediately (original behaviour).
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
                    tab.history_entries.push(HistoryEntry {
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

    pub(crate) fn history_prev(&mut self) {
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

    pub(crate) fn history_next(&mut self) {
        let active = self.active_tab;
        let tab = &mut self.tabs[active];
        let Some(idx) = tab.history_index else {
            return;
        };
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

    pub(crate) fn resize_tab(&mut self, idx: usize, rows: u16, cols: u16) {
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
    pub(crate) fn resize_all_tabs(&mut self) {
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

    pub(crate) fn add_new_tab(&mut self) {
        let split_ratio = self.tab().split_ratio;
        // After adding the new tab there will be at least 2 tabs, so the tab bar
        // will appear and steal one cell row - account for that when sizing the PTY.
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
            search: crate::search::SearchState::default(),
            command_running: false,
            unread_output: false,
            bell_pending: false,
        });
        self.active_tab = self.tabs.len() - 1;
    }

    /// Close the tab at `idx`. No-op when there is only one tab.
    pub(crate) fn close_tab(&mut self, idx: usize) {
        if self.tabs.len() == 1 {
            return;
        }
        self.tabs.remove(idx); // PTY is dropped -> SIGHUP sent to shell
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Move the tab at `from` so that it ends up just before `insert_before`.
    /// Indices are clamped; calling with a no-op position is safe.
    pub(crate) fn move_tab_to(&mut self, from: usize, insert_before: usize) {
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

    /// Push a transient toast notification visible for `secs` seconds.
    pub(crate) fn push_toast(&mut self, text: impl Into<String>, kind: crate::state::ToastKind) {
        use std::time::Duration;
        self.overlays.toasts.push_back(crate::state::Toast::new(
            text,
            kind,
            Duration::from_secs(4),
        ));
    }
}
