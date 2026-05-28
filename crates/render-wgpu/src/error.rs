use thiserror::Error;

/// Errors returned by the public [`crate::run_gpu_window`] family of functions.
///
/// Internally the renderer uses `anyhow` while interacting with `wgpu` and
/// `winit`, but the public boundary exposes a stable, typed enum so binaries
/// can match on the failure mode (event loop creation, surface configuration,
/// runtime render errors) and react appropriately.
#[derive(Debug, Error)]
pub enum RenderError {
    /// Failed to create the OS event loop (winit).
    #[error("event loop: {0}")]
    EventLoop(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    /// Failed to create or initialise the OS window.
    #[error("window: {0}")]
    Window(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    /// Failed to initialise the GPU (adapter/device/surface).
    #[error("gpu init: {0}")]
    GpuInit(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
    /// A runtime render error occurred while pumping frames.
    #[error("render: {0}")]
    Render(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl RenderError {
    pub(crate) fn event_loop<E>(source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self::EventLoop(source.into())
    }

    pub(crate) fn window<E>(source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self::Window(source.into())
    }

    pub(crate) fn gpu_init<E>(source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self::GpuInit(source.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_error_display_prefix() {
        let err = RenderError::event_loop(anyhow::anyhow!("boom"));
        assert!(err.to_string().starts_with("event loop:"));
    }
}
