mod builder;
pub mod components;
mod event;
mod layout;
mod overlays;
mod scene;
mod screen;
mod snapshot;
mod stats;
mod theme;

pub use builder::{RenderContext, build_scene};
pub use components::{Background, Editor, Terminal, overlay};
pub use event::AppWindowEvent;
pub use layout::{
    CellMetrics, FrameLayout, RenderTarget, SEPARATOR_WIDTH_PX, TAB_HEIGHT_MULTIPLIER,
    compute_frame_layout,
};
pub use overlays::{
    CommandPalette, ContextMenu, KeybindingRow, KeybindingsOverlay, SettingsItem, SettingsOverlay,
    StickyCommandOverlay, SuggestionDropdown, Toast, ToastKind,
};
pub use scene::{
    Color, EmojiCommand, Rect, RectCommand, RenderCommand, Scene, SceneLayer, TextCommand,
    TextStyle,
};
pub use screen::{DamageRegion, RenderCell, RenderRow};
pub use snapshot::{RenderSnapshot, SearchPanel, SnapshotImage, TerminalLink};
pub use stats::RenderStats;
pub use theme::{
    ColorTheme, FontConfig, PaneKind, PaneLayout, PipelineStage, RenderConfig, SCROLLBAR_W_PX,
    VsyncMode, default_ansi_palette, snapshot_to_ime_area,
};
