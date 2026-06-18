pub trait Clipboard {
    fn get(&self) -> Option<String>;
    fn set(&mut self, text: String);
}

pub trait Ime {
    fn begin_composition(&mut self);
    fn update_preedit(&mut self, preedit: &str);
    fn commit(&mut self) -> Option<String>;
    fn cancel(&mut self);
}

pub trait Accessibility {
    /// Announce a short text string to the active screen reader.
    ///
    /// On macOS this posts `NSAccessibilityAnnouncementNotification`; on Linux
    /// it fires an AT-SPI text announcement.  Callers should pass the newest
    /// terminal output line(s) so the user hears them without navigating.
    fn announce(&self, text: &str);
    /// Move screen-reader focus to the identified node.
    fn set_focus(&mut self, node_id: &str);
    /// Push a fresh semantic accessibility tree to the platform layer.
    ///
    /// Called after each render frame with the current terminal structure so
    /// screen readers can navigate prompts, command output, and hyperlinks.
    /// Implementations are free to diff against the previous tree and only
    /// notify the AT of changed nodes.
    fn update_tree(&mut self, tree: &crate::types::AccessibilityTree);
}

pub trait DpiAwareness {
    fn scale_factor(&self) -> f32;
    fn logical_to_physical(&self, value: f32) -> f32 {
        value * self.scale_factor()
    }
}

pub trait FontFallback {
    fn fallback_for_char(&self, ch: char) -> Option<String>;
}

/// Read-only process metadata used by the application layer.
pub trait ProcessInfo {
    /// Return the current working directory of the process, if available.
    fn read_child_cwd(&self, pid: u32) -> Option<String>;
}

/// Operations the application layer needs to perform on the host window or
/// shell that don't fit any of the other trait boundaries. Backends owning the
/// window (`render-wgpu`) provide a concrete implementation and hand it to the
/// application layer through the renderer's setup callback.
///
/// All methods take `&self` so the trait object can be cheaply shared with
/// callbacks living inside the event loop. Implementations must therefore use
/// interior mutability where mutation is required.
pub trait WindowControl {
    /// Request a redraw of the host window. May coalesce with pending redraws.
    fn request_redraw(&self);
    /// Set the host window's title bar text.
    fn set_title(&self, title: &str);
    /// Open the given URL with the OS default handler. Only `http://`,
    /// `https://`, `file://`, `mailto:` and `ftp://` schemes are accepted;
    /// other inputs are silently ignored to avoid arbitrary command execution.
    fn open_url(&self, url: &str);
    /// Send an OS-level notification. Default: no-op (silently ignored).
    fn notify(&self, _title: &str, _body: &str) {}
}
