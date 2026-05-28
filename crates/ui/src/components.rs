use app_orchestrator::App;
use std::time::Instant;
use terminal_pty::PortablePtySession;

use crate::actions::SettingsCmd;

#[derive(Debug, Clone)]
pub struct UiConfig {
    pub padding_horizontal: f32,
    pub padding_vertical: f32,
    pub active_theme_idx: Option<usize>,
    pub active_font_idx: usize,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            padding_horizontal: 8.0,
            padding_vertical: 8.0,
            active_theme_idx: None,
            active_font_idx: 0,
        }
    }
}

pub struct TabPane {
    pub app: App,
    pub pty: Option<PortablePtySession>,
    pub scroll: ScrollState,
    pub terminal_selection: SelectionState,
    pub editor_selection: SelectionState,
    pub split_ratio: f32,
    pub is_terminal_fullscreen: bool,
    pub pre_fullscreen_split_ratio: f32,
    pub cwd_label: String,
    pub suggestions: SuggestionState,
}

impl TabPane {
    pub fn new(app: App, pty: Option<PortablePtySession>, cwd_label: String) -> Self {
        Self {
            app,
            pty,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollState {
    pub terminal_offset: usize,
    pub editor_offset: usize,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            terminal_offset: 0,
            editor_offset: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    pub row: usize,
    pub col: usize,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    pub anchor: Option<SelectionPoint>,
    pub end: Option<SelectionPoint>,
    pub in_progress: bool,
}

impl SelectionState {
    pub fn begin(&mut self, point: SelectionPoint) {
        self.anchor = Some(point);
        self.end = Some(point);
        self.in_progress = true;
    }

    pub fn update(&mut self, point: SelectionPoint) {
        if self.in_progress {
            self.end = Some(point);
        }
    }

    pub fn finalize(&mut self) {
        self.in_progress = false;
    }

    pub fn clear(&mut self) {
        self.anchor = None;
        self.end = None;
        self.in_progress = false;
    }
}

#[derive(Debug, Clone, Default)]
pub struct SuggestionState {
    pub prefix: Option<String>,
    pub index: Option<usize>,
}

impl SuggestionState {
    pub fn clear(&mut self) {
        self.prefix = None;
        self.index = None;
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

pub struct TabManager {
    pub tabs: Vec<TabPane>,
    pub active: usize,
    pub drag: Option<DragState>,
    pub context_menu: Option<ContextMenuState>,
}

impl TabManager {
    pub fn new(initial: TabPane) -> Self {
        Self {
            tabs: vec![initial],
            active: 0,
            drag: None,
            context_menu: None,
        }
    }

    pub fn active_tab(&self) -> &TabPane {
        &self.tabs[self.active]
    }

    pub fn active_tab_mut(&mut self) -> &mut TabPane {
        &mut self.tabs[self.active]
    }

    pub fn open_new(&mut self, tab: TabPane) {
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

#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    pub open: bool,
    pub cursor: usize,
    pub edit_buf: Option<String>,
    pub dirty: bool,
    pub just_saved: bool,
    pub search_buf: Option<String>,
    pub search_selected: usize,
    pub search_scroll_offset: usize,
}

impl SettingsState {
    pub fn apply(&mut self, action: SettingsCmd) {
        match action {
            SettingsCmd::MoveUp => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            SettingsCmd::MoveDown => {
                self.cursor = self.cursor.saturating_add(1);
            }
            SettingsCmd::MoveLeft | SettingsCmd::MoveRight => {}
            SettingsCmd::PageUp | SettingsCmd::Home => {
                self.cursor = 0;
                self.search_scroll_offset = 0;
            }
            SettingsCmd::PageDown | SettingsCmd::End => {
                self.cursor = self.cursor.saturating_add(10);
                self.search_scroll_offset = self.search_scroll_offset.saturating_add(10);
            }
            SettingsCmd::BeginEdit => {
                self.edit_buf = Some(String::new());
            }
            SettingsCmd::InsertChar(ch) => {
                if let Some(buf) = self.edit_buf.as_mut() {
                    buf.push(ch);
                    self.dirty = true;
                }
            }
            SettingsCmd::Backspace => {
                if let Some(buf) = self.edit_buf.as_mut() {
                    buf.pop();
                    self.dirty = true;
                }
            }
            SettingsCmd::Delete => {
                self.edit_buf = Some(String::new());
                self.dirty = true;
            }
            SettingsCmd::CommitEdit => {
                self.edit_buf = None;
            }
            SettingsCmd::CancelEdit => {
                self.edit_buf = None;
                self.dirty = false;
            }
            SettingsCmd::Save => {
                self.just_saved = true;
                self.dirty = false;
            }
            SettingsCmd::OpenSearch => {
                self.search_buf = Some(String::new());
                self.search_selected = 0;
            }
            SettingsCmd::CloseSearch => {
                self.search_buf = None;
                self.search_selected = 0;
                self.search_scroll_offset = 0;
            }
            SettingsCmd::SearchInsertChar(ch) => {
                if let Some(buf) = self.search_buf.as_mut() {
                    buf.push(ch);
                }
            }
            SettingsCmd::SearchBackspace => {
                if let Some(buf) = self.search_buf.as_mut() {
                    buf.pop();
                }
            }
            SettingsCmd::SearchNext => {
                self.search_selected = self.search_selected.saturating_add(1);
            }
            SettingsCmd::SearchPrev => {
                self.search_selected = self.search_selected.saturating_sub(1);
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OverlayManager {
    pub settings: SettingsState,
    pub context_menu: Option<ContextMenuState>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorBlink {
    pub last_toggle: Instant,
    pub phase: bool,
}

impl Default for CursorBlink {
    fn default() -> Self {
        Self {
            last_toggle: Instant::now(),
            phase: true,
        }
    }
}

impl CursorBlink {
    pub fn tick(&mut self) {
        const BLINK_HALF_MS: u128 = 500;
        if self.last_toggle.elapsed().as_millis() >= BLINK_HALF_MS {
            self.phase = !self.phase;
            self.last_toggle = Instant::now();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BellState {
    pub flash_until: Option<Instant>,
}

impl BellState {
    pub fn is_active(&self) -> bool {
        self.flash_until
            .map(|deadline| Instant::now() < deadline)
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifierState {
    pub ctrl: bool,
    pub super_key: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowMetrics {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_factor: f64,
    pub cell_w: f32,
    pub cell_h: f32,
    pub cursor_x: f64,
    pub cursor_y: f64,
}

impl Default for WindowMetrics {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            x: 0,
            y: 0,
            scale_factor: 1.0,
            cell_w: 9.0,
            cell_h: 18.0,
            cursor_x: 0.0,
            cursor_y: 0.0,
        }
    }
}
