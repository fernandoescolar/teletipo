#![doc = "WGPU renderer, geometry conversion, and GPU window integration."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod atlas;
mod batch;
mod error;
mod geometry;
mod glyph_raster;
mod pipeline;
mod renderer;
pub mod shell_highlight;
mod surface;
mod types;
mod window;

pub use atlas::{GlyphAtlas, GlyphEntry, GlyphKey};
pub use batch::{Batch, BatchBuilder, CellQuad, FramePacer};
pub use error::RenderError;
pub use geometry::{
    SCROLLBAR_W_PX, snapshot_to_cell_quads, snapshot_to_cell_quads_in_bounds, snapshot_to_ime_area,
};
pub use renderer::{NullRenderer, Renderer, WgpuRenderer};
pub use types::{
    AppWindowEvent, ColorTheme, CommandPalette, ContextMenu, DamageRegion, FontConfig, PaneKind,
    PaneLayout, PipelineStage, RenderCell, RenderConfig, RenderRow, RenderSnapshot, RenderStats,
    SearchPanel, SettingsItem, SettingsOverlay, SuggestionDropdown, TerminalLink, Toast, ToastKind,
    VsyncMode, default_ansi_palette,
};
pub use window::{
    run_gpu_window, run_gpu_window_live, run_gpu_window_live_with_events,
    run_gpu_window_live_with_events_and_window,
};
