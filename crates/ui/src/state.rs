use crate::components::{
    BellState, CursorBlink, ModifierState, OverlayManager, PaneLayout, TabManager, UiConfig,
    WindowMetrics,
};

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
