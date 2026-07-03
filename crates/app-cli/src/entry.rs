use std::cell::RefCell;
use std::rc::Rc;

use clap::Parser;
use platform_abstraction::default_shell;
use render_glow::{FontConfig, RenderConfig};

use crate::launch::{build_initial_state, load_session, save_session};
use crate::runtime::EventCtx;
use crate::{commands, metrics, onboarding};

#[derive(Debug, Parser)]
#[command(name = "teletipo", version, about = "Modern terminal/editor prototype")]
struct Cli {
    #[arg(long, default_value_t = 24)]
    rows: usize,

    #[arg(long, default_value_t = 80)]
    cols: usize,

    #[arg(long)]
    shell: Option<String>,

    #[arg(
        short = 'e',
        long,
        visible_alias = "execute",
        help = "Execute a command and exit"
    )]
    exec: Option<String>,

    #[arg(
        long,
        visible_alias = "working-directory",
        help = "Start the shell/command in this directory"
    )]
    cwd: Option<String>,

    #[arg(long, help = "Expose Prometheus metrics on 127.0.0.1:9898")]
    metrics: bool,

    #[command(subcommand)]
    command: Option<commands::Commands>,
}

#[allow(clippy::too_many_lines)]
pub fn run(
    update_rx: std::sync::mpsc::Receiver<Result<Option<String>, String>>,
) -> std::process::ExitCode {
    let cli = Cli::parse();
    if let Some(cmd) = cli.command {
        return commands::dispatch(cmd);
    }
    onboarding::show_macos_privacy_onboarding_once();
    crate::mem_report::report("startup");
    let metrics_handle = metrics::install_metrics(cli.metrics);
    let shell = cli.shell.unwrap_or_else(default_shell);

    // ── GPU path ─────────────────────────────────────────────────────────────
    let session = load_session();
    // Clamp to sane logical-pixel bounds — guards against session files that
    // previously stored physical pixels and grew beyond GPU texture limits.
    let window_width = session.window_width.clamp(400, 3840);
    let window_height = session.window_height.clamp(300, 2160);
    let window_pos = match (session.window_x, session.window_y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    let state = match build_initial_state(
        cli.exec.as_deref(),
        cli.cwd.as_deref(),
        &shell,
        session,
        update_rx,
    ) {
        Ok(state) => Rc::new(RefCell::new(state)),
        Err(err) => {
            tracing::error!(error = %err, "failed to initialize runtime state");
            return std::process::ExitCode::FAILURE;
        }
    };
    crate::mem_report::report("state_initialized");

    // Initialize config file watcher
    if let Some(config_dir) = dirs::config_dir() {
        match crate::config_watcher::ConfigWatcher::start(config_dir) {
            Ok(mut watcher) => {
                state.borrow_mut().config_reload_rx = watcher.rx.take();
                // Watcher is stored in a box that lives for the duration of the app
                // (SAFETY: This is intentional - the watcher must remain alive for file monitoring)
                let _ = Box::leak(Box::new(watcher));
            }
            Err(err) => {
                tracing::warn!(error = %err, "failed to initialize config file watcher");
            }
        }
    }

    let (initial_font_family, initial_font_size, initial_opacity) = {
        let s = state.borrow();
        (
            s.user_config.font.family.clone(),
            s.user_config.font.size,
            s.user_config.terminal.opacity,
        )
    };

    let event_ctx = EventCtx::new(Rc::clone(&state));
    let event_ctx_for_frame = event_ctx.clone();
    let event_ctx_for_events = event_ctx.clone();
    let event_ctx_for_window = event_ctx;
    let first_frame = std::sync::Once::new();
    if let Err(err) = render_glow::run_gpu_window_live_with_events_and_window(
        move || {
            first_frame.call_once(|| crate::mem_report::report("first_frame"));
            event_ctx_for_frame.build_snapshot()
        },
        move |event| event_ctx_for_events.handle_event(event),
        move |window, redrawer| event_ctx_for_window.install_window(window, redrawer),
        RenderConfig {
            initial_size: Some((window_width, window_height)),
            initial_position: window_pos,
            font: FontConfig {
                font_family: initial_font_family.clone(),
                font_size: initial_font_size,
            },
            opacity: initial_opacity,
            ..RenderConfig::default()
        },
    ) {
        tracing::error!(error = %err, "failed to start glow backend");
    }

    drop(metrics_handle);

    // Persist session state so the next run can restore it.
    save_session(&state.borrow());
    std::process::ExitCode::SUCCESS
}
