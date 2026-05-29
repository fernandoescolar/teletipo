use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use self_update::cargo_crate_version;
use tracing::{error, info, warn};

const SIGNING_KEY: [u8; 32] = [
    0x1f, 0x4f, 0x40, 0x29, 0x89, 0x63, 0x9a, 0xa1, 0x47, 0xb8, 0x37, 0x84, 0x90, 0x6a, 0xdb, 0x96,
    0xdd, 0xa6, 0x50, 0x84, 0xf4, 0x34, 0x90, 0xf1, 0x3d, 0xaf, 0x10, 0x2b, 0xfc, 0x3e, 0x65, 0xe3,
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

fn executable_path() -> io::Result<PathBuf> {
    std::env::current_exe()
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
    builder
        .repo_owner("fernandoescolar")
        .repo_name("teletipo")
        .bin_name("teletipo")
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
    std::thread::spawn(move || {
        let result = try_update();
        if let Err(err) = tx.send(result) {
            error!(error = %err, "failed to deliver update result to UI thread");
        }
    });
    rx
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
}
