//! UI configuration values rendered through the settings overlay.

#[derive(Debug, Clone)]
pub struct UiConfig {
    pub padding_horizontal: f32,
    pub padding_vertical: f32,
    pub active_theme_idx: Option<usize>,
    pub active_font_idx: usize,
    // User settings values
    pub font_size: f32,
    pub font_family: Option<String>,
    pub terminal_shell: Option<String>,
    pub terminal_scrollback_lines: u32,
    pub terminal_bell: bool,
    pub active_theme: Option<String>,
    // Available options for pickers
    pub available_themes: Vec<String>,
    pub available_fonts: Vec<String>,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            padding_horizontal: 8.0,
            padding_vertical: 8.0,
            active_theme_idx: None,
            active_font_idx: 0,
            font_size: 14.0,
            font_family: None,
            terminal_shell: None,
            terminal_scrollback_lines: 0,
            terminal_bell: true,
            active_theme: None,
            available_themes: Vec::new(),
            available_fonts: Vec::new(),
        }
    }
}

impl UiConfig {
    /// Return the current string value of `section.key` for the settings overlay.
    pub fn get_field(&self, section: &str, key: &str) -> String {
        match (section, key) {
            ("theme", "theme") => self
                .active_theme
                .clone()
                .unwrap_or_else(|| "(none)".to_owned()),
            ("font", "size") => format!("{}", self.font_size),
            ("font", "family") => {
                if self.active_font_idx == 0 {
                    "(default)".to_owned()
                } else {
                    self.available_fonts
                        .get(self.active_font_idx)
                        .cloned()
                        .unwrap_or_else(|| "(default)".to_owned())
                }
            }
            ("padding", "horizontal") => format!("{}", self.padding_horizontal as u32),
            ("padding", "vertical") => format!("{}", self.padding_vertical as u32),
            ("terminal", "shell") => self
                .terminal_shell
                .clone()
                .unwrap_or_else(|| "(auto)".to_owned()),
            ("terminal", "scrollback_lines") => {
                if self.terminal_scrollback_lines == 0 {
                    "(default)".to_owned()
                } else {
                    format!("{}", self.terminal_scrollback_lines)
                }
            }
            ("terminal", "bell") => {
                if self.terminal_bell {
                    "on".to_owned()
                } else {
                    "off".to_owned()
                }
            }
            _ => String::new(),
        }
    }

    /// Apply a new string value to `section.key`.
    pub fn set_field(&mut self, section: &str, key: &str, value: &str) {
        let value = value.trim();
        match (section, key) {
            ("font", "size") => {
                if let Ok(v) = value.parse::<f32>() {
                    self.font_size = v.clamp(4.0, 80.0);
                }
            }
            ("font", "family") => {
                self.font_family = if value.is_empty() || value == "(default)" {
                    None
                } else {
                    Some(value.to_owned())
                };
            }
            ("padding", "horizontal") => {
                if let Ok(v) = value.parse::<f32>() {
                    self.padding_horizontal = v.clamp(0.0, 100.0);
                }
            }
            ("padding", "vertical") => {
                if let Ok(v) = value.parse::<f32>() {
                    self.padding_vertical = v.clamp(0.0, 100.0);
                }
            }
            ("terminal", "shell") => {
                self.terminal_shell = if value.is_empty() || value == "(auto)" {
                    None
                } else {
                    Some(value.to_owned())
                };
            }
            ("terminal", "scrollback_lines") => {
                if value.is_empty() || value == "(default)" {
                    self.terminal_scrollback_lines = 0;
                } else if let Ok(v) = value.parse::<u32>() {
                    self.terminal_scrollback_lines = v.min(500_000);
                }
            }
            ("terminal", "bell") => {
                self.terminal_bell = matches!(value.to_lowercase().as_str(), "on" | "true" | "1");
            }
            _ => {}
        }
    }
}
