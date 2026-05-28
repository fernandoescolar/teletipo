use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

// ── Persisted config structs ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FontCfg {
    /// Font point size (e.g. 14.0).
    pub size: f32,
    /// Absolute path to a TTF/OTF font file. `None` = use the built-in default.
    pub path: Option<String>,
}

impl Default for FontCfg {
    fn default() -> Self {
        Self { size: 14.0, path: None }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PaddingCfg {
    /// Horizontal padding in physical pixels (left + right inset of the text grid).
    pub horizontal: u32,
    /// Vertical padding in physical pixels (top + bottom inset of the text grid).
    pub vertical: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UserConfig {
    pub font:         FontCfg,
    pub padding:      PaddingCfg,
    /// Name of the active preset theme file (`None` = default colors).
    pub active_theme: Option<String>,
}

// ── Settings field descriptors (drives the in-app settings overlay) ─────────

/// Static descriptor of every editable setting.
pub const SETTINGS_FIELDS: &[SettingsDef] = &[
    // "theme" key is the theme picker — cycled with ← →, not free-text edited.
    SettingsDef { section: "theme",   key: "theme" },
    SettingsDef { section: "font",    key: "size" },
    SettingsDef { section: "font",    key: "path" },
    SettingsDef { section: "padding", key: "horizontal" },
    SettingsDef { section: "padding", key: "vertical" },
];

pub struct SettingsDef {
    pub section: &'static str,
    pub key:     &'static str,
}

// ── UserConfig field get/set ────────────────────────────────────────────────

impl UserConfig {
    /// Return the current string value of `section.key`.
    pub fn get_field(&self, section: &str, key: &str) -> String {
        match (section, key) {
            ("theme",   "theme")       => self.active_theme.clone()
                                              .unwrap_or_else(|| "(none)".to_owned()),
            ("font",    "size")        => format!("{}", self.font.size),
            ("font",    "path")        => self.font.path.clone()
                                              .unwrap_or_else(|| "(default)".to_owned()),
            ("padding", "horizontal")  => format!("{}", self.padding.horizontal),
            ("padding", "vertical")    => format!("{}", self.padding.vertical),
            _ => String::new(),
        }
    }

    /// Validate and apply a new string value to `section.key`.
    /// Returns `true` if the value was valid and was applied.
    pub fn set_field(&mut self, section: &str, key: &str, value: &str) -> bool {
        let value = value.trim();
        match (section, key) {
            ("font", "size") => {
                if let Ok(v) = value.parse::<f32>()
                    && v > 4.0 && v < 80.0 { self.font.size = v; return true; }
                false
            }
            ("font", "path") => {
                self.font.path = if value.is_empty() || value == "(default)" {
                    None
                } else {
                    Some(value.to_owned())
                };
                true
            }
            ("padding", "horizontal") => {
                if let Ok(v) = value.parse::<u32>()
                    && v <= 100 { self.padding.horizontal = v; return true; }
                false
            }
            ("padding", "vertical") => {
                if let Ok(v) = value.parse::<u32>()
                    && v <= 100 { self.padding.vertical = v; return true; }
                false
            }
            _ => false,
        }
    }
}

// ── File I/O ─────────────────────────────────────────────────────────────────

pub fn config_path() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("teletipo");
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("config.toml"))
}

/// Load config from disk.  If the file does not exist yet, write a default
/// one with inline comments so the user can discover all options.
pub fn load_config() -> UserConfig {
    let path = match config_path() {
        Some(p) => p,
        None => return UserConfig::default(),
    };
    if !path.exists() {
        let cfg = UserConfig::default();
        write_default_config(&path);
        return cfg;
    }
    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return UserConfig::default(),
    };
    toml::from_str(&data).unwrap_or_default()
}

pub fn save_config(cfg: &UserConfig) {
    if let Some(path) = config_path()
        && let Ok(s) = toml::to_string_pretty(cfg) {
            let _ = fs::write(path, s);
        }
}

/// Write a richly commented default config file.
fn write_default_config(path: &std::path::Path) {
    let content = r##"# teletipo configuration
# Edit this file directly or use the in-app settings (Cmd+,).
# Use the in-app theme picker (Cmd+,  then ← →) to select a colour theme.
# Font changes take effect after restarting teletipo.

# active_theme = "tokyo-night"   # name of a YAML file in ~/.config/teletipo/themes/

[font]
# Point size of the monospace font.
size = 14.0
# Absolute path to a TTF/OTF font file.  Comment out to use the built-in default.
# path = "/path/to/YourFont.ttf"

[padding]
# Physical-pixel inset of the terminal text grid from the window edges.
horizontal = 0
vertical   = 0
"##;
    let _ = fs::write(path, content);
}

// ── Color helpers ─────────────────────────────────────────────────────────────

/// Parse a CSS hex color string (#rrggbb or #rrggbbaa) into [r, g, b, a] 0..1.
pub fn parse_color(s: &str) -> Option<[f32; 4]> {
    let s = s.trim().trim_start_matches('#');
    match s.len() {
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()? as f32 / 255.0;
            let g = u8::from_str_radix(&s[2..4], 16).ok()? as f32 / 255.0;
            let b = u8::from_str_radix(&s[4..6], 16).ok()? as f32 / 255.0;
            Some([r, g, b, 1.0])
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()? as f32 / 255.0;
            let g = u8::from_str_radix(&s[2..4], 16).ok()? as f32 / 255.0;
            let b = u8::from_str_radix(&s[4..6], 16).ok()? as f32 / 255.0;
            let a = u8::from_str_radix(&s[6..8], 16).ok()? as f32 / 255.0;
            Some([r, g, b, a])
        }
        _ => None,
    }
}
