use std::sync::Mutex;

use crate::traits::{Accessibility, Clipboard, DpiAwareness, FontFallback, Ime};

#[derive(Default)]
pub struct MemoryClipboard {
    content: Option<String>,
}

/// Real system clipboard backed by [`arboard`]. The underlying handle is
/// created lazily on first use and re-used across calls (creating a fresh
/// `arboard::Clipboard` per access is expensive on macOS).
///
/// If `arboard::Clipboard::new()` fails (e.g. headless CI, no display server)
/// the failure is sticky: subsequent calls silently no-op rather than retrying
/// every invocation.
#[derive(Default)]
pub struct SystemClipboard {
    inner: Mutex<SystemClipboardInner>,
}

#[derive(Default)]
struct SystemClipboardInner {
    handle: Option<arboard::Clipboard>,
    failed: bool,
}

impl SystemClipboardInner {
    /// Lazy-init wrapper. Returns `None` once a previous init attempt failed.
    fn handle_mut(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.failed {
            return None;
        }
        if self.handle.is_none() {
            match arboard::Clipboard::new() {
                Ok(cb) => self.handle = Some(cb),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to initialise system clipboard");
                    self.failed = true;
                    return None;
                }
            }
        }
        self.handle.as_mut()
    }
}

impl Clipboard for SystemClipboard {
    fn get(&self) -> Option<String> {
        let mut guard = self.inner.lock().ok()?;
        let cb = guard.handle_mut()?;
        cb.get_text().ok()
    }

    fn set(&mut self, text: String) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        if let Some(cb) = guard.handle_mut() {
            let _ = cb.set_text(text);
        }
    }
}

#[derive(Default)]
pub struct MemoryIme {
    preedit: String,
    committed: Option<String>,
}

#[derive(Default)]
pub struct MemoryAccessibility {
    pub last_announcement: Option<String>,
    pub focused_node: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct FixedDpi {
    pub scale: f32,
}

impl Default for FixedDpi {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

#[derive(Default)]
pub struct BasicFontFallback;

impl Clipboard for MemoryClipboard {
    fn get(&self) -> Option<String> {
        self.content.clone()
    }

    fn set(&mut self, text: String) {
        self.content = Some(text);
    }
}

impl Ime for MemoryIme {
    fn begin_composition(&mut self) {
        self.preedit.clear();
        self.committed = None;
    }

    fn update_preedit(&mut self, preedit: &str) {
        self.preedit = preedit.to_string();
    }

    fn commit(&mut self) -> Option<String> {
        if self.preedit.is_empty() {
            return None;
        }
        self.committed = Some(self.preedit.clone());
        self.preedit.clear();
        self.committed.clone()
    }

    fn cancel(&mut self) {
        self.preedit.clear();
        self.committed = None;
    }
}

impl Accessibility for MemoryAccessibility {
    fn announce(&self, _text: &str) {}

    fn set_focus(&mut self, node_id: &str) {
        self.focused_node = Some(node_id.to_string());
    }
}

impl DpiAwareness for FixedDpi {
    fn scale_factor(&self) -> f32 {
        self.scale
    }
}

impl FontFallback for BasicFontFallback {
    fn fallback_for_char(&self, ch: char) -> Option<String> {
        if ch.is_ascii() {
            Some("monospace".to_string())
        } else {
            Some("fallback-unicode".to_string())
        }
    }
}
