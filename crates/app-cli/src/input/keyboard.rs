use crate::coords::{clamp_editor_scroll, editor_cursor_row_col, editor_row_col_to_offset, extract_selection};
use crate::settings;
use crate::GpuRuntimeState;
use arboard::Clipboard;
use render_wgpu::AppWindowEvent;
use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

pub(super) fn handle_event(state: &mut GpuRuntimeState, event: AppWindowEvent) {
    let AppWindowEvent::KeyboardInput(key_event) = &event else {
        if let AppWindowEvent::ImeCommit(text) = event {
            if !text.is_empty() && text != "\r" && text != "\n" {
                state.tab_mut().app.insert_editor_input(text.as_str());
            }
        }
        return;
    };

    if key_event.state != ElementState::Pressed {
        return;
    }

    if state.settings.open {
        settings::handle_settings_key(state, key_event);
        return;
    }

    match &key_event.logical_key {
        Key::Named(NamedKey::Escape) => state.send_terminal_input(b"\x1b"),
        Key::Named(NamedKey::Tab) => {
            let editor_text = state.tab().app.editor_snapshot();
            state.send_terminal_input(b"\x15");
            if !editor_text.is_empty() {
                state.send_terminal_input(editor_text.as_bytes());
            }
            state.send_terminal_input(b"\t");
        }

        Key::Named(NamedKey::PageUp) => {
            let max_scroll = state.tab().app.scrollback_len();
            let prev = state.tab().scroll_offset;
            state.tab_mut().scroll_offset = prev.saturating_add(5).min(max_scroll);
        }
        Key::Named(NamedKey::PageDown) => {
            let prev = state.tab().scroll_offset;
            state.tab_mut().scroll_offset = prev.saturating_sub(5);
        }

        Key::Character(ch) if state.super_down => {
            match ch.as_str() {
                "," => {
                    state.settings.open = true;
                    state.settings.cursor = 0;
                    state.settings.edit_buf = None;
                    return;
                }
                "c" => {
                    if let Some((start, end)) = state.tab().app.editor_selection() {
                        let editor_text = state.tab().app.editor_snapshot();
                        let selected = editor_text[start..end].to_string();
                        if !selected.is_empty() {
                            if let Ok(mut cb) = Clipboard::new() {
                                let _ = cb.set_text(selected);
                            }
                        }
                    } else {
                        let anchor = state.tab().selection_anchor;
                        let sel_end = state.tab().selection_end;
                        if let (Some(anchor), Some(sel_end)) = (anchor, sel_end) {
                            let last_text = state.tab().last_terminal_text.clone();
                            let text = extract_selection(&last_text, anchor, sel_end);
                            if !text.is_empty() {
                                if let Ok(mut cb) = Clipboard::new() {
                                    let _ = cb.set_text(text);
                                }
                            }
                        }
                    }
                }
                "v" => {
                    if let Ok(mut cb) = Clipboard::new() {
                        if let Ok(text) = cb.get_text() {
                            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
                            if !normalized.is_empty() {
                                state.tab_mut().app.insert_editor_input(&normalized);
                            }
                        }
                    }
                }
                "t" => state.add_new_tab(),
                "w" => {
                    let idx = state.active_tab;
                    state.close_tab(idx);
                }
                "[" => {
                    state.active_tab = state.active_tab.saturating_sub(1);
                }
                "]" => {
                    let last = state.tabs.len() - 1;
                    state.active_tab = (state.active_tab + 1).min(last);
                }
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                    if let Some(d) = ch.as_str().chars().next().and_then(|c| c.to_digit(10)) {
                        let idx = (d as usize - 1).min(state.tabs.len() - 1);
                        state.active_tab = idx;
                    }
                }
                _ => {}
            }
        }

        Key::Character(ch) if state.ctrl_down && ch.as_str() == "," => {
            state.settings.open = true;
            state.settings.cursor = 0;
            state.settings.edit_buf = None;
        }

        Key::Character(ch) if state.ctrl_down => {
            if let Some(c) = ch.as_str().chars().next() {
                let lo = c.to_ascii_lowercase();
                if ('a'..='z').contains(&lo) {
                    state.send_terminal_input(&[lo as u8 - b'a' + 1]);
                } else if c == '[' {
                    state.send_terminal_input(b"\x1b");
                } else if c == '\\' {
                    state.send_terminal_input(b"\x1c");
                } else if c == ']' {
                    state.send_terminal_input(b"\x1d");
                }
            }
        }

        Key::Named(NamedKey::Enter) => {
            if state.shift_down {
                state.tab_mut().app.insert_editor_input("\n");
            } else {
                state.tab_mut().scroll_offset = 0;
                state.run_editor_command();
            }
        }
        Key::Named(NamedKey::Backspace) => {
            state.tab_mut().app.editor_backspace();
        }
        Key::Named(NamedKey::Delete) => {
            state.tab_mut().app.editor_delete_forward();
        }
        Key::Named(NamedKey::ArrowLeft) => {
            let extend = state.shift_down;
            state.tab_mut().app.editor_move_cursor_left(extend);
        }
        Key::Named(NamedKey::ArrowRight) => {
            let extend = state.shift_down;
            state.tab_mut().app.editor_move_cursor_right(extend);
        }
        Key::Named(NamedKey::ArrowUp) => {
            let text = state.tab().app.editor_snapshot();
            let offset = state.tab().app.editor_cursor_offset();
            let (row, col) = editor_cursor_row_col(&text, offset);
            if row == 0 && !state.shift_down {
                state.history_prev();
            } else if row > 0 {
                let new_offset = editor_row_col_to_offset(&text, row - 1, col);
                let extend = state.shift_down;
                state.tab_mut().app.set_editor_cursor(new_offset, extend);
            }
        }
        Key::Named(NamedKey::ArrowDown) => {
            let text = state.tab().app.editor_snapshot();
            let offset = state.tab().app.editor_cursor_offset();
            let (row, col) = editor_cursor_row_col(&text, offset);
            let last_row = text.lines().count().saturating_sub(1);
            if row >= last_row && !state.shift_down {
                state.history_next();
            } else if row < last_row {
                let new_offset = editor_row_col_to_offset(&text, row + 1, col);
                let extend = state.shift_down;
                state.tab_mut().app.set_editor_cursor(new_offset, extend);
            }
        }
        Key::Named(NamedKey::Home) => {
            state.tab_mut().app.set_editor_cursor(0, false);
        }
        Key::Named(NamedKey::End) => {
            let end = state.tab().app.editor_snapshot().len();
            state.tab_mut().app.set_editor_cursor(end, false);
        }
        Key::Named(NamedKey::Space) => {
            state.tab_mut().app.insert_editor_input(" ");
        }
        Key::Character(_) => {
            if let Some(text) = key_event.text.as_ref() {
                if text != "\n" && text != "\r" && text != "\r\n" {
                    state.tab_mut().app.insert_editor_input(text.as_str());
                }
            }
        }
        _ => {}
    }
    clamp_editor_scroll(state);
}
