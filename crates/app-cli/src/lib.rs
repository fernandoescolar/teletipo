#![doc = "Application runtime, event loop orchestration, and CLI entry helpers."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod commands;
mod completion;
mod consts;
mod layout;
mod metrics;
mod onboarding;
mod runtime;
mod search;
mod shell;
mod state;

mod config;
mod coords;
mod input;
mod launch;
mod settings;
mod snapshot;
mod tab;
mod theme;
pub mod updater;
use clap::Parser;
use platform_abstraction::default_shell;
use render_wgpu::{FontConfig, RenderConfig, run_gpu_window_live_with_events_and_window};
use std::cell::RefCell;
use std::rc::Rc;

pub(crate) use completion::suggestion_matches_frecency;
use launch::{build_initial_state, load_session, sanitize_terminal_size, save_session};
use runtime::EventCtx;
pub(crate) use runtime::GpuRuntimeState;
pub(crate) use settings::SettingsUiState;
pub(crate) use state::{
    CursorState, DragState, LayoutState, ModifierState, OverlayState, ThemeFontState, UpdateBanner,
};

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

    #[command(subcommand)]
    command: Option<commands::Commands>,
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
    let event_ctx = EventCtx::new(Rc::clone(&state));
    let event_ctx_for_frame = event_ctx.clone();
    let event_ctx_for_events = event_ctx.clone();
    let event_ctx_for_window = event_ctx;

    if let Err(err) = run_gpu_window_live_with_events_and_window(
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
        tracing::error!(error = %err, "failed to start gpu backend");
    }

    drop(metrics_handle);

    // Persist session state so the next run can restore it.
    save_session(&state.borrow());
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod input_smoke_tests {
    //! Headless smoke tests that drive [`input::handle_event`] against a
    //! [`shell::NullShell`]-backed [`GpuRuntimeState`] without spawning a PTY
    //! or opening a window.

    use super::*;
    use render_wgpu::AppWindowEvent;

    /// Construct a minimal [`GpuRuntimeState`] with a single PTY-less tab and
    /// the provided [`shell::AppShell`] implementation. Suitable for tests that
    /// only need to exercise event dispatch — no window, no shell process.
    fn build_test_state(shell_services: Box<dyn shell::AppShell>) -> GpuRuntimeState {
        let app = app_orchestrator::App::new(24, 80).expect("valid terminal size");
        let tab = tab::TabState {
            app,
            pty: None,
            scroll_offset: 0,
            editor_scroll_offset: 0,
            history: Vec::new(),
            history_index: None,
            saved_input: String::new(),
            split_ratio: 0.5,
            was_terminal_fullscreen: false,
            pre_fullscreen_split_ratio: 0.5,
            selection_anchor: None,
            selection_anchor_scroll: 0,
            selection_end: None,
            selection_end_scroll: 0,
            is_selecting: false,
            is_selecting_editor: false,
            last_terminal_text: String::new(),
            term_row_count: 24,
            cwd: String::new(),
            suggestion_prefix: None,
            suggestion_index: None,
            history_entries: Vec::new(),
            pending_cmd: None,
            shell_integration: false,
            search: search::SearchState::default(),
            command_running: false,
        };
        GpuRuntimeState {
            tabs: vec![tab],
            active_tab: 0,
            shell: "/bin/sh".to_owned(),
            modifiers: ModifierState::default(),
            layout: LayoutState {
                window_width: 800,
                window_height: 600,
                window_x: 0,
                window_y: 0,
                scale_factor: 1.0,
                cell_w: 8.0,
                cell_h: 16.0,
            },
            cursor: CursorState::default(),
            drag: DragState::default(),
            overlays: OverlayState::default(),
            user_config: config::UserConfig::default(),
            config_error: None,
            themes_fonts: ThemeFontState::default(),
            update_rx: None,
            settings: SettingsUiState::default(),
            should_exit: false,
            shell_services,
        }
    }

    #[test]
    fn cursor_moved_event_updates_cursor_state() {
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        input::handle_event(
            &mut state,
            AppWindowEvent::CursorMoved {
                x: 123.5,
                y: 456.25,
            },
        );
        assert_eq!(state.cursor.cursor_x, 123.5);
        assert_eq!(state.cursor.cursor_y, 456.25);
    }

    #[test]
    fn window_moved_event_updates_layout() {
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        input::handle_event(&mut state, AppWindowEvent::WindowMoved { x: 42, y: -7 });
        assert_eq!(state.layout.window_x, 42);
        assert_eq!(state.layout.window_y, -7);
    }

    #[test]
    fn modifiers_changed_event_updates_modifier_state() {
        use winit::keyboard::ModifiersState;
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        let mods = ModifiersState::SUPER | ModifiersState::SHIFT;
        input::handle_event(&mut state, AppWindowEvent::ModifiersChanged(mods));
        assert!(state.modifiers.super_down);
        assert!(state.modifiers.shift_down);
        assert!(!state.modifiers.ctrl_down);
    }

    #[test]
    fn ime_commit_event_inserts_text_into_editor() {
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        input::handle_event(&mut state, AppWindowEvent::ImeCommit("héllo".to_owned()));
        assert!(state.tab().app.editor_snapshot().contains("héllo"));
    }

    #[test]
    fn resized_event_updates_layout_dimensions() {
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        input::handle_event(
            &mut state,
            AppWindowEvent::Resized {
                width: 1024,
                height: 768,
                scale_factor: 2.0,
                cell_w: 10.0,
                cell_h: 20.0,
            },
        );
        assert_eq!(state.layout.window_width, 1024);
        assert_eq!(state.layout.window_height, 768);
        assert_eq!(state.layout.scale_factor, 2.0);
        assert_eq!(state.layout.cell_w, 10.0);
        assert_eq!(state.layout.cell_h, 20.0);
    }

    #[test]
    fn failed_command_is_kept_in_history() {
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        state.tabs[0].pending_cmd = Some("cargo test".to_owned());

        state.finalize_pending_cmd(0, 1);

        assert_eq!(state.tabs[0].pending_cmd, None);
        assert_eq!(state.tabs[0].history, vec!["cargo test".to_owned()]);
        assert_eq!(state.tabs[0].history_entries.len(), 1);
        assert_eq!(state.tabs[0].history_entries[0].cmd, "cargo test");
        assert_eq!(state.tabs[0].history_entries[0].count, 1);
    }

    #[test]
    fn null_shell_clipboard_roundtrip_via_state() {
        // Verifies the NullShell wired through `shell_services` is reachable
        // from within state and round-trips clipboard text correctly — the
        // contract the Cmd+C / Cmd+V keyboard handlers rely on.
        let mut state = build_test_state(Box::new(shell::NullShell::default()));
        state.shell_services.clipboard_set("smoke-test".to_owned());
        assert_eq!(
            state.shell_services.clipboard_get().as_deref(),
            Some("smoke-test"),
        );
    }

    #[test]
    fn startup_snapshot_renders_command_output() {
        use std::sync::mpsc::channel;
        use std::time::{Duration, Instant};

        let (update_tx, update_rx) = channel();
        drop(update_tx);

        let session = tab::PersistentSession::default();
        let mut state = launch::build_initial_state(
            24,
            80,
            Some("printf 'hello from teletipo\n'"),
            "/bin/sh",
            session,
            update_rx,
        )
        .expect("build initial state");

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut snapshot = snapshot::build_snapshot(&mut state);
        while !snapshot.terminal_text.contains("hello from teletipo") {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for PTY output"
            );
            std::thread::sleep(Duration::from_millis(20));
            snapshot = snapshot::build_snapshot(&mut state);
        }

        assert!(snapshot.terminal_text.contains("hello from teletipo"));
        assert_eq!(snapshot.active_tab, 0);
        assert_eq!(snapshot.scroll_offset, 0);
    }
}
