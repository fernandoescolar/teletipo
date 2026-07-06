#![doc = "GPUI renderer: alternative GPU rendering backend using the GPUI framework."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod input;
mod window;
mod scene;
mod text;
mod color;
mod geom;

use std::sync::{Arc, Mutex, atomic::AtomicBool};

use anyhow::Result;
use gpui::{App, Application, AppContext};
use platform_abstraction::WindowControl;
use render_model::{AppWindowEvent, RenderConfig, RenderSnapshot};

pub use window::{GpuiWindowControl, GpuiView, window_options};

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
    next_snapshot: F,
    on_event: E,
    on_window_ready: W,
    config: RenderConfig,
) -> Result<()>
where
    F: 'static + FnMut() -> RenderSnapshot,
    E: 'static + FnMut(AppWindowEvent),
    W: 'static + FnOnce(Box<dyn WindowControl>),
{
    let redraw_requested = Arc::new(AtomicBool::new(true));
    let pending_title = Arc::new(Mutex::new(None));
    let control = GpuiWindowControl {
        redraw_requested: redraw_requested.clone(),
        pending_title: pending_title.clone(),
    };

    Application::new().run(move |cx: &mut App| {
        let options = window_options(&config, cx);
        let mut on_window_ready = Some(on_window_ready);
        let mut next_snapshot = Some(next_snapshot);
        let mut on_event = Some(on_event);
        let redraw_requested = redraw_requested.clone();
        let pending_title = pending_title.clone();

        cx.open_window(options, move |_, cx| {
            if let Some(on_window_ready) = on_window_ready.take() {
                on_window_ready(Box::new(control));
            }

            cx.new(|_| GpuiView {
                next_snapshot: Box::new(next_snapshot.take().expect("snapshot callback used once")),
                on_event: Box::new(on_event.take().expect("event callback used once")),
                redraw_requested,
                pending_title,
                config,
                last_title: String::new(),
                last_modifiers: platform_abstraction::ModifierKeys::default(),
            })
        })
        .expect("failed to open GPUI window");
        cx.activate(true);
    });

    Ok(())
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

