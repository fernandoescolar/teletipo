#[cfg(target_os = "macos")]
use std::{fs, path::PathBuf, process::Command};

#[cfg(target_os = "macos")]
const MARKER_FILE: &str = "macos_fda_onboarding_v1.seen";

pub(crate) fn show_macos_privacy_onboarding_once() {
    #[cfg(target_os = "macos")]
    {
        if let Some(marker) = marker_path() {
            if marker.exists() {
                return;
            }
            if let Err(err) = run_dialog_and_maybe_open_settings() {
                tracing::warn!(error = %err, "failed to show macOS privacy onboarding dialog");
            }
            if let Some(parent) = marker.parent()
                && let Err(err) = fs::create_dir_all(parent)
            {
                tracing::warn!(path = %parent.display(), error = %err, "failed to create onboarding directory");
                return;
            }
            if let Err(err) = fs::write(&marker, b"seen\n") {
                tracing::warn!(path = %marker.display(), error = %err, "failed to persist onboarding marker");
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn marker_path() -> Option<PathBuf> {
    let dir = dirs::data_local_dir()?.join("teletipo");
    Some(dir.join(MARKER_FILE))
}

#[cfg(target_os = "macos")]
fn run_dialog_and_maybe_open_settings() -> std::io::Result<()> {
    let script = r#"
set promptText to "Teletipo may need access to files in Desktop, Documents, and other folders.\n\nFor smoother navigation, you can grant Full Disk Access in System Settings."
set choice to button returned of (display dialog promptText with title "Teletipo" buttons {"Later", "Open Settings"} default button "Open Settings")
return choice
"#;

    let output = Command::new("osascript").args(["-e", script]).output()?;
    let selected = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if selected == "Open Settings" {
        open_full_disk_access_settings();
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_full_disk_access_settings() {
    let deep_link = "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";
    if Command::new("open").arg(deep_link).status().is_ok() {
        return;
    }

    // Fallback to the Privacy & Security pane when deep links are unavailable.
    let _ = Command::new("open")
        .arg("/System/Library/PreferencePanes/Security.prefPane")
        .status();
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn marker_path_points_to_teletipo_data_dir() {
        if let Some(path) = marker_path() {
            let s = path.to_string_lossy();
            assert!(s.contains("teletipo"));
            assert!(s.ends_with(MARKER_FILE));
        }
    }
}
