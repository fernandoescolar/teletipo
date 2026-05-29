use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Validation bounds ───────────────────────────────────────────────────────

/// Min/max permitted font point size.
pub const FONT_SIZE_MIN: f32 = 4.0;
pub const FONT_SIZE_MAX: f32 = 80.0;
/// Max horizontal/vertical padding in physical pixels.
pub const PADDING_MAX: u32 = 200;
/// Max scrollback lines per session.
pub const SCROLLBACK_LINES_MAX: u32 = 1_000_000;

// ── Persisted config structs ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FontCfg {
    /// Font point size (e.g. 14.0).
    pub size: f32,
    /// Font family name (e.g. "Hack", "Consolas"). `None` = use default.
    pub family: Option<String>,
}

impl Default for FontCfg {
    fn default() -> Self {
        Self {
            size: 14.0,
            family: None,
        }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalCfg {
    /// Shell executable path.  `None` = auto-detect from environment.
    pub shell: Option<String>,
    /// Number of scrollback lines kept per session (0 = built-in default).
    pub scrollback_lines: u32,
    /// Show a visual bell flash on BEL (0x07).  Default: `true`.
    pub bell: bool,
}

impl Default for TerminalCfg {
    fn default() -> Self {
        Self {
            shell: None,
            scrollback_lines: 0,
            bell: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UserConfig {
    pub font: FontCfg,
    pub padding: PaddingCfg,
    pub terminal: TerminalCfg,
    /// Name of the active preset theme file (`None` = default colors).
    pub active_theme: Option<String>,
}

// ── Settings field descriptors (drives the in-app settings overlay) ─────────

/// Static descriptor of every editable setting.
pub const SETTINGS_FIELDS: &[SettingsDef] = &[
    // "theme" key is the theme picker — cycled with ← →, not free-text edited.
    SettingsDef {
        section: "theme",
        key: "theme",
    },
    SettingsDef {
        section: "font",
        key: "size",
    },
    SettingsDef {
        section: "font",
        key: "family",
    },
    SettingsDef {
        section: "padding",
        key: "horizontal",
    },
    SettingsDef {
        section: "padding",
        key: "vertical",
    },
    SettingsDef {
        section: "terminal",
        key: "shell",
    },
    SettingsDef {
        section: "terminal",
        key: "scrollback_lines",
    },
    SettingsDef {
        section: "terminal",
        key: "bell",
    },
];

pub struct SettingsDef {
    pub section: &'static str,
    pub key: &'static str,
}

// ── UserConfig field get/set ────────────────────────────────────────────────

impl UserConfig {
    /// Return the current string value of `section.key`.
    pub fn get_field(&self, section: &str, key: &str) -> String {
        match (section, key) {
            ("theme", "theme") => self
                .active_theme
                .clone()
                .unwrap_or_else(|| "(none)".to_owned()),
            ("font", "size") => format!("{}", self.font.size),
            ("font", "family") => self
                .font
                .family
                .clone()
                .unwrap_or_else(|| "(default)".to_owned()),
            ("padding", "horizontal") => format!("{}", self.padding.horizontal),
            ("padding", "vertical") => format!("{}", self.padding.vertical),
            ("terminal", "shell") => self
                .terminal
                .shell
                .clone()
                .unwrap_or_else(|| "(auto)".to_owned()),
            ("terminal", "scrollback_lines") => {
                if self.terminal.scrollback_lines == 0 {
                    "(default)".to_owned()
                } else {
                    format!("{}", self.terminal.scrollback_lines)
                }
            }
            ("terminal", "bell") => {
                if self.terminal.bell {
                    "on".to_owned()
                } else {
                    "off".to_owned()
                }
            }
            _ => String::new(),
        }
    }

    /// Clamp out-of-range numeric fields to safe values. Each clamp logs a
    /// `tracing::warn!` event so the user can see why their setting was
    /// modified.
    pub fn validate(&mut self) {
        if !self.font.size.is_finite()
            || self.font.size < FONT_SIZE_MIN
            || self.font.size > FONT_SIZE_MAX
        {
            let old = self.font.size;
            self.font.size = self.font.size.clamp(FONT_SIZE_MIN, FONT_SIZE_MAX);
            if !old.is_finite() {
                self.font.size = FontCfg::default().size;
            }
            tracing::warn!(
                field = "font.size",
                old = old,
                new = self.font.size,
                "clamped out-of-range value"
            );
        }
        if self.padding.horizontal > PADDING_MAX {
            let old = self.padding.horizontal;
            self.padding.horizontal = PADDING_MAX;
            tracing::warn!(
                field = "padding.horizontal",
                old = old,
                new = self.padding.horizontal,
                "clamped out-of-range value"
            );
        }
        if self.padding.vertical > PADDING_MAX {
            let old = self.padding.vertical;
            self.padding.vertical = PADDING_MAX;
            tracing::warn!(
                field = "padding.vertical",
                old = old,
                new = self.padding.vertical,
                "clamped out-of-range value"
            );
        }
        if self.terminal.scrollback_lines > SCROLLBACK_LINES_MAX {
            let old = self.terminal.scrollback_lines;
            self.terminal.scrollback_lines = SCROLLBACK_LINES_MAX;
            tracing::warn!(
                field = "terminal.scrollback_lines",
                old = old,
                new = self.terminal.scrollback_lines,
                "clamped out-of-range value"
            );
        }
        if let Some(ref shell) = self.terminal.shell
            && !std::path::Path::new(shell).exists()
        {
            tracing::warn!(
                field = "terminal.shell",
                shell = %shell,
                "configured shell path does not exist; will fall back at spawn time"
            );
        }
    }

    /// Validate and apply a new string value to `section.key`.
    /// Returns `true` if the value was valid and was applied.
    pub fn set_field(&mut self, section: &str, key: &str, value: &str) -> bool {
        let value = value.trim();
        match (section, key) {
            ("font", "size") => {
                if let Ok(v) = value.parse::<f32>()
                    && (FONT_SIZE_MIN..=FONT_SIZE_MAX).contains(&v)
                {
                    self.font.size = v;
                    return true;
                }
                false
            }
            ("font", "family") => {
                self.font.family = if value.is_empty() || value == "(default)" {
                    None
                } else {
                    Some(value.to_owned())
                };
                true
            }
            ("padding", "horizontal") => {
                if let Ok(v) = value.parse::<u32>()
                    && v <= PADDING_MAX
                {
                    self.padding.horizontal = v;
                    return true;
                }
                false
            }
            ("padding", "vertical") => {
                if let Ok(v) = value.parse::<u32>()
                    && v <= PADDING_MAX
                {
                    self.padding.vertical = v;
                    return true;
                }
                false
            }
            ("terminal", "shell") => {
                self.terminal.shell = if value.is_empty() || value == "(auto)" {
                    None
                } else {
                    Some(value.to_owned())
                };
                true
            }
            ("terminal", "scrollback_lines") => {
                if value.is_empty() || value == "(default)" {
                    self.terminal.scrollback_lines = 0;
                    return true;
                }
                if let Ok(v) = value.parse::<u32>()
                    && v <= SCROLLBACK_LINES_MAX
                {
                    self.terminal.scrollback_lines = v;
                    return true;
                }
                false
            }
            ("terminal", "bell") => match value.to_lowercase().as_str() {
                "on" | "true" | "1" => {
                    self.terminal.bell = true;
                    true
                }
                "off" | "false" | "0" => {
                    self.terminal.bell = false;
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }
}

// ── File I/O ─────────────────────────────────────────────────────────────────

/// Errors that can occur while loading the user config file.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The XDG/standard config directory could not be located.
    #[error("could not determine config directory")]
    NoConfigDir,
    /// The config file could not be read from disk.
    #[error("failed to read config file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The config file contents could not be parsed as TOML.
    #[error("failed to parse config file {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

pub fn config_path() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("teletipo");
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("config.toml"))
}

/// Fallible variant of [`load_config`]. Returns a typed error instead of
/// silently falling back to defaults.
///
/// If the config file does not exist yet, a richly commented default file is
/// written to disk and `UserConfig::default()` is returned.
pub fn load_config_result() -> Result<UserConfig, ConfigError> {
    let path = config_path().ok_or(ConfigError::NoConfigDir)?;
    if !path.exists() {
        let cfg = UserConfig::default();
        write_default_config(&path);
        return Ok(cfg);
    }
    let data = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source,
    })?;
    let mut cfg: UserConfig = toml::from_str(&data).map_err(|source| ConfigError::Parse {
        path: path.clone(),
        source,
    })?;
    cfg.validate();
    Ok(cfg)
}

/// Load config from disk.  If the file does not exist yet, write a default
/// one with inline comments so the user can discover all options.
///
/// On any I/O or parse failure this function logs a `tracing::warn!` event
/// with the offending path and returns `UserConfig::default()`. Callers that
/// need the underlying error should use [`load_config_result`].
pub fn load_config() -> UserConfig {
    match load_config_result() {
        Ok(cfg) => cfg,
        Err(err) => {
            tracing::warn!(error = %err, "falling back to default config");
            UserConfig::default()
        }
    }
}

pub fn save_config(cfg: &UserConfig) {
    if let Some(path) = config_path()
        && let Ok(s) = toml::to_string_pretty(cfg)
    {
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
# Font family name. Comment out to use the built-in default.
# family = "Hack"

[padding]
# Physical-pixel inset of the terminal text grid from the window edges.
horizontal = 0
vertical   = 0

[terminal]
# Shell executable. Comment out to auto-detect from $SHELL / system default.
# shell = "/bin/zsh"
# Scrollback lines per session (0 = built-in default).
# scrollback_lines = 10000
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

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_rrggbb() {
        let c = parse_color("#ff8000").unwrap();
        assert!((c[0] - 1.0).abs() < 1e-3);
        assert!((c[1] - 128.0 / 255.0).abs() < 1e-3);
        assert!((c[2] - 0.0).abs() < 1e-3);
        assert!((c[3] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn parse_color_invalid_returns_none() {
        assert!(parse_color("not-a-color").is_none());
        assert!(parse_color("#fff").is_none());
    }

    #[test]
    fn validate_clamps_font_size_too_small() {
        let mut cfg = UserConfig {
            font: FontCfg {
                size: 1.0,
                family: None,
            },
            ..Default::default()
        };
        cfg.validate();
        assert_eq!(cfg.font.size, FONT_SIZE_MIN);
    }

    #[test]
    fn validate_clamps_font_size_too_large() {
        let mut cfg = UserConfig {
            font: FontCfg {
                size: 9999.0,
                family: None,
            },
            ..Default::default()
        };
        cfg.validate();
        assert_eq!(cfg.font.size, FONT_SIZE_MAX);
    }

    #[test]
    fn validate_resets_nan_font_size_to_default() {
        let mut cfg = UserConfig {
            font: FontCfg {
                size: f32::NAN,
                family: None,
            },
            ..Default::default()
        };
        cfg.validate();
        assert_eq!(cfg.font.size, FontCfg::default().size);
    }

    #[test]
    fn validate_clamps_padding() {
        let mut cfg = UserConfig {
            padding: PaddingCfg {
                horizontal: 10_000,
                vertical: 10_000,
            },
            ..Default::default()
        };
        cfg.validate();
        assert_eq!(cfg.padding.horizontal, PADDING_MAX);
        assert_eq!(cfg.padding.vertical, PADDING_MAX);
    }

    #[test]
    fn validate_clamps_scrollback_lines() {
        let mut cfg = UserConfig {
            terminal: TerminalCfg {
                scrollback_lines: u32::MAX,
                ..Default::default()
            },
            ..Default::default()
        };
        cfg.validate();
        assert_eq!(cfg.terminal.scrollback_lines, SCROLLBACK_LINES_MAX);
    }

    #[test]
    fn validate_leaves_in_range_values_unchanged() {
        let mut cfg = UserConfig {
            font: FontCfg {
                size: 14.0,
                family: None,
            },
            padding: PaddingCfg {
                horizontal: 8,
                vertical: 8,
            },
            terminal: TerminalCfg {
                scrollback_lines: 10_000,
                ..Default::default()
            },
            ..Default::default()
        };
        let snapshot = format!("{cfg:?}");
        cfg.validate();
        assert_eq!(format!("{cfg:?}"), snapshot);
    }

    #[test]
    fn config_error_parse_includes_path() {
        // Construct a Parse error by parsing intentionally bad TOML and check
        // Display contains a path-like marker.
        let path = std::path::PathBuf::from("/tmp/teletipo-test.toml");
        let err = match toml::from_str::<UserConfig>("not = valid = toml") {
            Ok(_) => panic!("expected parse error"),
            Err(source) => ConfigError::Parse {
                path: path.clone(),
                source,
            },
        };
        let msg = err.to_string();
        assert!(msg.contains("teletipo-test.toml"));
    }

    #[test]
    fn set_field_accepts_documented_maximums() {
        let mut cfg = UserConfig::default();
        assert!(cfg.set_field("font", "size", &FONT_SIZE_MAX.to_string()));
        assert_eq!(cfg.font.size, FONT_SIZE_MAX);

        assert!(cfg.set_field("padding", "horizontal", &PADDING_MAX.to_string()));
        assert!(cfg.set_field("padding", "vertical", &PADDING_MAX.to_string()));
        assert_eq!(cfg.padding.horizontal, PADDING_MAX);
        assert_eq!(cfg.padding.vertical, PADDING_MAX);

        assert!(cfg.set_field(
            "terminal",
            "scrollback_lines",
            &SCROLLBACK_LINES_MAX.to_string()
        ));
        assert_eq!(cfg.terminal.scrollback_lines, SCROLLBACK_LINES_MAX);
    }

    #[test]
    fn set_field_rejects_values_above_documented_maximums() {
        let mut cfg = UserConfig::default();
        assert!(!cfg.set_field("font", "size", "80.1"));
        assert!(!cfg.set_field("padding", "horizontal", "201"));
        assert!(!cfg.set_field("padding", "vertical", "201"));
        assert!(!cfg.set_field("terminal", "scrollback_lines", "1000001"));
    }
}
