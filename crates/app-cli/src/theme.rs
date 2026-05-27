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
    pub black:   String,
    pub red:     String,
    pub green:   String,
    pub yellow:  String,
    pub blue:    String,
    pub magenta: String,
    pub cyan:    String,
    pub white:   String,
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
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Load all valid `*.yaml`/`*.yml` files from the themes directory.
/// Invalid or unreadable files are silently skipped.
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
            let data = fs::read_to_string(e.path()).ok()?;
            serde_yaml::from_str(&data).ok()
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
                        if s.ends_with(".yaml") || s.ends_with(".yml") { Some(()) } else { None }
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
        let _ = fs::write(path, content);
    }
}

/// Bundled default themes shipped with the binary.
const BUNDLED_THEMES: &[(&str, &str)] = &[
    ("catppuccin-mocha.yaml",  include_str!("../../../themes/catppuccin-mocha.yaml")),
    ("dracula.yaml",           include_str!("../../../themes/dracula.yaml")),
    ("gruvbox-dark.yaml",      include_str!("../../../themes/gruvbox-dark.yaml")),
    ("nord.yaml",              include_str!("../../../themes/nord.yaml")),
    ("one-dark.yaml",          include_str!("../../../themes/one-dark.yaml")),
    ("rose-pine.yaml",         include_str!("../../../themes/rose-pine.yaml")),
    ("solarized-dark.yaml",    include_str!("../../../themes/solarized-dark.yaml")),
    ("tokyo-night.yaml",       include_str!("../../../themes/tokyo-night.yaml")),
];
