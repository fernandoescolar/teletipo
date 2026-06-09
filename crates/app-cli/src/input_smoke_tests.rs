//! Headless smoke tests that drive [`crate::input::handle_event`] against a
//! [`crate::shell::NullShell`]-backed [`crate::GpuRuntimeState`] without
//! spawning a PTY or opening a window.

use app_orchestrator::App;
use render_wgpu::AppWindowEvent;

use crate::config;
use crate::input;
use crate::launch;
use crate::runtime::GpuRuntimeState;
use crate::search;
use crate::settings::SettingsUiState;
use crate::shell;
use crate::snapshot;
use crate::state::{
    CursorState, DragState, LayoutState, ModifierState, OverlayState, ThemeFontState,
};
use crate::tab;

/// Construct a minimal [`GpuRuntimeState`] with a single PTY-less tab and
/// the provided [`shell::AppShell`] implementation. Suitable for tests that
/// only need to exercise event dispatch — no window, no shell process.
fn build_test_state(shell_services: Box<dyn shell::AppShell>) -> GpuRuntimeState {
    let app = App::new(24, 80).expect("valid terminal size");
    let tab = tab::TabState {
        app,
        pty: None,
        scroll_offset: 0,
        editor_scroll_offset: 0,
        editor_horizontal_scroll_offset: 0,
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
        unread_output: false,
        bell_pending: false,
        a11y_screen_version: 0,
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
        update_last_checked: std::time::Instant::now(),
        settings: SettingsUiState::default(),
        command_palette: None,
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
    use winit::keyboard::ModifiersState as WinitModifiers;
    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    let mods = WinitModifiers::SUPER | WinitModifiers::SHIFT;
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
fn repeated_commands_are_kept_in_history() {
    let mut state = build_test_state(Box::new(shell::NullShell::default()));

    state.tabs[0].pending_cmd = Some("cargo test".to_owned());
    state.finalize_pending_cmd(0, 0);
    state.tabs[0].pending_cmd = Some("cargo test".to_owned());
    state.finalize_pending_cmd(0, 0);

    assert_eq!(
        state.tabs[0].history,
        vec!["cargo test".to_owned(), "cargo test".to_owned()]
    );
    assert_eq!(state.tabs[0].history_entries[0].count, 2);
}

#[test]
fn jump_to_prev_prompt_scrolls_back() {
    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    state.resize_tab(0, 4, 80);
    state.tabs[0].term_row_count = 4;
    state.tabs[0]
        .app
        .feed_terminal(b"\x1b]133;A\x07p1\nout1\n\x1b]133;A\x07p2\nout2\n\x1b]133;A\x07p3\nout3\n");

    state.jump_to_prev_prompt();

    assert!(state.tabs[0].scroll_offset > 0);
}

#[test]
fn jump_to_next_prompt_scrolls_forward_after_backjump() {
    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    state.resize_tab(0, 4, 80);
    state.tabs[0].term_row_count = 4;
    state.tabs[0].app.feed_terminal(
        b"\x1b]133;A\x07p1\nout1\n\x1b]133;A\x07p2\nout2\n\x1b]133;A\x07p3\nout3\n\x1b]133;A\x07p4\nout4\n\x1b]133;A\x07p5\nout5\n",
    );

    state.jump_to_prev_prompt();
    state.jump_to_prev_prompt();
    let visible_rows = state.tabs[0].term_row_count.max(1);
    let scrollback = state.tabs[0].app.scrollback_len();
    let total_rows = scrollback.saturating_add(visible_rows);
    let prev_window_start = total_rows
        .saturating_sub(visible_rows)
        .saturating_sub(state.tabs[0].scroll_offset.min(scrollback));
    let prev_selected = prev_window_start + state.tabs[0].selection_anchor.unwrap().0;

    state.jump_to_next_prompt();

    let next_window_start = total_rows
        .saturating_sub(visible_rows)
        .saturating_sub(state.tabs[0].scroll_offset.min(scrollback));
    let next_selected = next_window_start + state.tabs[0].selection_anchor.unwrap().0;

    assert!(next_selected > prev_selected);
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
