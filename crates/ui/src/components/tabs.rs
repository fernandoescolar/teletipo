//! Tabs, drag/context-menu state, and pane layout.

use crate::tab_backend::TabBackend;

use super::selection::{ScrollState, SelectionState, SuggestionState};

pub struct TabPane<B: TabBackend> {
    pub backend: B,
    pub scroll: ScrollState,
    pub terminal_selection: SelectionState,
    pub editor_selection: SelectionState,
    pub split_ratio: f32,
    pub is_terminal_fullscreen: bool,
    pub pre_fullscreen_split_ratio: f32,
    pub cwd_label: String,
    pub suggestions: SuggestionState,
}

impl<B: TabBackend> TabPane<B> {
    pub fn new(backend: B, cwd_label: String) -> Self {
        Self {
            backend,
            scroll: ScrollState::default(),
            terminal_selection: SelectionState::default(),
            editor_selection: SelectionState::default(),
            split_ratio: 0.7,
            is_terminal_fullscreen: false,
            pre_fullscreen_split_ratio: 0.7,
            cwd_label,
            suggestions: SuggestionState::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragState {
    pub tab_index: usize,
    pub start_x: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextMenuState {
    pub tab_index: usize,
    pub x: f64,
    pub y: f64,
    pub hovered_item: Option<usize>,
}

pub struct TabManager<B: TabBackend> {
    pub tabs: Vec<TabPane<B>>,
    pub active: usize,
    pub drag: Option<DragState>,
    pub context_menu: Option<ContextMenuState>,
}

impl<B: TabBackend> TabManager<B> {
    pub fn new(initial: TabPane<B>) -> Self {
        Self {
            tabs: vec![initial],
            active: 0,
            drag: None,
            context_menu: None,
        }
    }

    pub fn active_tab(&self) -> &TabPane<B> {
        &self.tabs[self.active]
    }

    pub fn active_tab_mut(&mut self) -> &mut TabPane<B> {
        &mut self.tabs[self.active]
    }

    pub fn open_new(&mut self, tab: TabPane<B>) {
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    pub fn close(&mut self, idx: usize) {
        if self.tabs.len() <= 1 || idx >= self.tabs.len() {
            return;
        }
        self.tabs.remove(idx);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if self.active > idx {
            self.active -= 1;
        }
    }

    pub fn switch(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active = idx;
        }
    }

    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() {
            return;
        }
        let insert_at = to.min(self.tabs.len());
        let tab = self.tabs.remove(from);
        let adjusted = if from < insert_at {
            insert_at.saturating_sub(1)
        } else {
            insert_at
        }
        .min(self.tabs.len());
        self.tabs.insert(adjusted, tab);
        self.active = adjusted;
    }

    pub fn start_drag(&mut self, tab_index: usize, start_x: f64) {
        self.drag = Some(DragState { tab_index, start_x });
    }

    pub fn end_drag(&mut self) {
        self.drag = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneFocus {
    Terminal,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneLayout {
    pub focus: PaneFocus,
    pub split_ratio: f32,
    pub terminal_fullscreen: bool,
}

impl Default for PaneLayout {
    fn default() -> Self {
        Self {
            focus: PaneFocus::Editor,
            split_ratio: 0.7,
            terminal_fullscreen: false,
        }
    }
}

impl PaneLayout {
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            PaneFocus::Terminal => PaneFocus::Editor,
            PaneFocus::Editor => PaneFocus::Terminal,
        };
    }

    pub fn set_split_ratio(&mut self, ratio: f32) {
        self.split_ratio = ratio.clamp(0.2, 0.85);
    }

    pub fn toggle_fullscreen(&mut self) {
        self.terminal_fullscreen = !self.terminal_fullscreen;
        if self.terminal_fullscreen {
            self.split_ratio = 1.0;
        }
    }
}
