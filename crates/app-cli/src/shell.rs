//! Application shell — thin abstraction over host-OS capabilities that the
//! input handlers need (clipboard today; window/redraw/file dialogs later).
//!
//! The goal is to keep `GpuRuntimeState` testable without requiring a live
//! windowing system or a real OS clipboard.
//!
//! Two implementations are provided:
//!
//! - [`SystemShell`] — real, backed by [`platform_abstraction::SystemClipboard`]
//!   for the clipboard and by a [`WindowControl`] handle (installed at startup)
//!   for window-level operations. If the OS clipboard backend fails (e.g.
//!   headless CI), calls silently no-op.
//! - [`NullShell`] — pure in-memory; intended for unit tests.

use platform_abstraction::{Accessibility, Clipboard, SystemClipboard, WindowControl};
use std::sync::Arc;

/// Capabilities the app needs from the host OS, abstracted so input handlers
/// can be exercised without a real window/clipboard.
pub(crate) trait AppShell {
    /// Read the system clipboard. Returns `None` on any backend error.
    fn clipboard_get(&mut self) -> Option<String>;

    /// Write `text` to the system clipboard. Silently no-ops on backend error.
    fn clipboard_set(&mut self, text: String);

    /// Open `url` with the OS default handler.
    fn open_url(&mut self, url: &str);

    /// Send an OS notification with a title and body. Default: no-op.
    fn notify(&mut self, _title: &str, _body: &str) {}

    /// Install a [`WindowControl`] handle so the shell can forward
    /// `open_url` and `notify` calls to the real window. Called once during
    /// startup after the event loop is ready.
    /// Default: drops the handle (used by [`NullShell`] in tests).
    fn install_window(&mut self, _window: Arc<dyn WindowControl>) {}

    /// Push a fresh semantic accessibility tree to the platform's AT layer.
    /// Default: no-op.
    fn update_accessibility_tree(&mut self, _tree: &platform_abstraction::AccessibilityTree) {}
}

/// Real shell. The clipboard is delegated to
/// [`platform_abstraction::SystemClipboard`], which lazily creates an
/// `arboard` handle on first use and re-uses it across calls. Window-level
/// operations are forwarded to a [`WindowControl`] installed via
/// [`AppShell::install_window`] at startup.
pub(crate) struct SystemShell {
    clipboard: SystemClipboard,
    window: Option<Arc<dyn WindowControl>>,
    accessibility: platform_abstraction::NativePlatformServices,
}

impl SystemShell {
    pub(crate) fn new() -> Self {
        Self {
            clipboard: SystemClipboard::default(),
            window: None,
            accessibility: platform_abstraction::native_services(),
        }
    }
}

impl AppShell for SystemShell {
    fn clipboard_get(&mut self) -> Option<String> {
        self.clipboard.get()
    }

    fn clipboard_set(&mut self, text: String) {
        self.clipboard.set(text);
    }

    fn open_url(&mut self, url: &str) {
        if let Some(w) = self.window.as_ref() {
            w.open_url(url);
        }
    }

    fn notify(&mut self, title: &str, body: &str) {
        if let Some(w) = self.window.as_ref() {
            w.notify(title, body);
        }
    }

    fn install_window(&mut self, window: Arc<dyn WindowControl>) {
        self.window = Some(window);
    }

    fn update_accessibility_tree(&mut self, tree: &platform_abstraction::AccessibilityTree) {
        self.accessibility.accessibility.update_tree(tree);
    }
}

/// In-memory shell for tests. The "clipboard" is just an `Option<String>`.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct NullShell {
    clipboard: Option<String>,
    /// Most recent URL passed to [`AppShell::open_url`].
    last_url: Option<String>,
}

#[cfg(test)]
impl NullShell {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn last_url(&self) -> Option<&str> {
        self.last_url.as_deref()
    }
}

#[cfg(test)]
impl AppShell for NullShell {
    fn clipboard_get(&mut self) -> Option<String> {
        self.clipboard.clone()
    }

    fn clipboard_set(&mut self, text: String) {
        self.clipboard = Some(text);
    }

    fn open_url(&mut self, url: &str) {
        self.last_url = Some(url.to_owned());
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

    #[test]
    fn null_shell_records_open_url() {
        let mut shell = NullShell::new();
        shell.open_url("https://example.com");
        assert_eq!(shell.last_url(), Some("https://example.com"));
    }
}
