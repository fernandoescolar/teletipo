/// Static descriptor of every editable setting.
pub struct SettingsDef {
    pub section: &'static str,
    pub key: &'static str,
}

pub const SETTINGS_FIELDS: &[SettingsDef] = &[
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

/// Returns the increment step for numeric fields, or `None` if the field is
/// not a numeric stepper.
pub fn numeric_step(section: &str, key: &str) -> Option<f32> {
    match (section, key) {
        ("font", "size") => Some(0.5),
        ("padding", "horizontal") | ("padding", "vertical") => Some(1.0),
        ("terminal", "scrollback_lines") => Some(500.0),
        _ => None,
    }
}
