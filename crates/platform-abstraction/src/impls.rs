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
/// The key difference from the original: if a read fails, we clear the cache
/// and retry with a fresh connection instead of permanently giving up. This
/// handles KDE and other environments where clipboard connectivity might be
/// temporarily unavailable.
#[derive(Default)]
pub struct SystemClipboard {
    inner: Mutex<SystemClipboardInner>,
}

#[derive(Default)]
struct SystemClipboardInner {
    handle: Option<arboard::Clipboard>,
}

impl SystemClipboardInner {
    fn handle_mut(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.handle.is_none() {
            match arboard::Clipboard::new() {
                Ok(cb) => self.handle = Some(cb),
                Err(err) => {
                    tracing::debug!("clipboard initialization failed: {}", err);
                    return None;
                }
            }
        }
        self.handle.as_mut()
    }

    fn get_text(&mut self) -> Option<String> {
        // Try with current handle
        if let Some(cb) = self.handle_mut() {
            match cb.get_text() {
                Ok(text) => return Some(text),
                Err(err) => {
                    tracing::debug!("clipboard read failed: {}", err);
                    // Clear handle and retry once with fresh connection
                    self.handle = None;
                }
            }
        }

        // Retry with fresh handle
        if let Some(cb) = self.handle_mut() {
            cb.get_text().ok()
        } else {
            None
        }
    }

    fn set_text(&mut self, text: String) {
        if let Some(cb) = self.handle_mut()
            && let Err(err) = cb.set_text(text)
        {
            tracing::debug!("clipboard write failed: {}", err);
            // Clear handle for next operation
            self.handle = None;
        }
    }
}

impl Clipboard for SystemClipboard {
    fn get(&self) -> Option<String> {
        let mut guard = self.inner.lock().ok()?;
        guard.get_text()
    }

    fn set(&mut self, text: String) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.set_text(text);
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

    fn update_tree(&mut self, _tree: &crate::types::AccessibilityTree) {
        // In-memory stub: no-op. Used in tests and on unsupported platforms.
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

// ── Linux accessibility adapter ───────────────────────────────────────────────

/// Linux accessibility adapter.
///
/// Announces text to the system speech engine by spawning `spd-say`
/// (speech-dispatcher, used by Orca) or `espeak` as a fire-and-forget child
/// process.  If neither tool is installed the call silently no-ops — this
/// matches the behaviour of most terminal emulators on speech-reader-less
/// desktops.
///
/// `update_tree` uses the same command-zone diffing logic as the macOS
/// adapter: only newly completed zones are announced so VoiceOver/Orca does
/// not re-read the entire history on every keystroke.
#[cfg(target_os = "linux")]
#[derive(Default)]
pub struct LinuxAccessibility {
    previous_zone_count: usize,
}

#[cfg(target_os = "linux")]
impl Accessibility for LinuxAccessibility {
    fn announce(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        // Only speak when a screen reader is actually active.  Orca (and any
        // other AT-SPI consumer) sets AT_SPI_BUS_ADDRESS in the session; if
        // that variable is absent there is nothing listening so we bail out.
        if std::env::var_os("AT_SPI_BUS_ADDRESS").is_none() {
            return;
        }
        // Try spd-say (speech-dispatcher) first; fall back to espeak.
        // Both are spawned fire-and-forget — we don't wait for them to finish.
        let launched = std::process::Command::new("spd-say")
            .arg("--")
            .arg(text)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok();

        if !launched {
            let _ = std::process::Command::new("espeak")
                .arg(text)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }
    }

    fn set_focus(&mut self, _node_id: &str) {}

    fn update_tree(&mut self, tree: &crate::types::AccessibilityTree) {
        use crate::types::AccessNode;

        let zone_count = tree
            .nodes
            .iter()
            .filter(|n| matches!(n, AccessNode::CommandZone { .. }))
            .count();

        if zone_count > self.previous_zone_count {
            let new_zones = tree
                .nodes
                .iter()
                .filter_map(|n| match n {
                    AccessNode::CommandZone {
                        command_text,
                        output_text,
                        exit_code,
                        ..
                    } => Some((command_text.as_str(), output_text.as_str(), *exit_code)),
                    _ => None,
                })
                .skip(self.previous_zone_count);

            for (cmd, output, code) in new_zones {
                let summary = match code {
                    Some(0) => format!("Command completed: {cmd}"),
                    Some(c) => format!("Command failed (exit {c}): {cmd}"),
                    None => format!("Running: {cmd}"),
                };
                self.announce(&summary);
                if !output.trim().is_empty() {
                    let preview: String = output.chars().take(200).collect();
                    self.announce(preview.trim());
                }
            }
            self.previous_zone_count = zone_count;
        }
    }
}
