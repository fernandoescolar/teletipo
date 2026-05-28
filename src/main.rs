use tracing_subscriber::{EnvFilter, fmt};

fn main() -> std::process::ExitCode {
    init_tracing();
    let update_rx = app_cli::updater::spawn_update();
    app_cli::run(update_rx)
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
    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
}
