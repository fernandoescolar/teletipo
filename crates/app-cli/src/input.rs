pub(crate) mod keyboard;
mod pointer;

#[cfg(test)]
pub(crate) use pointer::execute_editor_context_menu_item;

use crate::GpuRuntimeState;
use render_glow::AppWindowEvent;

pub(crate) fn handle_event(state: &mut GpuRuntimeState, event: AppWindowEvent) {
    if pointer::handle_event(state, &event) {
        return;
    }
    if let AppWindowEvent::DroppedFile(ref path) = event {
        handle_dropped_file(state, path);
        return;
    }
    if let AppWindowEvent::WindowFocused(focused) = event {
        state.window_focused = focused;
        return;
    }
    keyboard::handle_event(state, event);
}

/// Handle a file dropped onto the window.
///
/// * `.yaml` files are loaded as theme files and applied immediately.
/// * Any other file has its path quoted and pasted into the editor input.
fn handle_dropped_file(state: &mut GpuRuntimeState, path: &std::path::Path) {
    use crate::state::ToastKind;

    // Check if it looks like a theme file.
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml") {
        // Try to load the YAML file as a theme.
        let result = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|data| {
                serde_yaml::from_str::<crate::theme::ThemeFile>(&data).map_err(|e| e.to_string())
            });
        match result {
            Ok(tf) => {
                let name = tf.name.clone();
                crate::settings::apply_theme_file(&mut state.user_config, &tf);
                crate::config::save_config(&state.user_config);
                // Reload available themes list to include this one if it's new.
                state.themes_fonts.available_themes = crate::theme::load_themes();
                if let Some(idx) = state
                    .themes_fonts
                    .available_themes
                    .iter()
                    .position(|t| t.name == name)
                {
                    state.themes_fonts.active_theme_idx = Some(idx);
                }
                state.push_toast(format!("Theme applied: {name}"), ToastKind::Success);
            }
            Err(err) => {
                state.push_toast(format!("Invalid theme file: {err}"), ToastKind::Error);
            }
        }
        return;
    }

    // For any other file, insert the path into the editor input.
    let path_str = path.to_string_lossy();
    // Shell-quote the path if it contains spaces or special characters.
    let quoted = if path_str.contains(' ') || path_str.contains('\'') {
        format!("\"{}\"", path_str.replace('"', "\\\""))
    } else {
        path_str.into_owned()
    };
    let active = state.active_tab;
    state.tabs[active].app.insert_editor_input(&quoted);
}
