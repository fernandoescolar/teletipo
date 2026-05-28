//! Application shell — thin abstraction over host-OS capabilities that the
//! input handlers need (clipboard today; window/redraw/file dialogs later).
//!
//! The goal is to keep `GpuRuntimeState` testable without requiring a live
//! windowing system or an `arboard::Clipboard` handle.
//!
//! Two implementations are provided:
//!
//! - [`SystemShell`] — real, backed by [`arboard`]. Lazily initialises the
//!   clipboard handle the first time it is used; if `arboard` fails (e.g.
//!   headless CI), subsequent calls silently no-op.
//! - [`NullShell`] — pure in-memory; intended for unit tests.

/// Capabilities the app needs from the host OS, abstracted so input handlers
/// can be exercised without a real window/clipboard.
pub(crate) trait AppShell {
    /// Read the system clipboard. Returns `None` on any backend error.
    fn clipboard_get(&mut self) -> Option<String>;

    /// Write `text` to the system clipboard. Silently no-ops on backend error.
    fn clipboard_set(&mut self, text: String);
}

/// Real shell backed by `arboard`. The clipboard handle is created lazily on
/// first use and re-used across calls (cheaper than `Clipboard::new()` per
/// keystroke, which is what the previous implementation did).
pub(crate) struct SystemShell {
    clipboard: Option<arboard::Clipboard>,
    clipboard_failed: bool,
}

impl SystemShell {
    pub(crate) fn new() -> Self {
        Self {
            clipboard: None,
            clipboard_failed: false,
        }
    }

    /// Returns a mutable reference to the cached clipboard, lazily creating it.
    /// Returns `None` once a previous init has failed (avoids retrying every call).
    fn clipboard_mut(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.clipboard_failed {
            return None;
        }
        if self.clipboard.is_none() {
            match arboard::Clipboard::new() {
                Ok(cb) => self.clipboard = Some(cb),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to initialise system clipboard");
                    self.clipboard_failed = true;
                    return None;
                }
            }
        }
        self.clipboard.as_mut()
    }
}

impl AppShell for SystemShell {
    fn clipboard_get(&mut self) -> Option<String> {
        let cb = self.clipboard_mut()?;
        cb.get_text().ok()
    }

    fn clipboard_set(&mut self, text: String) {
        if let Some(cb) = self.clipboard_mut() {
            let _ = cb.set_text(text);
        }
    }
}

/// In-memory shell for tests. The "clipboard" is just an `Option<String>`.
#[derive(Default)]
#[allow(dead_code)] // currently used only by tests; will be reached when GpuRuntimeState gets test coverage.
pub(crate) struct NullShell {
    clipboard: Option<String>,
}

#[cfg(test)]
impl NullShell {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl AppShell for NullShell {
    fn clipboard_get(&mut self) -> Option<String> {
        self.clipboard.clone()
    }

    fn clipboard_set(&mut self, text: String) {
        self.clipboard = Some(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_shell_clipboard_roundtrip() {
        let mut shell = NullShell::new();
        assert_eq!(shell.clipboard_get(), None);
        shell.clipboard_set("hello".to_string());
        assert_eq!(shell.clipboard_get().as_deref(), Some("hello"));
    }

    #[test]
    fn null_shell_overwrites_previous_value() {
        let mut shell = NullShell::new();
        shell.clipboard_set("a".to_string());
        shell.clipboard_set("b".to_string());
        assert_eq!(shell.clipboard_get().as_deref(), Some("b"));
    }
}
