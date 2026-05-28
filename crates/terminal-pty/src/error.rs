use std::io;

use thiserror::Error;

/// Errors produced by the [`crate::PortablePtySession`] API.
///
/// The `stage` field on [`PtyError::Pty`] identifies which step of the PTY
/// lifecycle failed (e.g. `"open pty"`, `"spawn command"`), enabling callers to
/// branch on a stable, human-readable label without inspecting the underlying
/// `portable_pty` error string.
#[derive(Debug, Error)]
pub enum PtyError {
    /// A `portable_pty` operation failed at the named stage.
    #[error("pty {stage}: {source}")]
    Pty {
        stage: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
    /// An I/O error occurred while reading from or writing to the PTY.
    #[error("pty io: {0}")]
    Io(#[from] io::Error),
}

impl PtyError {
    /// Helper used by session code to wrap underlying `portable_pty` errors.
    ///
    /// Accepts any source convertible into a boxed std error, including
    /// `anyhow::Error` (which `portable_pty` uses internally).
    pub(crate) fn stage<E>(stage: &'static str, source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        Self::Pty {
            stage,
            source: source.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_error_display_includes_stage() {
        let inner = io::Error::other("boom");
        let err = PtyError::stage("open pty", inner);
        let msg = err.to_string();
        assert!(msg.contains("open pty"), "got: {msg}");
        assert!(msg.contains("boom"), "got: {msg}");
    }

    #[test]
    fn pty_error_io_variant_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe");
        let err: PtyError = io_err.into();
        assert!(matches!(err, PtyError::Io(_)));
    }
}
