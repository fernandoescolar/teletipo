use crate::components::UiConfig;
use crate::config::{numeric_step, SETTINGS_FIELDS};
use crate::input::{current_line_prefix, cursor_at_line_end, detect_terminal_links};
use crate::state::UiState;
use render_wgpu::{
    ColorTheme, RenderSnapshot, SettingsItem, SettingsOverlay, SuggestionDropdown, TabContextMenu,
    TerminalLink,
};

/// Truncate `s` to at most `max_chars` Unicode scalar values, appending `…`
/// if the string is longer.
fn truncate_display(s: &str, max_chars: usize) -> String {
    let mut char_indices = s.char_indices();
    match char_indices.nth(max_chars) {
        None => s.to_owned(),
        Some((byte_pos, _)) => format!("{}…", &s[..byte_pos]),
    }
}

pub fn theme_from_config(theme: Option<&ColorTheme>) -> ColorTheme {
    theme.cloned().unwrap_or_default()
}

/// Maximum search dropdown rows visible at once.
const SEARCH_MAX_VISIBLE: usize = 8;

pub fn build_settings_overlay(state: &UiState) -> Option<SettingsOverlay> {
    if !state.overlays.settings.open {
        return None;
    }

    let mut items: Vec<SettingsItem> = Vec::new();
    let mut last_section = "";
    for field in SETTINGS_FIELDS {
        if field.section != last_section {
            last_section = field.section;
            items.push(SettingsItem {
                is_header: true,
                is_selectable: false,
                is_searchable: false,
                key: format!("[{}]", field.section),
                value: String::new(),
            });
        }
        let is_searchable = (field.section == "font" && field.key == "family")
            || field.key == "theme";
        let is_selectable = is_searchable
            || numeric_step(field.section, field.key).is_some();
        let value = state.config.get_field(field.section, field.key);
        items.push(SettingsItem {
            is_header: false,
            is_selectable,
            is_searchable,
            key: field.key.to_owned(),
            value,
        });
    }

    let n_fields = SETTINGS_FIELDS.len();
    let cursor = state.overlays.settings.cursor.min(n_fields.saturating_sub(1));

    let (search_matches, search_selected, search_scroll_offset) =
        if let Some(ref buf) = state.overlays.settings.search_buf {
            let q = buf.to_lowercase();
            let field = &SETTINGS_FIELDS[cursor];
            let matches: Vec<String> = if field.section == "font" && field.key == "family" {
                state.config.available_fonts.iter()
                    .filter(|f| f.to_lowercase().contains(&q))
                    .cloned()
                    .collect()
            } else if field.key == "theme" {
                state.config.available_themes.iter()
                    .filter(|t| t.to_lowercase().contains(&q))
                    .cloned()
                    .collect()
            } else {
                vec![]
            };
            // Clamp scroll so the selected row stays visible.
            let sel = state.overlays.settings.search_selected;
            let mut off = state.overlays.settings.search_scroll_offset;
            if sel >= off + SEARCH_MAX_VISIBLE {
                off = sel.saturating_sub(SEARCH_MAX_VISIBLE - 1);
            } else if sel < off {
                off = sel;
            }
            let max_off = matches.len().saturating_sub(SEARCH_MAX_VISIBLE);
            off = off.min(max_off);
            (matches, sel, off)
        } else {
            (vec![], 0, 0)
        };

    Some(SettingsOverlay {
        items,
        cursor,
        editing: state.overlays.settings.edit_buf.clone(),
        just_saved: state.overlays.settings.just_saved,
        search_buf: state.overlays.settings.search_buf.clone(),
        search_matches,
        search_selected,
        search_scroll_offset,
    })
}

fn links_for_snapshot(terminal_text: &str) -> Vec<TerminalLink> {
    detect_terminal_links(terminal_text)
        .into_iter()
        .map(|(row, col_start, col_end, target)| TerminalLink {
            row,
            col_start,
            col_end,
            target,
        })
        .collect()
}

fn tab_context_menu(state: &UiState) -> Option<TabContextMenu> {
    state.tabs.context_menu.map(|menu| TabContextMenu {
        tab_idx: menu.tab_index,
        x_px: menu.x as f32,
        y_px: menu.y as f32,
        hovered_item: menu.hovered_item,
    })
}

fn suggestion_dropdown(
    state: &UiState,
    editor_text: &str,
    editor_cursor_offset: usize,
) -> Option<SuggestionDropdown> {
    let active = state.tabs.active_tab();
    let selected = active.suggestions.index?;

    let prefix = active
        .suggestions
        .prefix
        .as_deref()
        .unwrap_or_else(|| current_line_prefix(editor_text, editor_cursor_offset));

    let items: Vec<String> = active.app.frecency_suggestions(prefix, 20);
    if items.len() < 2 {
        return None;
    }

    const MAX_VISIBLE: usize = 8;
    let scroll_offset = selected
        .saturating_sub(MAX_VISIBLE - 1)
        .min(items.len().saturating_sub(MAX_VISIBLE));

    Some(SuggestionDropdown {
        items: items.into_iter().map(|s| truncate_display(&s, 50)).collect(),
        selected,
        scroll_offset,
    })
}

