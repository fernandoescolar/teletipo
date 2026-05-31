use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use self_update::cargo_crate_version;
use tracing::{error, info, warn};

const SIGNING_KEY: [u8; 32] = [
    0xbc, 0x53, 0xae, 0x98, 0x0c, 0xaf, 0xb1, 0x23, 0xe6, 0xc0, 0xe4, 0xfe, 0xe2, 0x6f, 0x02, 0xb9,
    0x82, 0x54, 0x6d, 0xdb, 0xe0, 0x53, 0xc6, 0xdc, 0x8d, 0xa0, 0x8d, 0xb3, 0xde, 0x92, 0xbe, 0x1f,
];

fn asset_target() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos-universal"
    } else if cfg!(target_os = "linux") {
        "linux-x86_64"
    } else if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else {
        "unknown"
    }
}

fn bin_path_in_archive() -> String {
    let bin = format!("teletipo{}", std::env::consts::EXE_SUFFIX);
    if cfg!(target_os = "windows") {
        bin
    } else {
        format!("teletipo-{}/{bin}", asset_target())
    }
}

fn executable_path() -> io::Result<PathBuf> {
    std::env::current_exe()
}

fn is_running_from_macos_app_bundle(exe_path: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let Some(macos_dir) = exe_path.parent() else {
        return false;
    };
    if macos_dir.file_name().and_then(|s| s.to_str()) != Some("MacOS") {
        return false;
    }
    let Some(contents_dir) = macos_dir.parent() else {
        return false;
    };
    if contents_dir.file_name().and_then(|s| s.to_str()) != Some("Contents") {
        return false;
    }
    let Some(app_dir) = contents_dir.parent() else {
        return false;
    };
    app_dir
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
}

fn self_update_allowed(_exe_path: &Path) -> bool {
    true
}

/// After replacing the binary inside a macOS .app bundle the original
/// code-signature is invalidated.  Re-apply an ad-hoc signature so that
/// Gatekeeper / TCC does not block the updated app on next launch.
#[cfg(target_os = "macos")]
fn try_resign_macos_bundle(exe_path: &Path) {
    // Walk up: exe → MacOS/ → Contents/ → Teletipo.app
    let Some(app_dir) = exe_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    else {
        return;
    };
    if !app_dir
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
    {
        return;
    }
    match std::process::Command::new("codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(app_dir)
        .output()
    {
        Ok(out) if out.status.success() => {
            info!(bundle = %app_dir.display(), "re-applied ad-hoc signature after update");
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            warn!(bundle = %app_dir.display(), %stderr, "could not re-sign bundle after update");
        }
        Err(err) => {
            warn!(error = %err, "codesign not available; skipping re-signing");
        }
    }
}

fn rollback_backup_path(exe_path: &Path) -> PathBuf {
    let file_name = exe_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("teletipo");
    exe_path.with_file_name(format!("{file_name}.bak"))
}

fn backup_current_executable(exe_path: &Path) -> io::Result<PathBuf> {
    let backup_path = rollback_backup_path(exe_path);
    fs::copy(exe_path, &backup_path)?;
    Ok(backup_path)
}

fn build_updater() -> anyhow::Result<Box<dyn self_update::update::ReleaseUpdate>> {
    let mut builder = self_update::backends::github::Update::configure();
    let bin_path = bin_path_in_archive();
    builder
        .repo_owner("fernandoescolar")
        .repo_name("teletipo")
        .bin_name("teletipo")
        .bin_path_in_archive(&bin_path)
        .target(asset_target())
        .show_download_progress(false)
        .show_output(false)
        .no_confirm(true)
        .current_version(cargo_crate_version!())
        .verifying_keys([SIGNING_KEY]);
    Ok(builder.build()?)
}

#[tracing::instrument]
fn try_update() -> Result<Option<String>, String> {
    let exe_path =
        executable_path().map_err(|err| format!("could not resolve current executable: {err}"))?;
    let backup_path = backup_current_executable(&exe_path)
        .map_err(|err| format!("could not save rollback backup: {err}"))?;

    let updater = build_updater().map_err(|err| err.to_string())?;
    let status = updater.update().map_err(|err| {
        warn!(error = %err, backup = %backup_path.display(), "update failed; rollback backup preserved");
        err.to_string()
    })?;

    match status {
        self_update::Status::Updated(v) => {
            info!(version = %v, backup = %backup_path.display(), "update applied");
            #[cfg(target_os = "macos")]
            if is_running_from_macos_app_bundle(&exe_path) {
                try_resign_macos_bundle(&exe_path);
            }
            Ok(Some(v))
        }
        self_update::Status::UpToDate(_) => {
            info!(backup = %backup_path.display(), "no update available");
            Ok(None)
        }
    }
}

#[tracing::instrument]
pub fn spawn_update() -> mpsc::Receiver<Result<Option<String>, String>> {
    let (tx, rx) = mpsc::channel();
    let exe_path = executable_path().ok();
    if let Some(ref path) = exe_path
        && !self_update_allowed(path)
    {
        return rx;
    }
    std::thread::spawn(move || {
        let result = try_update();
        if let Err(err) = tx.send(result) {
            error!(error = %err, "failed to deliver update result to UI thread");
        }
    });
    rx
}

/// Restart the current process by re-executing it in-place.
///
/// On Unix this uses `exec()` to replace the process image; on Windows it
/// spawns a new instance and exits.  The function only returns on error (e.g.
/// permission denied or executable not found).
pub(crate) fn restart_app() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let exe = match executable_path() {
        Ok(p) => p,
        Err(e) => {
            error!(error = %e, "restart: could not resolve executable path");
            return;
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&exe).args(&args[1..]).exec();
        error!(error = %err, "restart: exec failed");
    }
    #[cfg(not(unix))]
    {
        match std::process::Command::new(&exe).args(&args[1..]).spawn() {
            Ok(_) => std::process::exit(0),
            Err(e) => error!(error = %e, "restart: spawn failed"),
        }
    }
}

#[tracing::instrument]
pub(crate) fn rollback_latest_update() -> anyhow::Result<bool> {
    let exe_path = executable_path()?;
    let backup_path = rollback_backup_path(&exe_path);
    if !backup_path.exists() {
        return Ok(false);
    }
    info!(backup = %backup_path.display(), current = %exe_path.display(), "rolling back update");
    self_update::self_replace::self_replace(backup_path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rollback_backup_path_preserves_filename_and_appends_suffix() {
        let path = Path::new("/tmp/teletipo");
        assert_eq!(
            rollback_backup_path(path),
            PathBuf::from("/tmp/teletipo.bak")
        );
    }

    #[test]
    fn rollback_backup_path_handles_exe_suffix() {
        let path = Path::new("/tmp/teletipo.exe");
        assert_eq!(
            rollback_backup_path(path),
            PathBuf::from("/tmp/teletipo.exe.bak")
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn bin_path_in_archive_matches_windows_layout() {
        assert_eq!(bin_path_in_archive(), "teletipo.exe");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn bin_path_in_archive_matches_unix_layout() {
        assert_eq!(
            bin_path_in_archive(),
            format!("teletipo-{}/teletipo", asset_target())
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn detects_macos_app_bundle_layout() {
        let path = Path::new("/Applications/Teletipo.app/Contents/MacOS/teletipo");
        assert!(is_running_from_macos_app_bundle(path));
    }

    #[test]
    fn rejects_non_bundle_layout() {
        let path = Path::new("/usr/local/bin/teletipo");
        assert!(!is_running_from_macos_app_bundle(path));
    }
}
