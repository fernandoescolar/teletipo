#![doc = "Glow renderer: winit + glutin + glow window loop and 2D painter."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod emoji;
mod font;
mod painter;
mod shaders;
mod types;
mod util;
mod window;

pub use window::{
    run_gpu_window, run_gpu_window_live, run_gpu_window_live_with_events,
    run_gpu_window_live_with_events_and_window,
};

pub use render_wgpu::{
    Batch, BatchBuilder, CellQuad, ColorTheme, CommandPalette, ContextMenu, DamageRegion,
    GlyphAtlas, GlyphEntry, GlyphKey, NullRenderer, PaneKind, PaneLayout, PipelineStage,
    RenderCell, RenderError, RenderRow, RenderStats, Renderer, SCROLLBAR_W_PX, SearchPanel,
    SettingsItem, SettingsOverlay, SuggestionDropdown, TerminalLink, Toast, ToastKind, VsyncMode,
    WgpuRenderer, default_ansi_palette, snapshot_to_cell_quads, snapshot_to_cell_quads_in_bounds,
};
