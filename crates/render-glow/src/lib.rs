#![doc = "Glow renderer: winit + glutin + glow window loop and 2D painter."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod backend;
mod batch;
mod emoji;
mod emoji_atlas;
mod font;
mod glyph_atlas;
mod painter;
mod pipelines;
mod shaders;
mod types;
mod util;
mod window;
mod winit_compat;

pub use window::{
    Redrawer, run_gpu_window, run_gpu_window_live, run_gpu_window_live_with_events,
    run_gpu_window_live_with_events_and_window,
};

// Re-export only the types explicitly needed for backend API compatibility.
// Callers should import render_model types directly for application code.
pub use render_model::{AppWindowEvent, RenderConfig, RenderSnapshot};
