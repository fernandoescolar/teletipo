#![doc = "GPUI renderer: alternative GPU rendering backend using the GPUI framework."]
#![warn(missing_docs)]
#![allow(missing_docs)]

use anyhow::Result;
use platform_abstraction::WindowControl;
use render_model::{AppWindowEvent, RenderConfig, RenderSnapshot};

/// Run a GPUI-based GPU rendering window with event handling and custom window setup.
///
/// This function provides the same interface as render-glow's counterpart, allowing
/// the application layer to remain backend-agnostic.
///
/// # Arguments
/// * `next_snapshot` - Closure that provides the next frame's render snapshot
/// * `on_event` - Closure that handles incoming window events
/// * `on_window_ready` - Closure called when the window is ready, receives the WindowControl handle
/// * `config` - Rendering configuration (initial size, position, opacity, etc.)
///
/// # Returns
/// * `Ok(())` if the window closed normally
/// * `Err(_)` if there was an error during window creation or rendering
pub fn run_gpu_window_live_with_events_and_window<F, E, W>(
    mut _next_snapshot: F,
    mut _on_event: E,
    _on_window_ready: W,
    _config: RenderConfig,
) -> Result<()>
where
    F: 'static + FnMut() -> RenderSnapshot,
    E: 'static + FnMut(AppWindowEvent),
    W: FnOnce(Box<dyn WindowControl>),
{
    // TODO: Implement GPUI window loop
    // This is a stub implementation that will be filled in with actual GPUI code.
    //
    // Steps:
    // 1. Create a GPUI app with the specified config
    // 2. Create a GpuiWindowControl implementing WindowControl
    // 3. Call on_window_ready with the window control
    // 4. Run the event loop, translating GPUI events to AppWindowEvent
    // 5. Call next_snapshot() each frame and pass to GPUI renderer
    // 6. Return when the window closes

    tracing::warn!("render-gpui: MVP implementation not yet available");
    Err(anyhow::anyhow!(
        "render-gpui backend is not yet implemented"
    ))
}

/// Convenience function: run with default event handler (no-op).
pub fn run_gpu_window_live<F>(next_snapshot: F, config: RenderConfig) -> Result<()>
where
    F: 'static + FnMut() -> RenderSnapshot,
{
    run_gpu_window_live_with_events_and_window(next_snapshot, |_| {}, |_| {}, config)
}

/// Convenience function: run a static snapshot without frame updates.
pub fn run_gpu_window(snapshot: RenderSnapshot, config: RenderConfig) -> Result<()> {
    run_gpu_window_live(move || snapshot.clone(), config)
}

/// Wrapper type for GPUI-specific context and state.
/// This allows backend-specific data to be stored without polluting the neutral API.
#[doc(hidden)]
pub struct GpuiBackendContext {
    // TODO: Add GPUI-specific state
    _phantom: std::marker::PhantomData<()>,
}
