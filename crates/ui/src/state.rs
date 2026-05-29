use crate::actions::{EditorCmd, SettingsCmd, UiAction};
use crate::components::{
    BellState, CursorBlink, ModifierState, OverlayManager, PaneLayout, SelectionPoint, TabManager,
    TabPane, UiConfig, WindowMetrics,
};
use crate::tab_backend::TabBackend;

pub struct UiState<B: TabBackend> {
    pub tabs: TabManager<B>,
    pub shell: String,
    pub layout: PaneLayout,
    pub overlays: OverlayManager,
    pub modifiers: ModifierState,
    pub window: WindowMetrics,
    pub cursor_blink: CursorBlink,
    pub bell: BellState,
    pub should_exit: bool,
    pub config: UiConfig,
    pub pending_update: Option<String>,
    tab_factory: Box<dyn Fn() -> TabPane<B>>,
}

impl<B: TabBackend> UiState<B> {
    pub fn new(
        shell: String,
        config: UiConfig,
        initial_tab: TabPane<B>,
        tab_factory: Box<dyn Fn() -> TabPane<B>>,
    ) -> Self {
        Self {
            tabs: TabManager::new(initial_tab),
            shell,
            layout: PaneLayout::default(),
            overlays: OverlayManager::default(),
            modifiers: ModifierState::default(),
            window: WindowMetrics::default(),
            cursor_blink: CursorBlink::default(),
            bell: BellState::default(),
            should_exit: false,
            config,
            pending_update: None,
            tab_factory,
        }
    }

