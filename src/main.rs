#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::OnceLock;

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Use `mimalloc` instead of the system allocator. Long-running GUI processes
/// on macOS frequently hold inflated RSS because the default allocator caches
/// freed pages aggressively and fragments under wgpu/winit's mixed allocation
/// patterns. mimalloc typically returns memory to the OS sooner and yields a
/// 20–40 % RSS reduction here for the same workload.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

static LOG_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

fn main() -> std::process::ExitCode {
    init_tracing();
    let update_rx = app_cli::updater::spawn_update();
    app_cli::run(update_rx)
}

fn log_dir() -> Option<PathBuf> {
    let dir = dirs::data_local_dir()?.join("teletipo").join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Initialize the global tracing subscriber.
///
/// Honors the `RUST_LOG` environment variable; defaults to `warn` for the whole
/// process and `info` for the `teletipo` family of crates so user-facing
/// warnings and errors are visible without flooding stderr.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,app_cli=info,app_orchestrator=info,render_wgpu=info")
    });
    if let Some(dir) = log_dir() {
        let file_appender = tracing_appender::rolling::daily(dir, "teletipo.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let _ = LOG_GUARD.set(guard);
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .with_target(false)
                    .with_ansi(false)
                    .with_writer(non_blocking),
            )
            .try_init();
        return;
    }

    // Fallback to stderr only when log directory creation fails.
    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
}
