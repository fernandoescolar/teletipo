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

/// Capabilities the app needs from the host OS, abstracted so input handlers
/// can be exercised without a real window/clipboard.
pub(crate) trait AppShell {
    /// Read the system clipboard. Returns `None` on any backend error.
    fn clipboard_get(&mut self) -> Option<String>;

    /// Write `text` to the system clipboard. Silently no-ops on backend error.
    fn clipboard_set(&mut self, text: String);

    /// Ask the host window to schedule a redraw. Default: no-op.
    #[allow(dead_code)] // plumbing in place; no caller yet (see T9).
    fn request_redraw(&mut self) {}

    /// Set the host window's title bar text. Default: no-op.
    #[allow(dead_code)] // plumbing in place; no caller yet (see T9).
    fn set_title(&mut self, _title: &str) {}

    /// Open `url` with the OS default handler. Default: no-op.
    #[allow(dead_code)] // plumbing in place; no caller yet (see T9).
    fn open_url(&mut self, _url: &str) {}

    /// Send an OS notification with a title and body. Default: no-op.
    fn notify(&mut self, _title: &str, _body: &str) {}

    /// Install a [`WindowControl`] implementation so the shell can forward
    /// redraw/title/open-url calls to the real window. Called once during
    /// startup by the GPU backend before the event loop pumps any events.
    /// Default: drops the handle (used by [`NullShell`] in tests).
    fn install_window(&mut self, _window: Box<dyn WindowControl>) {}

    /// Announce a short text string to the active screen reader.
    /// Default: no-op. Real implementation in [`SystemShell`].
    #[allow(dead_code)]
    fn announce(&mut self, _text: &str) {}

    /// Push a fresh semantic accessibility tree to the platform's AT layer.
    /// Default: no-op.
    fn update_accessibility_tree(&mut self, _tree: &platform_abstraction::AccessibilityTree) {}
}

/// Real shell. The clipboard is delegated to
/// [`platform_abstraction::SystemClipboard`], which lazily creates an
/// `arboard` handle on first use and re-uses it across calls. Window-level
/// operations (redraw / title / open-url) are forwarded to a
/// [`WindowControl`] installed via [`AppShell::install_window`] at startup.
pub(crate) struct SystemShell {
    clipboard: SystemClipboard,
    window: Option<Box<dyn WindowControl>>,
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

    fn request_redraw(&mut self) {
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    fn set_title(&mut self, title: &str) {
        if let Some(w) = self.window.as_ref() {
            w.set_title(title);
        }
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

    fn install_window(&mut self, window: Box<dyn WindowControl>) {
        self.window = Some(window);
    }

    fn announce(&mut self, text: &str) {
        self.accessibility.accessibility.announce(text);
    }

    fn update_accessibility_tree(&mut self, tree: &platform_abstraction::AccessibilityTree) {
        self.accessibility.accessibility.update_tree(tree);
    }
}

/// In-memory shell for tests. The "clipboard" is just an `Option<String>`.
#[derive(Default)]
#[allow(dead_code)] // currently used only by tests; will be reached when GpuRuntimeState gets test coverage.
pub(crate) struct NullShell {
    clipboard: Option<String>,
    /// Number of times [`AppShell::request_redraw`] has been invoked.
    redraw_requests: u32,
    /// Most recent title passed to [`AppShell::set_title`].
    last_title: Option<String>,
    /// Most recent URL passed to [`AppShell::open_url`].
    last_url: Option<String>,
}

#[cfg(test)]
impl NullShell {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn redraw_requests(&self) -> u32 {
        self.redraw_requests
    }

    pub(crate) fn last_title(&self) -> Option<&str> {
        self.last_title.as_deref()
    }

    pub(crate) fn last_url(&self) -> Option<&str> {
        self.last_url.as_deref()
    }
}

impl AppShell for NullShell {
    fn clipboard_get(&mut self) -> Option<String> {
        self.clipboard.clone()
    }

    fn clipboard_set(&mut self, text: String) {
        self.clipboard = Some(text);
    }

    fn request_redraw(&mut self) {
        self.redraw_requests = self.redraw_requests.saturating_add(1);
    }

    fn set_title(&mut self, title: &str) {
        self.last_title = Some(title.to_owned());
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
    fn null_shell_records_window_calls() {
        let mut shell = NullShell::new();
        shell.request_redraw();
        shell.request_redraw();
        shell.set_title("hello");
        shell.open_url("https://example.com");
        assert_eq!(shell.redraw_requests(), 2);
        assert_eq!(shell.last_title(), Some("hello"));
        assert_eq!(shell.last_url(), Some("https://example.com"));
    }
}