    #[allow(clippy::too_many_lines)] // dispatcher: long flat match arms by design
    pub fn apply_action(&mut self, action: UiAction) {
        match action {
            UiAction::NewTab => {
                let new_tab = (self.tab_factory)();
                self.tabs.open_new(new_tab);
            }
            UiAction::CloseTab(idx) => {
                self.tabs.close(idx);
            }
            UiAction::SwitchTab(idx) => {
                self.tabs.switch(idx);
            }
            UiAction::MoveTab { from, to } => {
                self.tabs.move_tab(from, to);
            }
            UiAction::DragTabStart(idx) => {
                self.tabs.start_drag(idx, self.window.cursor_x);
            }
            UiAction::DragTabUpdate { .. } => {}
            UiAction::DragTabEnd => {
                self.tabs.end_drag();
            }
            UiAction::OpenTabContextMenu { tab, x, y } => {
                self.tabs.context_menu = Some(crate::components::ContextMenuState {
                    tab_index: tab,
                    x,
                    y,
                    hovered_item: None,
                });
            }
            UiAction::CloseContextMenu => {
                self.tabs.context_menu = None;
            }
            UiAction::ContextMenuHover(hover) => {
                if let Some(menu) = self.tabs.context_menu.as_mut() {
                    menu.hovered_item = hover;
                }
            }
            UiAction::RenameTab(_, _) => {}
            UiAction::ToggleFocus => {
                self.layout.toggle_focus();
            }
            UiAction::SetSplitRatio(ratio) => {
                self.layout.set_split_ratio(ratio);
                self.tabs.active_tab_mut().split_ratio = self.layout.split_ratio;
            }
            UiAction::ToggleFullscreen => {
                self.layout.toggle_fullscreen();
                self.tabs.active_tab_mut().is_terminal_fullscreen = self.layout.terminal_fullscreen;
            }
            UiAction::SendToTerminal(bytes) => {
                self.tabs.active_tab_mut().backend.send_bytes(&bytes);
            }
            UiAction::EditorInsert(text) => {
                self.tabs.active_tab_mut().backend.insert_text(&text);
            }
            UiAction::EditorAction(cmd) => {
                let tab = self.tabs.active_tab_mut();
                match cmd {
                    EditorCmd::Backspace => tab.backend.backspace(),
                    EditorCmd::DeleteForward => tab.backend.delete_forward(),
                    EditorCmd::MoveLeft { extend_selection } => {
                        tab.backend.move_cursor_left(extend_selection)
                    }
                    EditorCmd::MoveRight { extend_selection } => {
                        tab.backend.move_cursor_right(extend_selection)
                    }
                    EditorCmd::SetCursor {
                        offset,
                        extend_selection,
                    } => tab.backend.set_cursor(offset, extend_selection),
                    EditorCmd::Undo => tab.backend.undo(),
                    EditorCmd::Redo => tab.backend.redo(),
                    EditorCmd::Clear => tab.backend.clear_editor(),
                }
            }
            UiAction::SendReturn => {
                let tab = self.tabs.active_tab_mut();
                let text = tab.backend.editor_snapshot();
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    tab.backend.record_history(&trimmed);
                }
                tab.backend.run_command(true);
            }
            UiAction::ScrollBy(delta) => {
                let tab = self.tabs.active_tab_mut();
                if delta > 0 {
                    tab.scroll.terminal_offset =
                        tab.scroll.terminal_offset.saturating_add(delta as usize);
                } else {
                    tab.scroll.terminal_offset = tab
                        .scroll
                        .terminal_offset
                        .saturating_sub(delta.unsigned_abs() as usize);
                }
            }
            UiAction::ScrollTo(offset) => {
                self.tabs.active_tab_mut().scroll.terminal_offset = offset;
            }
            UiAction::EditorScrollBy(delta) => {
                let tab = self.tabs.active_tab_mut();
                if delta > 0 {
                    tab.scroll.editor_offset =
                        tab.scroll.editor_offset.saturating_add(delta as usize);
                } else {
                    tab.scroll.editor_offset = tab
                        .scroll
                        .editor_offset
                        .saturating_sub(delta.unsigned_abs() as usize);
                }
            }
            UiAction::SelectionBegin { row, col } => {
                let offset = self.tabs.active_tab().scroll.terminal_offset;
                self.tabs
                    .active_tab_mut()
                    .terminal_selection
                    .begin(SelectionPoint {
                        row,
                        col,
                        scroll_offset: offset,
                    });
            }
            UiAction::SelectionUpdate { row, col } => {
                let offset = self.tabs.active_tab().scroll.terminal_offset;
                self.tabs
                    .active_tab_mut()
                    .terminal_selection
                    .update(SelectionPoint {
                        row,
                        col,
                        scroll_offset: offset,
                    });
            }
            UiAction::SelectionEnd => {
                self.tabs.active_tab_mut().terminal_selection.finalize();
            }
            UiAction::ClearSelection => {
                self.tabs.active_tab_mut().terminal_selection.clear();
            }
            UiAction::CopySelection => {}
            UiAction::OpenSettings => {
                self.overlays.settings.open = true;
            }
            UiAction::CloseSettings => {
                self.overlays.settings.open = false;
            }
            UiAction::SettingsAction(action) => {
                // Intercept actions that need access to UiConfig (cycling, commit, begin-edit).
                match &action {
                    SettingsCmd::MoveLeft | SettingsCmd::MoveRight => {
                        let is_right = matches!(&action, SettingsCmd::MoveRight);
                        let cursor = self.overlays.settings.cursor;
                        if let Some(field) = crate::config::SETTINGS_FIELDS.get(cursor) {
                            if field.key == "theme" && !self.config.available_themes.is_empty() {
                                let n = self.config.available_themes.len();
                                let cur = self.config.active_theme_idx.unwrap_or(0);
                                let next = if is_right {
                                    (cur + 1) % n
                                } else if cur == 0 {
                                    n - 1
                                } else {
                                    cur - 1
                                };
                                self.config.active_theme_idx = Some(next);
                                self.config.active_theme =
                                    Some(self.config.available_themes[next].clone());
                                self.overlays.settings.dirty = true;
                            } else if field.section == "font"
                                && field.key == "family"
                                && !self.config.available_fonts.is_empty()
                            {
                                let n = self.config.available_fonts.len();
                                let cur = self.config.active_font_idx;
                                let next = if is_right {
                                    (cur + 1) % n
                                } else if cur == 0 {
                                    n - 1
                                } else {
                                    cur - 1
                                };
                                self.config.active_font_idx = next;
                                self.config.font_family = if next == 0 {
                                    None
                                } else {
                                    self.config.available_fonts.get(next).cloned()
                                };
                                self.overlays.settings.dirty = true;
                            } else if let Some(step) =
                                crate::config::numeric_step(field.section, field.key)
                            {
                                let raw = self.config.get_field(field.section, field.key);
                                let current: f32 = raw.parse().unwrap_or(0.0);
                                let delta = if is_right { step } else { -step };
                                let new_val = if step.fract() == 0.0 {
                                    format!("{}", (current + delta).max(0.0) as u32)
                                } else {
                                    format!("{:.1}", (current + delta).max(0.0))
                                };
                                self.config.set_field(field.section, field.key, &new_val);
                                self.overlays.settings.dirty = true;
                            }
                        }
                    }
                    SettingsCmd::BeginEdit => {
                        let cursor = self.overlays.settings.cursor;
                        if let Some(field) = crate::config::SETTINGS_FIELDS.get(cursor) {
                            let current = self.config.get_field(field.section, field.key);
                            let initial = if current == "(auto)"
                                || current == "(default)"
                                || current == "(none)"
                            {
                                String::new()
                            } else {
                                current
                            };
                            self.overlays.settings.edit_buf = Some(initial);
                        }
                        return; // handled — don't forward to SettingsState::apply
                    }
                    SettingsCmd::CommitEdit => {
                        if let Some(buf) = self.overlays.settings.edit_buf.take() {
                            let cursor = self.overlays.settings.cursor;
                            if let Some(field) = crate::config::SETTINGS_FIELDS.get(cursor) {
                                self.config.set_field(field.section, field.key, &buf);
                                self.overlays.settings.dirty = true;
                            }
                        }
                        return; // handled
                    }
                    _ => {}
                }
                self.overlays.settings.apply(action);
            }
            UiAction::CycleSuggestion(_) => {}
            UiAction::AcceptSuggestion => {
                self.tabs.active_tab_mut().suggestions.clear();
            }
            UiAction::ClearSuggestion => {
                self.tabs.active_tab_mut().suggestions.clear();
            }
            UiAction::Paste(text) => {
                self.tabs.active_tab_mut().backend.insert_text(&text);
            }
            UiAction::RequestExit => {
                self.should_exit = true;
            }
            UiAction::ToggleCursorBlink => {
                self.cursor_blink.tick();
            }
            UiAction::Resized {
                width,
                height,
                scale,
                cell_w,
                cell_h,
            } => {
                self.window.width = width;
                self.window.height = height;
                self.window.scale_factor = scale;
                self.window.cell_w = cell_w;
                self.window.cell_h = cell_h;
            }
            UiAction::CursorMoved { x, y } => {
                self.window.cursor_x = x;
                self.window.cursor_y = y;
            }
            UiAction::MouseWheel(delta_lines) => {
                if delta_lines > 0.0 {
                    self.tabs.active_tab_mut().scroll.terminal_offset = self
                        .tabs
                        .active_tab()
                        .scroll
                        .terminal_offset
                        .saturating_add(delta_lines as usize);
                } else {
                    self.tabs.active_tab_mut().scroll.terminal_offset = self
                        .tabs
                        .active_tab()
                        .scroll
                        .terminal_offset
                        .saturating_sub((-delta_lines) as usize);
                }
            }
            UiAction::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
            }
        }
    }

    /// Advance cursor blink and pump PTY output for all tabs.
    /// Returns `true` if the active tab received new data.
    pub fn tick(&mut self) -> bool {
        self.cursor_blink.tick();

        let mut active_had_data = false;
        let active = self.tabs.active;
        let mut dead_tabs: Vec<usize> = Vec::new();
        let mut fullscreen_to_enter: Vec<usize> = Vec::new();
        let mut fullscreen_to_exit: Vec<usize> = Vec::new();
        let mut flash_bell = false;
        let bell_enabled = self.config.terminal_bell;

        for i in 0..self.tabs.tabs.len() {
            let tab = &mut self.tabs.tabs[i];
            if !tab.backend.has_pty() {
                continue;
            }
            let had_data = tab.backend.pump().map(|n| n > 0).unwrap_or(false);
            for response in tab.backend.drain_responses() {
                tab.backend.send_bytes(response.as_bytes());
            }
            let is_dead = tab.backend.is_dead();

            if i == active && had_data {
                active_had_data = true;
            }
            if tab.backend.take_bell() && bell_enabled {
                flash_bell = true;
            }
            let now_fullscreen = tab.backend.is_alternate_screen();
            if now_fullscreen != tab.is_terminal_fullscreen {
                if now_fullscreen {
                    fullscreen_to_enter.push(i);
                } else {
                    fullscreen_to_exit.push(i);
                }
            }
            if is_dead {
                dead_tabs.push(i);
            }
        }

        if flash_bell {
            self.bell.flash_until =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(150));
        }

        if active_had_data {
            let tab = &mut self.tabs.tabs[active];
            tab.scroll.terminal_offset = 0;
            self.cursor_blink.phase = true;
        }

        // Apply fullscreen transitions and resize PTYs.
        let available_h = self.window.height as f32;
        let cell_h = self.window.cell_h;
        let cell_w = self.window.cell_w;
        let pad_h = self.config.padding_horizontal;
        let pad_v = self.config.padding_vertical;
        let win_w = self.window.width;
        let cols = ((win_w as f32 - 2.0 * pad_h) / cell_w).max(1.0) as u16;

        for i in fullscreen_to_enter {
            let tab = &mut self.tabs.tabs[i];
            tab.is_terminal_fullscreen = true;
            tab.pre_fullscreen_split_ratio = tab.split_ratio;
            tab.split_ratio = 1.0;
            tab.scroll.terminal_offset = 0;
            let term_h = (available_h - 2.0 * pad_v).max(cell_h);
            let rows = (term_h / cell_h).max(1.0) as u16;
            tab.backend.resize(rows, cols);
        }
        for i in fullscreen_to_exit {
            let tab = &mut self.tabs.tabs[i];
            tab.is_terminal_fullscreen = false;
            tab.split_ratio = tab.pre_fullscreen_split_ratio.clamp(0.2, 0.85);
            let term_h = (available_h * tab.split_ratio - 2.0 * pad_v).max(cell_h);
            let rows = (term_h / cell_h).max(1.0) as u16;
            tab.backend.resize(rows, cols);
        }

        // Close dead tabs (or set should_exit if it was the last one).
        for &idx in dead_tabs.iter().rev() {
            if self.tabs.tabs.len() == 1 {
                self.should_exit = true;
            } else {
                self.tabs.close(idx);
            }
        }

        active_had_data
    }
}
