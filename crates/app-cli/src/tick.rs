//! Per-frame mutations and housekeeping operations.
//!
//! This module contains all operations that modify `GpuRuntimeState` during
//! frame rendering. These are separated from read-only view construction to
//! clarify the flow: mutations happen first, then the snapshot is built.

use crate::GpuRuntimeState;

/// Store damage info extracted from terminal during a frame, for use during snapshot building.
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct FrameDamage {
    /// Terminal damage region extracted this frame.
    pub full_redraw: bool,
    pub dirty_rows: Vec<usize>,
}

use render_model::Toast;
use render_model::ToastKind;

/// Per-frame housekeeping: polls update channel, autosaves session, handles deferred
/// resize, pumps PTYs, advances cursor blink, and resets per-tab read indicators.
pub(crate) fn housekeeping(state: &mut GpuRuntimeState) {
    // Clear one-shot just_saved flag after it has been shown for a frame.
    if state.settings.just_saved {
        state.settings.just_saved = false;
    }

    // Poll the background update-check thread (once; then drop the receiver).
    if let Some(ref rx) = state.update_rx {
        match rx.try_recv() {
            Ok(Ok(Some(version))) => {
                state.overlays.pending_update = Some(crate::UpdateBanner::Available(version));
                state.update_rx = None;
            }
            Ok(Ok(None)) => {
                state.update_rx = None;
            }
            Ok(Err(err)) => {
                state.overlays.pending_update = Some(crate::UpdateBanner::Failed(err));
                state.update_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.update_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    // Poll the config reload file watcher (non-blocking, try to reload config).
    if let Some(ref rx) = state.config_reload_rx {
        match rx.try_recv() {
            Ok(_) => {
                // Config file changed; attempt to reload
                match crate::config::reload_config_safe(&state.user_config) {
                    Ok(new_config) => {
                        state.user_config = new_config;
                        state.push_toast(
                            "Config reloaded".to_string(),
                            crate::state::ToastKind::Success,
                        );
                    }
                    Err(err) => {
                        state.push_toast(err, crate::state::ToastKind::Error);
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.config_reload_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    // Autosave session every 5 minutes when session restore is enabled.
    const AUTOSAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
    if state.user_config.terminal.restore_session
        && state.last_session_save.elapsed() >= AUTOSAVE_INTERVAL
    {
        crate::launch::save_session(state);
        state.last_session_save = std::time::Instant::now();
    }

    // Re-arm the update check once per day while the app stays open.
    const UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
    if state.update_rx.is_none()
        && state.overlays.pending_update.is_none()
        && state.update_last_checked.elapsed() >= UPDATE_INTERVAL
    {
        state.update_rx = Some(crate::updater::spawn_update());
        state.update_last_checked = std::time::Instant::now();
    }

    // Apply deferred resize once window resizing has been idle for ≥ 150 ms.
    if state
        .overlays
        .pending_pty_resize
        .is_some_and(|t| t.elapsed().as_millis() >= 150)
        && !state.drag.dragging_separator
    {
        state.apply_deferred_resize();
    }

    let had_data = state.pump_all_ptys();
    if had_data {
        let active = state.active_tab;
        state.tabs[active].scroll_offset = 0;
        state.overlays.cursor_blink_last = std::time::Instant::now();
        state.overlays.cursor_blink_phase = true;
    }

    // Advance cursor blink: toggle every 500 ms.
    if state.overlays.cursor_blink_last.elapsed().as_millis() >= crate::consts::BLINK_HALF_MS {
        state.overlays.cursor_blink_phase = !state.overlays.cursor_blink_phase;
        state.overlays.cursor_blink_last = std::time::Instant::now();
    }

    // The active tab is always "read" — clear any pending unread indicator.
    state.tabs[state.active_tab].unread_output = false;
    state.tabs[state.active_tab].bell_pending = false;

    // Show a one-shot toast for config parse errors on startup.
    if let Some(err) = state.config_error.take() {
        state.push_toast(
            format!("Config error: {err}"),
            crate::state::ToastKind::Error,
        );
    }
}

/// Build the transient overlay label shown in the top-right corner (resize, PTY status, etc.).
#[allow(dead_code)]
pub(crate) fn build_resize_overlay(state: &mut GpuRuntimeState) -> Option<String> {
    if let Some(ref banner) = state.overlays.pending_update {
        return Some(match banner {
            crate::UpdateBanner::Available(v) => {
                format!("Update ready v{v} \u{2014} click to restart")
            }
            crate::UpdateBanner::Failed(err) => format!("Update failed: {err}"),
        });
    }
    if let Some((ref t, ref message)) = state.overlays.pty_status {
        if t.elapsed().as_secs_f32() < 2.5 {
            return Some(message.clone());
        }
        state.overlays.pty_status = None;
    }
    if let Some((ref t, cols, rows)) = state.overlays.last_resize {
        if t.elapsed().as_secs_f32() < 1.0 {
            return Some(format!("{cols}\u{d7}{rows}"));
        }
        state.overlays.last_resize = None;
    }
    if let Some((ref t, ref label)) = state.overlays.last_cmd_duration {
        if t.elapsed().as_secs_f32() < 4.0 {
            return Some(label.clone());
        }
        state.overlays.last_cmd_duration = None;
    }
    None
}

/// Refresh CWD labels, compute tab button labels, and compute drag insert position.
#[allow(dead_code)]
pub(crate) fn build_tab_bar(state: &mut GpuRuntimeState) -> (Vec<String>, Option<usize>) {
    // Refresh cwd labels from child process (best-effort; silent on failure).
    let n_tabs = state.tabs.len();
    for i in 0..n_tabs {
        if let Some(pid) = state.tabs[i].pty.as_ref().and_then(|p| p.child_pid())
            && let Some(new_cwd) = crate::coords::read_child_cwd(pid)
        {
            state.tabs[i].cwd = new_cwd;
        }
    }
    let tab_labels: Vec<String> = if state.tabs.len() > 1 {
        let n = state.tabs.len();
        let add_btn_w = state.layout.cell_w * 2.0;
        let tab_area_w = state.layout.window_width as f32 - add_btn_w;
        let tab_w_px = tab_area_w / n as f32;
        let max_chars = crate::snapshot::tab_button_max_chars(tab_w_px, state.layout.cell_w);
        state
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let label = crate::snapshot::tab_button_label_for_tab(
                    index,
                    tab.app.window_title(),
                    &tab.cwd,
                    tab.command_running,
                    tab.pending_cmd.as_deref(),
                    max_chars,
                );
                if index != state.active_tab {
                    let mut marker = String::new();
                    if tab.bell_pending {
                        marker.push('!');
                    }
                    if tab.unread_output {
                        marker.push('•');
                    }
                    if marker.is_empty() {
                        label
                    } else {
                        format!("{marker} {label}")
                    }
                } else {
                    label
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let tab_drag_insert_before = state.drag.tab_drag.and_then(|_| {
        if state.tabs.len() <= 1 {
            return None;
        }
        if (state.cursor.cursor_x - state.drag.tab_drag_start_x).abs() > 5.0 {
            let n = state.tabs.len();
            let add_btn_w = state.layout.cell_w as f64 * 2.0;
            let tab_area_w = (state.layout.window_width as f64 - add_btn_w).max(1.0);
            let frac = (state.cursor.cursor_x / tab_area_w).clamp(0.0, 1.0);
            Some((frac * n as f64).round() as usize)
        } else {
            None
        }
    });
    (tab_labels, tab_drag_insert_before)
}

/// GC expired toasts and convert them to the renderer's `Toast` type.
#[allow(dead_code)]
pub(crate) fn collect_toasts(state: &mut GpuRuntimeState) -> Vec<Toast> {
    let now = std::time::Instant::now();
    state.overlays.toasts.retain(|t| t.expires_at > now);
    state
        .overlays
        .toasts
        .iter()
        .map(|t| Toast {
            text: t.text.clone(),
            kind: match t.kind {
                crate::state::ToastKind::Info => ToastKind::Info,
                crate::state::ToastKind::Success => ToastKind::Success,
                crate::state::ToastKind::Warn => ToastKind::Warn,
                crate::state::ToastKind::Error => ToastKind::Error,
            },
        })
        .collect()
}