fn editor_suggestion_text(
    state: &UiState,
    editor_text: &str,
    editor_cursor_offset: usize,
) -> String {
    let active = state.tabs.active_tab();
    if let Some(idx) = active.suggestions.index {
        let prefix = active
            .suggestions
            .prefix
            .as_deref()
            .unwrap_or_else(|| current_line_prefix(editor_text, editor_cursor_offset));
        let matches = active.app.frecency_suggestions(prefix, idx + 2);
        return matches
            .get(idx)
            .map(|full| {
                if full.len() > prefix.len() {
                    truncate_display(&full[prefix.len()..], 80)
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();
    }

    if cursor_at_line_end(editor_text, editor_cursor_offset) {
        let prefix = current_line_prefix(editor_text, editor_cursor_offset);
        if !prefix.is_empty() {
            let matches = active.app.frecency_suggestions(prefix, 1);
            return matches
                .into_iter()
                .next()
                .map(|full| {
                    if full.len() > prefix.len() {
                        truncate_display(&full[prefix.len()..], 80)
                    } else {
                        String::new()
                    }
                })
                .unwrap_or_default();
        }
    }

    String::new()
}

pub fn build_snapshot(state: &mut UiState, theme: Option<&ColorTheme>, config: &UiConfig) -> RenderSnapshot {
    // Advance cursor blink and pump all PTYs.
    state.tick();

    // Clear one-shot just_saved flag after it has been shown for a frame.
    if state.overlays.settings.just_saved {
        state.overlays.settings.just_saved = false;
    }

    let active_tab = state.tabs.active;
    let editor_focused = state.layout.focus == crate::components::PaneFocus::Editor;
    let tab_context_menu = tab_context_menu(state);
    let tab_drag_from = state.tabs.drag.map(|drag| drag.tab_index);
    let settings_overlay = build_settings_overlay(state);
    let tab_labels = if state.tabs.tabs.len() > 1 {
        state
            .tabs
            .tabs
            .iter()
            .map(|pane| pane.cwd_label.clone())
            .collect()
    } else {
        Vec::new()
    };

    let resize_overlay = state.pending_update.as_ref().map(|v| {
        format!("Updated to v{v} \u{2014} restart to apply")
    });

    let (
        terminal_text,
        terminal_fg_colors,
        terminal_bg_colors,
        terminal_styles,
        editor_text,
        editor_cursor_offset,
        scroll_offset,
        scrollback_lines,
        split_ratio,
        editor_line_count,
        editor_scroll_offset,
        editor_selection,
        selection,
        title_cwd,
        cursor_shape,
        terminal_cursor_row,
        terminal_cursor_col,
        terminal_fullscreen,
    ) = {
        let tab = state.tabs.active_tab_mut();

        let styled = tab
            .app
            .terminal_styled_snapshot_at_offset(tab.scroll.terminal_offset);
        let terminal_text: String = styled.iter().map(|(ch, _, _, _)| *ch).collect();
        let terminal_fg_colors: Vec<Option<[f32; 3]>> =
            styled.iter().map(|(_, fg, _, _)| *fg).collect();
        let terminal_bg_colors: Vec<Option<[f32; 3]>> =
            styled.iter().map(|(_, _, bg, _)| *bg).collect();
        let terminal_styles: Vec<u8> = styled.iter().map(|(_, _, _, style)| *style).collect();
        let editor_text = tab.app.editor_snapshot();
        let editor_cursor_offset = tab.app.editor_cursor_offset();

        let selection = match (tab.terminal_selection.anchor, tab.terminal_selection.end) {
            (Some(a), Some(b)) => {
                let (sr, sc, er, ec) = if (a.row, a.col) <= (b.row, b.col) {
                    (a.row, a.col, b.row, b.col)
                } else {
                    (b.row, b.col, a.row, a.col)
                };
                Some((sr, sc, er, ec))
            }
            _ => None,
        };

        (
            terminal_text,
            terminal_fg_colors,
            terminal_bg_colors,
            terminal_styles,
            editor_text,
            editor_cursor_offset,
            tab.scroll.terminal_offset,
            tab.app.scrollback_len(),
            tab.split_ratio,
            tab.app.editor_snapshot().lines().count().max(1),
            tab.scroll.editor_offset,
            tab.app.editor_selection(),
            selection,
            tab.cwd_label.clone(),
            tab.app.cursor_shape(),
            tab.app.terminal_cursor_pos().0,
            tab.app.terminal_cursor_pos().1,
            tab.is_terminal_fullscreen,
        )
    };

    let editor_suggestion =
        editor_suggestion_text(state, &editor_text, editor_cursor_offset);
    let suggestion_dropdown =
        suggestion_dropdown(state, &editor_text, editor_cursor_offset);

    RenderSnapshot {
        terminal_text: terminal_text.clone(),
        terminal_fg_colors,
        terminal_bg_colors,
        terminal_styles,
        editor_text,
        editor_cursor_offset,
        scroll_offset,
        scrollback_lines,
        editor_focused,
        split_ratio,
        resize_overlay,
        editor_line_count,
        editor_scroll_offset,
        editor_selection,
        selection,
        tab_labels,
        active_tab,
        tab_context_menu,
        tab_drag_from,
        tab_drag_insert_before: None,
        theme: theme_from_config(theme),
        padding_h: config.padding_horizontal as u32,
        padding_v: config.padding_vertical as u32,
        settings_overlay,
        title_cwd,
        editor_suggestion,
        suggestion_dropdown,
        terminal_links: links_for_snapshot(&terminal_text),
        request_exit: state.should_exit,
        cursor_shape,
        bell_active: state.bell.is_active(),
        cursor_blink_on: state.cursor_blink.phase,
        terminal_cursor_row,
        terminal_cursor_col,
        terminal_fullscreen,
    }
}
