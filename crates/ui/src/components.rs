use app_orchestrator::App;
use std::time::Instant;
use terminal_pty::PortablePtySession;

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

#[derive(Debug, Clone, Default)]
pub struct SuggestionState {
    pub prefix: Option<String>,
    pub index: Option<usize>,
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BellState {
    pub flash_until: Option<Instant>,
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
