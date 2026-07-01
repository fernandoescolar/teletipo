//! Per-frame mutations and housekeeping operations.
//!
//! This module contains all operations that modify `GpuRuntimeState` during
//! frame rendering. These are separated from read-only view construction to
//! clarify the flow: mutations happen first, then the snapshot is built.

use crate::GpuRuntimeState;

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
