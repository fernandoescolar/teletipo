use crate::components::UiConfig;
use crate::input::detect_terminal_links;
use crate::state::UiState;
use render_wgpu::{
    ColorTheme, RenderSnapshot, SettingsItem, SettingsOverlay, SuggestionDropdown, TabContextMenu,
    TerminalLink,
};

pub fn theme_from_config(theme: Option<&ColorTheme>) -> ColorTheme {
    theme.cloned().unwrap_or_default()
}

pub fn build_settings_overlay(state: &UiState) -> Option<SettingsOverlay> {
    if !state.overlays.settings.open {
        return None;
    }

    Some(SettingsOverlay {
        items: vec![SettingsItem {
            is_header: true,
            is_selectable: false,
            is_searchable: false,
            key: "[settings]".to_owned(),
            value: String::new(),
        }],
        cursor: state.overlays.settings.cursor,
        editing: state.overlays.settings.edit_buf.clone(),
        just_saved: state.overlays.settings.just_saved,
        search_buf: state.overlays.settings.search_buf.clone(),
        search_matches: Vec::new(),
        search_selected: state.overlays.settings.search_selected,
        search_scroll_offset: state.overlays.settings.search_scroll_offset,
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

fn suggestion_dropdown(state: &UiState) -> Option<SuggestionDropdown> {
    let active = state.tabs.active_tab();
    active.suggestions.index.map(|selected| SuggestionDropdown {
        items: vec![active
            .suggestions
            .prefix
            .clone()
            .unwrap_or_else(|| String::from(""))],
        selected,
        scroll_offset: 0,
    })
}

pub fn build_snapshot(state: &mut UiState, theme: Option<&ColorTheme>, config: &UiConfig) -> RenderSnapshot {
    state.cursor_blink.tick();

    let active_tab = state.tabs.active;
    let editor_focused = state.layout.focus == crate::components::PaneFocus::Editor;
    let tab_context_menu = tab_context_menu(state);
    let tab_drag_from = state.tabs.drag.map(|drag| drag.tab_index);
    let settings_overlay = build_settings_overlay(state);
    let suggestion_dropdown = suggestion_dropdown(state);
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
        if let Some(mut pty) = tab.pty.take() {
            let had_data = tab.app.pump_pty_once(&mut pty).map(|n| n > 0).unwrap_or(false);
            tab.pty = Some(pty);
            if had_data {
                tab.scroll.terminal_offset = 0;
                state.cursor_blink.phase = true;
            }
        }

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
        resize_overlay: None,
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
        editor_suggestion: String::new(),
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
