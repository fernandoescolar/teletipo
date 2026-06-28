#![doc = "Glow renderer: winit + glutin + glow window loop and 2D painter."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod emoji;
mod font;
mod painter;
mod shaders;
mod types;
mod util;
mod wgpu_compat;
mod window;

pub use window::{
    Redrawer, run_gpu_window, run_gpu_window_live, run_gpu_window_live_with_events,
    run_gpu_window_live_with_events_and_window,
};

// Re-export types from wgpu_compat (types that were previously in render-wgpu)
pub use wgpu_compat::{
    AppWindowEvent, ColorTheme, CommandPalette, ContextMenu, DamageRegion, FontConfig,
    KeybindingRow, KeybindingsOverlay, PaneKind, PaneLayout, PipelineStage, RenderCell,
    RenderConfig, RenderRow, RenderSnapshot, RenderStats, SCROLLBAR_W_PX, SearchPanel,
    SettingsItem, SettingsOverlay, SuggestionDropdown, TerminalLink, Toast, ToastKind, VsyncMode,
    default_ansi_palette, snapshot_to_ime_area,
};
