use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::parse_color;

// ── Theme file format ─────────────────────────────────────────────────────────

/// Represents the full contents of a `.yaml` theme file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeFile {
    pub name: String,
    pub accent: String,
    pub cursor: String,
    pub background: String,
    pub foreground: String,
    /// "darker" or "lighter" — controls whether the editor pane is slightly
    /// lighter or darker than the terminal background.
    #[serde(default = "default_details")]
    pub details: String,
    pub terminal_colors: TerminalColors,
}

fn default_details() -> String {
    "darker".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalColors {
    pub bright: ColorSet,
    pub normal: ColorSet,
}

/// The eight named ANSI colors in a single brightness tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorSet {
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
}

// ── Palette extraction ────────────────────────────────────────────────────────

/// Build the 16-entry ANSI palette (indices 0-15) from a theme file.
/// Order: normal 0-7 (black…white), then bright 8-15 (black…white).
pub fn build_ansi_palette(tf: &ThemeFile) -> [[f32; 3]; 16] {
    fn hex3(s: &str) -> [f32; 3] {
        parse_color(s)
            .map(|[r, g, b, _]| [r, g, b])
            .unwrap_or([0.0, 0.0, 0.0])
    }
    let n = &tf.terminal_colors.normal;
    let b = &tf.terminal_colors.bright;
    [
        hex3(&n.black),   // 0
        hex3(&n.red),     // 1
        hex3(&n.green),   // 2
        hex3(&n.yellow),  // 3
        hex3(&n.blue),    // 4
        hex3(&n.magenta), // 5
        hex3(&n.cyan),    // 6
        hex3(&n.white),   // 7
        hex3(&b.black),   // 8
        hex3(&b.red),     // 9
        hex3(&b.green),   // 10
        hex3(&b.yellow),  // 11
        hex3(&b.blue),    // 12
        hex3(&b.magenta), // 13
        hex3(&b.cyan),    // 14
        hex3(&b.white),   // 15
    ]
}

// ── Filesystem helpers ────────────────────────────────────────────────────────

/// Returns `~/.config/teletipo/themes/`, creating it if needed.
pub fn themes_dir() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("teletipo").join("themes");
    if let Err(err) = fs::create_dir_all(&dir) {
        tracing::warn!(path = %dir.display(), error = %err, "failed to create themes directory");
        return None;
    }
    Some(dir)
}

/// Load all valid `*.yaml`/`*.yml` files from the themes directory.
/// Files that fail to parse log a `tracing::warn!` with the offending path
/// and are skipped.
pub fn load_themes() -> Vec<ThemeFile> {
    let dir = match themes_dir() {
        Some(d) => d,
        None => return Vec::new(),
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut themes: Vec<ThemeFile> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            s.ends_with(".yaml") || s.ends_with(".yml")
        })
        .filter_map(|e| {
            let path = e.path();
            let data = match fs::read_to_string(&path) {
                Ok(d) => d,
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "failed to read theme file");
                    return None;
                }
            };
            match serde_yaml::from_str::<ThemeFile>(&data) {
                Ok(t) => Some(t),
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "failed to parse theme file");
                    None
                }
            }
        })
        .collect();

    // Stable sort so the list is deterministic across runs.
    themes.sort_by(|a, b| a.name.cmp(&b.name));
    themes
}

// ── Bundled default themes ────────────────────────────────────────────────────

/// Copies the bundled default theme files into the user themes directory if
/// the directory is empty (i.e. first run or after manual deletion).
pub fn install_default_themes() {
    let dir = match themes_dir() {
        Some(d) => d,
        None => return,
    };

    // Only install if the directory contains no YAML files yet.
    let already_has_themes = fs::read_dir(&dir)
        .ok()
        .map(|mut rd| {
            rd.any(|e| {
                e.ok()
                    .and_then(|e| {
                        let n = e.file_name();
                        let s = n.to_string_lossy().into_owned();
                        if s.ends_with(".yaml") || s.ends_with(".yml") {
                            Some(())
                        } else {
                            None
                        }
                    })
                    .is_some()
            })
        })
        .unwrap_or(false);

    if already_has_themes {
        return;
    }

    for (filename, content) in BUNDLED_THEMES {
        let path = dir.join(filename);
        if let Err(err) = fs::write(&path, content) {
            tracing::warn!(path = %path.display(), error = %err, "failed to install bundled theme");
        }
    }
}

