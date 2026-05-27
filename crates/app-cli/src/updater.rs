use std::sync::mpsc;

use self_update::cargo_crate_version;

/// Maps the current OS to the asset-name fragment used in GitHub Release artifacts:
///   macOS   → `teletipo-macos-universal.tar.gz`
///   Linux   → `teletipo-linux-x86_64.tar.gz`
///   Windows → `teletipo-windows-x86_64.zip`
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

/// Checks for a newer GitHub release and, if found, silently downloads and
/// replaces the binary in-place. Returns the new version string on success,
/// or `None` when already up-to-date or on any error.
fn try_update() -> Option<String> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("fernandoescolar")
        .repo_name("teletipo")
        .bin_name("teletipo")
        .target(asset_target())
        .show_download_progress(false)
        .show_output(false)
        .no_confirm(true)
        .current_version(cargo_crate_version!())
        .build()
        .ok()?
        .update()
        .ok()?;
    match status {
        self_update::Status::Updated(v) => Some(v),
        self_update::Status::UpToDate(_) => None,
    }
}

/// Spawns a background thread that silently checks for and applies any
/// available update. Sends `Some(version)` if the binary was replaced,
/// `None` if already up-to-date or on any error.
pub fn spawn_update() -> mpsc::Receiver<Option<String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        tx.send(try_update()).ok();
    });
    rx
}
