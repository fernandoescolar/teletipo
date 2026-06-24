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

// Re-exports removed - render-wgpu has been deleted in favor of glow-only rendering
// Public exports are now only from window module and internal types
