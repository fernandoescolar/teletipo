use std::cell::RefCell;
use std::rc::Rc;

use clap::Parser;
use platform_abstraction::default_shell;
use render_wgpu::{FontConfig, RenderConfig};

use crate::launch::{build_initial_state, load_session, sanitize_terminal_size, save_session};
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

    #[arg(long, help = "Execute a command and exit")]
    exec: Option<String>,

    #[arg(long, help = "Expose Prometheus metrics on 127.0.0.1:9898")]
    metrics: bool,

    #[arg(
        long,
        env = "TELETIPO_RENDERER",
        value_enum,
        default_value_t = RendererBackend::Glow,
        help = "Select GPU renderer backend"
    )]
    renderer: RendererBackend,

    #[command(subcommand)]
    command: Option<commands::Commands>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum RendererBackend {
    Wgpu,
    Glow,
}

pub fn run(
    update_rx: std::sync::mpsc::Receiver<Result<Option<String>, String>>,
) -> std::process::ExitCode {
    let cli = Cli::parse();
    if let Some(cmd) = cli.command {
        return commands::dispatch(cmd);
    }
    onboarding::show_macos_privacy_onboarding_once();
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
    let (rows, cols) = sanitize_terminal_size(cli.rows, cli.cols);
    let state =
        match build_initial_state(rows, cols, cli.exec.as_deref(), &shell, session, update_rx) {
            Ok(state) => Rc::new(RefCell::new(state)),
            Err(err) => {
                tracing::error!(error = %err, "failed to initialize runtime state");
                return std::process::ExitCode::FAILURE;
            }
        };
    let (initial_font_family, initial_font_size) = {
        let s = state.borrow();
        (s.user_config.font.family.clone(), s.user_config.font.size)
    };
    match cli.renderer {
        RendererBackend::Wgpu => {
            let event_ctx = EventCtx::new(Rc::clone(&state));
            let event_ctx_for_frame = event_ctx.clone();
            let event_ctx_for_events = event_ctx.clone();
            let event_ctx_for_window = event_ctx;
            if let Err(err) = render_wgpu::run_gpu_window_live_with_events_and_window(
                move || event_ctx_for_frame.build_snapshot(),
                move |event| event_ctx_for_events.handle_event(event),
                move |window| event_ctx_for_window.install_window(window),
                RenderConfig {
                    initial_size: Some((window_width, window_height)),
                    initial_position: window_pos,
                    font: FontConfig {
                        font_family: initial_font_family,
                        font_size: initial_font_size,
                    },
                    ..RenderConfig::default()
                },
            ) {
                tracing::error!(error = %err, "failed to start wgpu backend");
            }
        }
        RendererBackend::Glow => {
            let event_ctx = EventCtx::new(Rc::clone(&state));
            let event_ctx_for_frame = event_ctx.clone();
            let event_ctx_for_events = event_ctx.clone();
            let event_ctx_for_window = event_ctx;
            if let Err(err) = render_glow::run_gpu_window_live_with_events_and_window(
                move || event_ctx_for_frame.build_snapshot(),
                move |event| event_ctx_for_events.handle_event(event),
                move |window| event_ctx_for_window.install_window(window),
                RenderConfig {
                    initial_size: Some((window_width, window_height)),
                    initial_position: window_pos,
                    font: FontConfig {
                        font_family: initial_font_family,
                        font_size: initial_font_size,
                    },
                    ..RenderConfig::default()
                },
            ) {
                tracing::error!(error = %err, "failed to start glow backend");
            }
        }
    }

    drop(metrics_handle);

    // Persist session state so the next run can restore it.
    save_session(&state.borrow());
    std::process::ExitCode::SUCCESS
}
