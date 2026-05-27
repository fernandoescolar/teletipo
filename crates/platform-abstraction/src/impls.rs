use crate::traits::{Accessibility, Clipboard, DpiAwareness, FontFallback, Ime};

#[derive(Default)]
pub struct MemoryClipboard {
    content: Option<String>,
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
