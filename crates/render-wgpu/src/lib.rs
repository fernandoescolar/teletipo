mod atlas;
mod batch;
mod geometry;
mod pipeline;
mod renderer;
mod types;
mod window;

pub use batch::{Batch, BatchBuilder, CellQuad, FramePacer};
pub use atlas::{GlyphAtlas, GlyphEntry, GlyphKey};
pub use geometry::{
    snapshot_to_cell_quads, snapshot_to_cell_quads_in_bounds, snapshot_to_ime_area,
    SCROLLBAR_W_PX,
};
pub use renderer::{NullRenderer, Renderer, WgpuRenderer};
pub use types::{
    AppWindowEvent, ColorTheme, DamageRegion, FontConfig, PaneKind, PaneLayout, PipelineStage,
    RenderConfig, RenderSnapshot, RenderStats, SettingsItem, SettingsOverlay, SuggestionDropdown,
    TabContextMenu, VsyncMode, default_ansi_palette,
};
pub use window::{run_gpu_window, run_gpu_window_live, run_gpu_window_live_with_events};