/// Bundled default themes shipped with the binary.
const BUNDLED_THEMES: &[(&str, &str)] = &[
    (
        "catppuccin-mocha.yaml",
        include_str!("../../../themes/catppuccin-mocha.yaml"),
    ),
    ("dracula.yaml", include_str!("../../../themes/dracula.yaml")),
    (
        "gruvbox-dark.yaml",
        include_str!("../../../themes/gruvbox-dark.yaml"),
    ),
    ("nord.yaml", include_str!("../../../themes/nord.yaml")),
    (
        "one-dark.yaml",
        include_str!("../../../themes/one-dark.yaml"),
    ),
    (
        "rose-pine.yaml",
        include_str!("../../../themes/rose-pine.yaml"),
    ),
    (
        "solarized-dark.yaml",
        include_str!("../../../themes/solarized-dark.yaml"),
    ),
    (
        "tokyo-night.yaml",
        include_str!("../../../themes/tokyo-night.yaml"),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn color_set(prefix: &str) -> ColorSet {
        ColorSet {
            black: format!("#{prefix}0000"),
            red: format!("#{prefix}1111"),
            green: format!("#{prefix}2222"),
            yellow: format!("#{prefix}3333"),
            blue: format!("#{prefix}4444"),
            magenta: format!("#{prefix}5555"),
            cyan: format!("#{prefix}6666"),
            white: format!("#{prefix}7777"),
        }
    }

    fn sample_theme() -> ThemeFile {
        ThemeFile {
            name: "test".into(),
            accent: "#ffffff".into(),
            cursor: "#ffffff".into(),
            background: "#000000".into(),
            foreground: "#ffffff".into(),
            details: "darker".into(),
            terminal_colors: TerminalColors {
                normal: color_set("00"),
                bright: color_set("80"),
            },
        }
    }

    #[test]
    fn palette_has_sixteen_entries() {
        let palette = build_ansi_palette(&sample_theme());
        assert_eq!(palette.len(), 16);
    }

    #[test]
    fn palette_normal_then_bright_ordering() {
        let palette = build_ansi_palette(&sample_theme());
        // Normal black at index 0, bright black at index 8.
        assert_eq!(palette[0][0], 0.0);
        // Bright tier (prefix "80" → 0x80 = 128) should be brighter than normal tier.
        assert!(palette[8][0] > palette[0][0]);
    }

    #[test]
    fn invalid_color_string_falls_back_to_black() {
        let mut tf = sample_theme();
        tf.terminal_colors.normal.red = "not-a-color".into();
        let palette = build_ansi_palette(&tf);
        assert_eq!(palette[1], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn default_details_is_darker() {
        // Verify the serde default function used by ThemeFile.
        assert_eq!(default_details(), "darker");
    }

    #[test]
    fn theme_yaml_roundtrip_through_serde() {
        let yaml = serde_yaml::to_string(&sample_theme()).expect("serialise");
        let parsed: ThemeFile = serde_yaml::from_str(&yaml).expect("parse");
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.details, "darker");
    }

    #[test]
    fn embedded_default_themes_parse() {
        for (name, body) in BUNDLED_THEMES {
            let parsed: Result<ThemeFile, _> = serde_yaml::from_str(body);
            assert!(
                parsed.is_ok(),
                "theme '{name}' failed to parse: {:?}",
                parsed.err()
            );
            let palette = build_ansi_palette(&parsed.unwrap());
            assert_eq!(palette.len(), 16);
        }
    }
}
