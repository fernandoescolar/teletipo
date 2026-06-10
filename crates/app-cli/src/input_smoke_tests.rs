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
        selected_block: None,
        collapsed_blocks: std::collections::HashSet::new(),
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
        .feed_terminal(b"\x1b]133;A\x07p1\n\x1b]133;B\x07\x1b]133;C\x07out1\n\x1b]133;D;0\x07\x1b]133;A\x07p2\n\x1b]133;B\x07\x1b]133;C\x07out2\n\x1b]133;D;0\x07\x1b]133;A\x07p3\n\x1b]133;B\x07\x1b]133;C\x07out3\n\x1b]133;D;0\x07");

    state.jump_to_prev_prompt();

    assert!(state.tabs[0].scroll_offset > 0);
}

#[test]
fn jump_to_next_prompt_scrolls_forward_after_backjump() {
    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    state.resize_tab(0, 4, 80);
    state.tabs[0].term_row_count = 4;
    state.tabs[0].app.feed_terminal(
        b"\x1b]133;A\x07p1\n\x1b]133;B\x07\x1b]133;C\x07out1\n\x1b]133;D;0\x07\x1b]133;A\x07p2\n\x1b]133;B\x07\x1b]133;C\x07out2\n\x1b]133;D;0\x07\x1b]133;A\x07p3\n\x1b]133;B\x07\x1b]133;C\x07out3\n\x1b]133;D;0\x07\x1b]133;A\x07p4\n\x1b]133;B\x07\x1b]133;C\x07out4\n\x1b]133;D;0\x07\x1b]133;A\x07p5\n\x1b]133;B\x07\x1b]133;C\x07out5\n\x1b]133;D;0\x07",
    );

    state.jump_to_prev_prompt();
    state.jump_to_prev_prompt();
    let prev_selected = state.tabs[0]
        .selected_block
        .expect("selected previous block");

    state.jump_to_next_prompt();

    let next_selected = state.tabs[0].selected_block.expect("selected next block");
    assert!(next_selected.0 > prev_selected.0);
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
fn editor_context_menu_reports_action_availability() {
    use winit::event::{ElementState, MouseButton};

    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    state.tab_mut().app.insert_editor_input("hello");
    input::handle_event(
        &mut state,
        AppWindowEvent::CursorMoved { x: 100.0, y: 500.0 },
    );
    input::handle_event(
        &mut state,
        AppWindowEvent::MouseInput {
            state: ElementState::Pressed,
            button: MouseButton::Right,
        },
    );

    let menu = state
        .overlays
        .context_menu
        .as_ref()
        .expect("editor context menu");
    assert_eq!(
        menu.items,
        ["Undo", "Redo", "Copy", "Cut", "Paste", "Select All"]
    );
    assert_eq!(menu.enabled_items, [true, false, false, false, false, true]);

    state.tab_mut().app.set_editor_cursor(0, false);
    state.tab_mut().app.set_editor_cursor(5, true);
    state.shell_services.clipboard_set("paste me".to_owned());
    input::handle_event(
        &mut state,
        AppWindowEvent::MouseInput {
            state: ElementState::Pressed,
            button: MouseButton::Right,
        },
    );

    let menu = state
        .overlays
        .context_menu
        .as_ref()
        .expect("editor context menu");
    assert_eq!(menu.enabled_items, [true, false, true, true, true, true]);
}

#[test]
fn editor_context_menu_executes_all_actions() {
    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    state.tab_mut().app.insert_editor_input("hello");
    state.tab_mut().app.set_editor_cursor(0, false);
    state.tab_mut().app.set_editor_cursor(5, true);

    input::execute_editor_context_menu_item(&mut state, 2);
    assert_eq!(
        state.shell_services.clipboard_get().as_deref(),
        Some("hello")
    );

    input::execute_editor_context_menu_item(&mut state, 3);
    assert_eq!(state.tab().app.editor_snapshot(), "");

    input::execute_editor_context_menu_item(&mut state, 0);
    assert_eq!(state.tab().app.editor_snapshot(), "hello");

    input::execute_editor_context_menu_item(&mut state, 5);
    assert_eq!(state.tab().app.editor_selection(), Some((0, 5)));

    state.shell_services.clipboard_set("world\r\n".to_owned());
    input::execute_editor_context_menu_item(&mut state, 4);
    assert_eq!(state.tab().app.editor_snapshot(), "world\n");

    input::execute_editor_context_menu_item(&mut state, 0);
    input::execute_editor_context_menu_item(&mut state, 1);
    assert_eq!(state.tab().app.editor_snapshot(), "world\n");
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

#[test]
fn structured_block_actions_copy_edit_and_collapse() {
    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    state.tabs[0].app.feed_terminal(b"\x1b]133;A\x07");
    state.tabs[0]
        .app
        .terminal
        .register_submitted_command("printf hello".to_owned());
    let mut stream = b"prompt\r\n\x1b]133;B\x07\x1b]133;C\x07".to_vec();
    for _ in 0..21 {
        stream.extend_from_slice(b"hello\r\n");
    }
    stream.extend_from_slice(b"\x1b]133;D;0\x07");
    state.tabs[0].app.feed_terminal(&stream);
    let id = state.tabs[0].app.execution_blocks()[0].id;
    state.tabs[0].selected_block = Some(id);

    state.copy_selected_block_command();
    assert_eq!(
        state.shell_services.clipboard_get().as_deref(),
        Some("printf hello")
    );
    state.copy_selected_block_output();
    assert!(
        state
            .shell_services
            .clipboard_get()
            .is_some_and(|output| output.lines().count() >= 20)
    );

    state.edit_selected_block_command();
    assert_eq!(state.tabs[0].app.editor_snapshot(), "printf hello");
    state.toggle_selected_block_collapse();
    assert!(state.tabs[0].collapsed_blocks.contains(&id));
    let rendered = snapshot::build_snapshot(&mut state).terminal_text_from_rows();
    assert!(rendered.contains("output lines · expand"));
    assert!(rendered.contains("✓"));
}

#[test]
fn running_block_header_and_right_click_actions_are_visible_and_enabled() {
    use winit::event::{ElementState, MouseButton};

    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    state.tabs[0].app.feed_terminal(b"\x1b]133;A\x07");
    state.tabs[0]
        .app
        .terminal
        .register_submitted_command("sleep 1".to_owned());
    state.tabs[0]
        .app
        .feed_terminal(b"prompt\x1b]133;B\x07\x1b]133;C\x07");

    let rendered = snapshot::build_snapshot(&mut state).terminal_text_from_rows();
    assert!(rendered.contains("●"), "running status should be visible");
    assert!(
        rendered.contains("▶  E  C  ↓"),
        "quick actions should be visible"
    );

    input::handle_event(&mut state, AppWindowEvent::CursorMoved { x: 20.0, y: 8.0 });
    input::handle_event(
        &mut state,
        AppWindowEvent::MouseInput {
            state: ElementState::Pressed,
            button: MouseButton::Right,
        },
    );

    assert!(state.tab().selected_block.is_some());
    let menu = state.overlays.context_menu.as_ref().expect("terminal menu");
    assert!(menu.enabled_items[3..8].iter().all(|enabled| *enabled));
    assert!(!menu.enabled_items[8]);
}

#[test]
fn quick_action_copy_icon_targets_block_header() {
    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    state.tabs[0].app.feed_terminal(b"\x1b]133;A\x07");
    state.tabs[0]
        .app
        .terminal
        .register_submitted_command("echo quick".to_owned());
    state.tabs[0]
        .app
        .feed_terminal(b"prompt\x1b]133;B\x07\x1b]133;C\x07quick\r\n\x1b]133;D;0\x07");

    assert!(state.activate_block_quick_action(0, 76, 80));
    assert_eq!(
        state.shell_services.clipboard_get().as_deref(),
        Some("echo quick")
    );
}

#[test]
fn terminal_context_menu_last_item_hitbox_matches_drawn_row() {
    use winit::event::{ElementState, MouseButton};

    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    state.tabs[0].app.feed_terminal(b"\x1b]133;A\x07");
    state.tabs[0]
        .app
        .terminal
        .register_submitted_command("echo menu".to_owned());
    state.tabs[0]
        .app
        .feed_terminal(b"prompt\x1b]133;B\x07\x1b]133;C\x07menu\r\n\x1b]133;D;0\x07");
    input::handle_event(&mut state, AppWindowEvent::CursorMoved { x: 20.0, y: 8.0 });
    input::handle_event(
        &mut state,
        AppWindowEvent::MouseInput {
            state: ElementState::Pressed,
            button: MouseButton::Right,
        },
    );

    let row_h = state.layout.cell_h as f64 * 1.4;
    let last = state.overlays.context_menu.as_ref().unwrap().items.len() - 1;
    input::handle_event(
        &mut state,
        AppWindowEvent::CursorMoved {
            x: 30.0,
            y: 8.0 + (last as f64 + 0.5) * row_h,
        },
    );
    assert_eq!(
        state.overlays.context_menu.as_ref().unwrap().hovered_item,
        Some(last)
    );
}

#[test]
fn collapse_all_command_blocks_collapses_every_long_output() {
    let mut state = build_test_state(Box::new(shell::NullShell::default()));
    for command in ["one", "two"] {
        state.tabs[0].app.feed_terminal(b"\x1b]133;A\x07");
        state.tabs[0]
            .app
            .terminal
            .register_submitted_command(command.to_owned());
        state.tabs[0]
            .app
            .feed_terminal(b"prompt\x1b]133;B\x07\x1b]133;C\x07");
        for _ in 0..21 {
            state.tabs[0].app.feed_terminal(b"line\r\n");
        }
        state.tabs[0].app.feed_terminal(b"\x1b]133;D;0\x07");
    }
    state.collapse_all_command_blocks();
    assert_eq!(state.tabs[0].collapsed_blocks.len(), 2);
}
