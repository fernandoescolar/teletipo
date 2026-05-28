use crate::actions::{EditorCmd, UiAction};
use crate::components::{
    BellState, CursorBlink, ModifierState, OverlayManager, PaneLayout, SelectionPoint, TabManager,
    TabPane, UiConfig, WindowMetrics,
};
use app_orchestrator::App;

const DEFAULT_ROWS: usize = 24;
const DEFAULT_COLS: usize = 80;

pub struct UiState {
    pub tabs: TabManager,
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
}

impl UiState {
    pub fn new(shell: String, config: UiConfig) -> Result<Self, String> {
        let app = App::new(DEFAULT_ROWS, DEFAULT_COLS).map_err(|err| err.to_string())?;
        let initial_tab = TabPane::new(app, None, String::new());
        Ok(Self {
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
        })
    }

    pub fn apply_action(&mut self, action: UiAction) {
        match action {
            UiAction::NewTab => {
                if let Ok(app) = App::new(DEFAULT_ROWS, DEFAULT_COLS) {
                    self.tabs.open_new(TabPane::new(app, None, String::new()));
                }
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
                let tab = self.tabs.active_tab_mut();
                if let Some(mut pty) = tab.pty.take() {
                    let _ = tab.app.send_pty_input(&mut pty, &bytes);
                    tab.pty = Some(pty);
                }
            }
            UiAction::EditorInsert(text) => {
                self.tabs.active_tab_mut().app.insert_editor_input(&text);
            }
            UiAction::EditorAction(cmd) => {
                let tab = self.tabs.active_tab_mut();
                match cmd {
                    EditorCmd::Backspace => tab.app.editor_backspace(),
                    EditorCmd::DeleteForward => tab.app.editor_delete_forward(),
                    EditorCmd::MoveLeft { extend_selection } => {
                        tab.app.editor_move_cursor_left(extend_selection)
                    }
                    EditorCmd::MoveRight { extend_selection } => {
                        tab.app.editor_move_cursor_right(extend_selection)
                    }
                    EditorCmd::SetCursor {
                        offset,
                        extend_selection,
                    } => tab.app.set_editor_cursor(offset, extend_selection),
                    EditorCmd::Undo => tab.app.editor_undo(),
                    EditorCmd::Redo => tab.app.editor_redo(),
                    EditorCmd::Clear => tab.app.editor_clear(),
                }
            }
            UiAction::SendReturn => {
                let tab = self.tabs.active_tab_mut();
                if let Some(mut pty) = tab.pty.take() {
                    let _ = tab.app.run_editor_command(&mut pty, true);
                    tab.pty = Some(pty);
                }
            }
            UiAction::ScrollBy(delta) => {
                let tab = self.tabs.active_tab_mut();
                if delta > 0 {
                    tab.scroll.terminal_offset =
                        tab.scroll.terminal_offset.saturating_add(delta as usize);
                } else {
                    tab.scroll.terminal_offset =
                        tab.scroll.terminal_offset.saturating_sub(delta.unsigned_abs() as usize);
                }
            }
            UiAction::ScrollTo(offset) => {
                self.tabs.active_tab_mut().scroll.terminal_offset = offset;
            }
            UiAction::EditorScrollBy(delta) => {
                let tab = self.tabs.active_tab_mut();
                if delta > 0 {
                    tab.scroll.editor_offset = tab.scroll.editor_offset.saturating_add(delta as usize);
                } else {
                    tab.scroll.editor_offset =
                        tab.scroll.editor_offset.saturating_sub(delta.unsigned_abs() as usize);
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
                self.tabs.active_tab_mut().app.insert_editor_input(&text);
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
}
