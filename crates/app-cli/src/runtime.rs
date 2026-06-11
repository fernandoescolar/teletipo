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
use platform_abstraction::{AccessNode, AccessibilityTree, WindowControl};
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
    /// When the most recent update check was spawned (for daily re-scheduling).
    pub(crate) update_last_checked: Instant,
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
    #[allow(clippy::cognitive_complexity)] // sequential housekeeping over every tab
    pub(crate) fn pump_all_ptys(&mut self) -> bool {
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
        // Capture any blocks that finished this pump while their row numbers are
        // still valid (a later window resize would reflow and invalidate them).
        let n = self.tabs.len();
        for idx in 0..n {
            self.capture_completed_blocks(idx);
        }
        if !resize_tabs.is_empty() {
            self.apply_fullscreen_resize_tabs(resize_tabs);
        }
        let pty_status = self.process_dead_tabs(dead_tabs);
        if let Some(message) = pty_status {
            self.overlays.pty_status = Some((Instant::now(), message));
        }

        // Push a fresh accessibility tree whenever the active tab's screen
        // content has changed.
        if active_had_data {
            let version = self.tabs[active].app.terminal_screen_version();
            if version != self.tabs[active].a11y_screen_version {
                self.push_accessibility_tree();
            }
        }

        active_had_data
    }

    fn apply_fullscreen_resize_tabs(&mut self, resize_tabs: Vec<usize>) {
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

    fn process_dead_tabs(&mut self, dead_tabs: Vec<usize>) -> Option<String> {
        let mut pty_status: Option<String> = None;
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
        pty_status
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

    fn record_history_command(&mut self, text: String) {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        for tab in &mut self.tabs {
            // Keep every execution in chronological order, including duplicates,
            // matching conventional shell history navigation.
            tab.history.push(text.clone());
            if let Some(entry) = tab
                .history_entries
                .iter_mut()
                .find(|entry| entry.cmd.trim() == text.as_str())
            {
                entry.count = entry.count.saturating_add(1);
                entry.last_used_secs = now_secs;
            } else {
                tab.history_entries.push(HistoryEntry {
                    cmd: text.clone(),
                    count: 1,
                    last_used_secs: now_secs,
                });
            }
        }
    }

    /// Commit `pending_cmd` (if any) for `tab_idx` to shared history.
    /// Called when the shell reports an exit code via OSC 133.
    pub(crate) fn finalize_pending_cmd(&mut self, tab_idx: usize, _exit_code: i32) {
        let Some(text) = self.tabs[tab_idx].pending_cmd.take() else {
            return;
        };
        if !text.is_empty() {
            self.record_history_command(text);
        }
    }

    pub(crate) fn run_editor_command(&mut self) {
        let active = self.active_tab;
        let text = self.tabs[active].app.editor_snapshot().trim().to_string();
        if !text.is_empty() {
            if self.tabs[active].shell_integration {
                self.tabs[active].pending_cmd = Some(text);
            } else {
                self.record_history_command(text);
            }
        }
        let tab = &mut self.tabs[active];
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
        tab.editor_horizontal_scroll_offset = 0;
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
        tab.editor_horizontal_scroll_offset = 0;
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
            tab.editor_horizontal_scroll_offset = 0;
        } else {
            tab.history_index = None;
            let saved = tab.saved_input.clone();
            tab.app.editor_clear();
            tab.app.insert_editor_input(&saved);
        }
    }

    pub(crate) fn jump_to_prev_prompt(&mut self) {
        self.jump_to_prompt(false);
    }

    pub(crate) fn jump_to_next_prompt(&mut self) {
        self.jump_to_prompt(true);
    }

    fn jump_to_prompt(&mut self, forward: bool) {
        let tab = self.tab_mut();
        let blocks: Vec<(app_orchestrator::BlockId, usize)> = tab
            .app
            .execution_blocks()
            .iter()
            .map(|block| (block.id, block.prompt_start_row))
            .collect();
        if blocks.is_empty() {
            self.push_toast("No command blocks yet", crate::state::ToastKind::Info);
            return;
        }

        let target_idx = match tab
            .selected_block
            .and_then(|id| blocks.iter().position(|b| b.0 == id))
        {
            Some(idx) if forward => idx.checked_add(1).filter(|idx| *idx < blocks.len()),
            Some(idx) => idx.checked_sub(1),
            None if forward => Some(0),
            None => Some(blocks.len() - 1),
        };
        let Some((target_id, target_row)) = target_idx.map(|idx| blocks[idx]) else {
            self.push_toast(
                if forward {
                    "No later command block"
                } else {
                    "No earlier command block"
                },
                crate::state::ToastKind::Info,
            );
            return;
        };

        tab.scroll_offset = crate::snapshot::tab_scroll_offset_to_center(tab, target_row);
        tab.selected_block = Some(target_id);
        tab.selection_anchor = None;
        tab.selection_end = None;
        tab.is_selecting = false;
    }

    /// Select the newest command block whose prompt begins at or before a
    /// viewport row. This makes right-clicking anywhere in a block target it.
    pub(crate) fn select_block_at_view_row(&mut self, view_row: usize) -> bool {
        let tab = self.tab_mut();
        let absolute_row = crate::snapshot::tab_view_row_to_abs(tab, view_row);
        let selected = tab
            .app
            .execution_blocks()
            .iter()
            .chain(tab.app.current_execution_block())
            .filter(|block| block.prompt_start_row <= absolute_row)
            .max_by_key(|block| block.prompt_start_row)
            .map(|block| block.id);
        tab.selected_block = selected;
        selected.is_some()
    }

    /// Handle a left click on a command-block row.
    ///
    /// - Click on a collapsed placeholder row → expand the block.
    /// - Click on a header row inside the toolbar → run the action.
    /// - Click anywhere else on a header row → select the block.
    ///
    /// Returns `true` when the click was consumed.
    pub(crate) fn activate_block_quick_action(
        &mut self,
        view_row: usize,
        col: usize,
        cols: usize,
    ) -> bool {
        let absolute_row = crate::snapshot::tab_view_row_to_abs(self.tab(), view_row);
        let tab = self.tab();
        let placeholder_id = tab
            .app
            .execution_blocks()
            .iter()
            .chain(tab.app.current_execution_block())
            .find(|block| {
                tab.collapsed_blocks.contains(&block.id)
                    && block.output_start_row == Some(absolute_row)
            })
            .map(|block| block.id);
        if let Some(id) = placeholder_id {
            let tab = self.tab_mut();
            tab.selected_block = Some(id);
            tab.collapsed_blocks.remove(&id);
            return true;
        }

        let header_id = tab
            .app
            .execution_blocks()
            .iter()
            .chain(tab.app.current_execution_block())
            .find(|block| block.prompt_start_row == absolute_row)
            .map(|block| block.id);
        let Some(id) = header_id else {
            return false;
        };
        self.tab_mut().selected_block = Some(id);

        if cols >= BLOCK_TOOLBAR_WIDTH && col + BLOCK_TOOLBAR_WIDTH >= cols {
            let offset = col - (cols - BLOCK_TOOLBAR_WIDTH);
            match block_toolbar_action(offset) {
                Some(BlockToolbarAction::Rerun) => self.rerun_selected_block_command(),
                Some(BlockToolbarAction::Edit) => self.edit_selected_block_command(),
                Some(BlockToolbarAction::Copy) => self.copy_selected_block_command(),
                Some(BlockToolbarAction::ToggleFold) => self.toggle_selected_block_collapse(),
                None => {}
            }
        }
        true
    }

    fn selected_block_command(&self) -> Option<String> {
        let tab = self.tab();
        let id = tab.selected_block?;
        tab.app.execution_block(id)?.command.clone()
    }

    pub(crate) fn copy_selected_block_command(&mut self) {
        let Some(command) = self.selected_block_command() else {
            self.push_toast(
                "Select a command block first",
                crate::state::ToastKind::Info,
            );
            return;
        };
        self.shell_services.clipboard_set(command);
        self.push_toast("Command copied", crate::state::ToastKind::Success);
    }

    pub(crate) fn copy_selected_block_output(&mut self) {
        let output = {
            let tab = self.tab();
            tab.selected_block.and_then(|id| tab.app.block_output(id))
        };
        let Some(output) = output else {
            self.push_toast(
                "Selected block output is unavailable",
                crate::state::ToastKind::Info,
            );
            return;
        };
        self.shell_services.clipboard_set(output);
        self.push_toast("Output copied", crate::state::ToastKind::Success);
    }

    pub(crate) fn edit_selected_block_command(&mut self) {
        let Some(command) = self.selected_block_command() else {
            self.push_toast(
                "Select a command block first",
                crate::state::ToastKind::Info,
            );
            return;
        };
        let tab = self.tab_mut();
        tab.app.editor_clear();
        tab.app.insert_editor_input(&command);
        tab.editor_scroll_offset = 0;
        tab.editor_horizontal_scroll_offset = 0;
    }

    pub(crate) fn rerun_selected_block_command(&mut self) {
        if self.tab().command_running {
            self.push_toast(
                "Cannot re-run while a command is active",
                crate::state::ToastKind::Warn,
            );
            return;
        }
        if !self.tab().app.editor_snapshot().is_empty() {
            self.push_toast(
                "Editor is not empty; use Edit Selected Command",
                crate::state::ToastKind::Warn,
            );
            return;
        }
        self.edit_selected_block_command();
        self.run_editor_command();
    }

    pub(crate) fn collapse_all_command_blocks(&mut self) {
        let collapsible: Vec<_> = self
            .tab()
            .app
            .execution_blocks()
            .iter()
            .filter(|block| {
                block.output_start_row.is_some() && block.output_end_row.is_some()
            })
            .map(|block| block.id)
            .collect();
        if collapsible.is_empty() {
            self.push_toast(
                "No command blocks with output to collapse",
                crate::state::ToastKind::Info,
            );
            return;
        }
        self.tab_mut().collapsed_blocks.extend(collapsible);
    }

    pub(crate) fn toggle_selected_block_collapse(&mut self) {
        let id = {
            let tab = self.tab();
            let Some(id) = tab.selected_block else {
                self.push_toast(
                    "Select a command block first",
                    crate::state::ToastKind::Info,
                );
                return;
            };
            id
        };
        let tab = self.tab_mut();
        if !tab.collapsed_blocks.insert(id) {
            tab.collapsed_blocks.remove(&id);
        }
    }

    pub(crate) fn resize_tab(&mut self, idx: usize, rows: u16, cols: u16) {
        let tab = &mut self.tabs[idx];
        if let Some(pty) = tab.pty.as_mut() {
            pty.resize(rows, cols);
        }
        tab.app.resize_terminal(rows as usize, cols as usize);
        // The terminal now has its real size: replay any restored session
        // content so its layout — and the command-block row numbers — match
        // the live geometry instead of the startup placeholder size.
        self.flush_pending_restore(idx);
        let tab = &mut self.tabs[idx];
        let max_scroll = tab.app.scrollback_len();
        if tab.scroll_offset > max_scroll {
            tab.scroll_offset = max_scroll;
        }
    }

    /// Capture the text of any blocks that have completed since the last call,
    /// storing it in `saved_blocks` for session persistence.  Done while the
    /// block's rows are still valid (before any later resize reflows them).
    pub(crate) fn capture_completed_blocks(&mut self, idx: usize) {
        let tab = &self.tabs[idx];
        // The in-progress block (if any) is not yet finished, so only the
        // settled `execution_blocks()` are eligible for capture.
        let total = tab.app.execution_blocks().len();
        if total <= tab.captured_block_count {
            return;
        }
        let dump = tab.app.terminal_ansi_snapshot();
        let wrap_flags = tab.app.terminal_wrap_flags();
        let lines: Vec<&str> = dump.lines().collect();
        // Build logical lines: join consecutive soft-wrapped rows without a
        // newline so the content reflows to the live column width on restore.
        let slice = |start: usize, end: usize| -> String {
            let end = end.min(lines.len());
            if start >= end {
                return String::new();
            }
            let mut out = String::new();
            for i in start..end {
                out.push_str(lines[i]);
                let is_wrapped = wrap_flags.get(i).copied().unwrap_or(false);
                if !is_wrapped && i + 1 < end {
                    out.push('\n');
                }
            }
            out
        };
        let mut captured = Vec::new();
        for block in &tab.app.execution_blocks()[tab.captured_block_count..total] {
            let prompt_start = block.prompt_start_row;
            let command_start = block.command_start_row.unwrap_or(prompt_start);
            let output_start = block.output_start_row.unwrap_or(command_start);
            let output_end = block.output_end_row.unwrap_or(output_start);
            captured.push(crate::tab::SavedBlock {
                prompt_ansi: slice(prompt_start, command_start),
                command_ansi: slice(command_start, output_start),
                output_ansi: slice(output_start, output_end),
                command: block.command.clone(),
                exit_code: block.exit_code,
                duration_ms: block.duration.map(|d| d.as_millis() as u64),
                started_at_secs: block.started_at.and_then(|t| {
                    t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
                }),
                cwd: block.cwd.as_ref().map(|p| p.to_string_lossy().into_owned()),
            });
        }
        let tab = &mut self.tabs[idx];
        tab.saved_blocks.extend(captured);
        tab.captured_block_count = total;
    }

    /// Replay deferred restored content into a tab's terminal at its current
    /// size.  Blocks are reconstructed through a synthetic OSC 133 stream so
    /// their row numbers are recomputed from scratch at the live geometry.
    /// No-op when the tab has nothing pending.  See [`crate::tab::PendingRestore`].
    pub(crate) fn flush_pending_restore(&mut self, idx: usize) {
        let Some(pending) = self.tabs[idx].pending_restore.take() else {
            return;
        };
        let app = &mut self.tabs[idx].app;
        // Home the cursor and clear the grid first: resizing the (empty)
        // terminal to its real size can leave the cursor padded down from the
        // top, which would otherwise offset every restored row.
        app.feed_terminal(b"\x1b[H\x1b[2J");

        let feed_region = |app: &mut app_orchestrator::App, region: &str| {
            for line in region.split('\n') {
                app.feed_terminal(line.as_bytes());
                app.feed_terminal(b"\r\n");
            }
        };

        if pending.blocks.is_empty() {
            // No structured blocks — replay the plain scrollback blob.
            for line in pending.terminal_output.lines() {
                app.feed_terminal(line.as_bytes());
                app.feed_terminal(b"\r\n");
            }
        } else {
            for block in &pending.blocks {
                app.feed_terminal(b"\x1b]133;A\x07");
                if !block.prompt_ansi.is_empty() {
                    feed_region(app, &block.prompt_ansi);
                }
                app.feed_terminal(b"\x1b]133;B\x07");
                if let Some(cmd) = &block.command {
                    app.terminal.register_submitted_command(cmd.clone());
                }
                if !block.command_ansi.is_empty() {
                    feed_region(app, &block.command_ansi);
                }
                app.feed_terminal(b"\x1b]133;C\x07");
                if !block.output_ansi.is_empty() {
                    feed_region(app, &block.output_ansi);
                }
                let code = block.exit_code.unwrap_or(0);
                app.feed_terminal(format!("\x1b]133;D;{code}\x07").as_bytes());
            }
        }

        // Adopt the restored blocks as the capture baseline so they persist on
        // the next save and newly run blocks append after them.
        let tab = &mut self.tabs[idx];
        tab.saved_blocks = pending.blocks;
        tab.captured_block_count = tab.app.execution_blocks().len();
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
        self.add_new_tab_with_shell(None);
    }

    pub(crate) fn add_new_tab_with_shell(&mut self, shell_override: Option<&str>) {
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
        let history = self.tab().history.clone();
        let history_entries = self.tab().history_entries.clone();
        let chosen_shell = shell_override
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.shell);
        let (pty, integration) = spawn_pty(chosen_shell, rows, cols, None, Some(&active_cwd))
            .map(|(p, i)| (Some(p), i))
            .unwrap_or((None, false));
        self.tabs.push(TabState {
            app,
            pty,
            scroll_offset: 0,
            editor_scroll_offset: 0,
            editor_horizontal_scroll_offset: 0,
            history,
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
            history_entries,
            pending_cmd: None,
            shell_integration: integration,
            search: crate::search::SearchState::default(),
            command_running: false,
            unread_output: false,
            bell_pending: false,
            a11y_screen_version: 0,
            selected_block: None,
            collapsed_blocks: std::collections::HashSet::new(),
            virtual_scrollback_lines: 0,
            collapsed_hidden_ranges: Vec::new(),
            last_frame_v_start: 0,
            pending_restore: None,
            saved_blocks: Vec::new(),
            captured_block_count: 0,
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

    /// Build and push a fresh semantic accessibility tree to the platform AT layer.
    ///
    /// Call this whenever the visible terminal content may have changed: new
    /// PTY output, scroll position change, or active-tab switch.  The method
    /// is cheap to call repeatedly; the platform layer (VoiceOver) only
    /// announces nodes that differ from the previous push.
    pub(crate) fn push_accessibility_tree(&mut self) {
        let active = self.active_tab;
        let tree = build_accessibility_tree(&self.tabs, active);
        self.shell_services.update_accessibility_tree(&tree);
        // Record the version so pump_all_ptys skips redundant pushes.
        if let Some(tab) = self.tabs.get_mut(active) {
            tab.a11y_screen_version = tab.app.terminal_screen_version();
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

// ── Command-block quick-action toolbar ────────────────────────────────────────
//
// The toolbar is drawn right-aligned in every block header row and hit-tested
// by column.  Layout (offsets within the toolbar, width 25):
//
//   ▶ run  edit  copy  ↓ fold
//   0───4  7──10  13─16  19───24
//
// `block_toolbar_text` and `block_toolbar_action` must stay in sync: the
// snapshot draws the text, the pointer handler maps clicks back to actions.

/// Column width of the block header quick-action toolbar.
pub(crate) const BLOCK_TOOLBAR_WIDTH: usize = 25;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BlockToolbarAction {
    Rerun,
    Edit,
    Copy,
    ToggleFold,
}

/// Toolbar text for a block header (uses only glyphs guaranteed by both
/// renderers).  Exactly [`BLOCK_TOOLBAR_WIDTH`] characters.
pub(crate) fn block_toolbar_text(collapsed: bool) -> &'static str {
    if collapsed {
        "▶ run  edit  copy  → open"
    } else {
        "▶ run  edit  copy  ↓ fold"
    }
}

/// Map a column offset inside the toolbar to the action under it.
pub(crate) fn block_toolbar_action(offset: usize) -> Option<BlockToolbarAction> {
    match offset {
        0..=4 => Some(BlockToolbarAction::Rerun),
        7..=10 => Some(BlockToolbarAction::Edit),
        13..=16 => Some(BlockToolbarAction::Copy),
        19..=24 => Some(BlockToolbarAction::ToggleFold),
        _ => None,
    }
}

// ── Accessibility tree builder ────────────────────────────────────────────────

/// Build the semantic [`AccessibilityTree`] for the currently active tab,
/// including tab-bar nodes for all tabs.
///
/// Called from [`GpuRuntimeState::push_accessibility_tree`] whenever the
/// terminal content, scroll position, or active tab changes.
fn build_accessibility_tree(tabs: &[TabState], active_tab: usize) -> AccessibilityTree {
    let tab = &tabs[active_tab];
    let scroll_offset = tab.scroll_offset;
    let mut nodes: Vec<AccessNode> = Vec::new();

    // ── Tab bar ───────────────────────────────────────────────────────────────
    for (i, t) in tabs.iter().enumerate() {
        let label = if t.cwd.is_empty() {
            format!("Tab {}", i + 1)
        } else {
            // Show only the last path component as the tab label.
            std::path::Path::new(&t.cwd)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&t.cwd)
                .to_owned()
        };
        nodes.push(AccessNode::Tab {
            index: i,
            label,
            active: i == active_tab,
        });
    }

    // ── Terminal viewport ─────────────────────────────────────────────────────
    {
        let text = tab.app.terminal.snapshot_text();
        // Derive cols from the first non-empty line of the snapshot; fall back
        // to a safe default so the AT always gets a plausible grid size.
        let cols = text.lines().next().map(|l| l.chars().count()).unwrap_or(80);
        nodes.push(AccessNode::Terminal {
            rows: tab.term_row_count,
            cols: cols.max(1),
            text,
        });
    }

    // ── Completed structured command blocks ────────────────────────────────
    {
        for block in tab.app.execution_blocks() {
            let Some(command_text) = block.command.clone().filter(|command| !command.is_empty())
            else {
                continue;
            };
            nodes.push(AccessNode::CommandZone {
                prompt_row: block.prompt_start_row,
                command_text,
                exit_code: block.exit_code,
                output_text: tab.app.block_output(block.id).unwrap_or_default(),
            });
        }
    }

    // ── OSC 8 hyperlinks visible at current scroll position ───────────────────
    {
        let text = tab.app.terminal.snapshot_text(); // one allocation, reused below
        let spans = tab.app.terminal.hyperlink_spans(scroll_offset);
        for (row, col_start, col_end, id) in spans {
            if let Some(uri) = tab.app.terminal.hyperlink_uri(id) {
                let label = text
                    .lines()
                    .nth(row)
                    .map(|l| {
                        l.chars()
                            .skip(col_start)
                            .take(col_end.saturating_sub(col_start))
                            .collect::<String>()
                    })
                    .unwrap_or_default();
                nodes.push(AccessNode::Hyperlink {
                    row,
                    col_start,
                    col_end,
                    label,
                    uri: uri.to_owned(),
                });
            }
        }
    }

    AccessibilityTree { nodes }
}
