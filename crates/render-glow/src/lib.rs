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

// Window functions require shared types that were in render-wgpu
// This is a temporary state while the crate is being cleaned up
// pub use window::{
//     run_gpu_window, run_gpu_window_live, run_gpu_window_live_with_events,
//     run_gpu_window_live_with_events_and_window,
// };
