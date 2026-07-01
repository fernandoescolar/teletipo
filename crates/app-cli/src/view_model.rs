//! Read-only view model construction.
//!
//! Functions in this module construct the visual representation of the current state
//! without modifying `GpuRuntimeState`. They depend on state populated by `tick::*` operations.

use crate::GpuRuntimeState;
use render_model::{CommandPalette, ContextMenu};

/// Build copy mode highlights and cursor position for the active tab.
#[allow(clippy::type_complexity, dead_code)]
pub(crate) fn copy_mode_section(
    state: &GpuRuntimeState,
    active: usize,
) -> (Vec<(usize, usize, usize)>, Option<(usize, usize)>) {
    let tab = &state.tabs[active];
    if !tab.copy_mode.active {
        return (Vec::new(), None);
    }

    let visible_rows = tab.term_row_count.max(1);
    let total_rows = (tab.app.scrollback_len() + visible_rows).max(visible_rows);
    let window_start = total_rows
        .saturating_sub(visible_rows)
        .saturating_sub(tab.scroll_offset.min(tab.app.scrollback_len()));
    let window_end = window_start.saturating_add(visible_rows);

    // Build selection highlights if anchor is set
    let mut highlights = Vec::new();
    if let Some((anchor_row, anchor_col)) = tab.copy_mode.anchor {
        let cursor_row = tab.copy_mode.cursor_row;
        let cursor_col = tab.copy_mode.cursor_col;

        // Normalize selection bounds
        let (start_row, start_col, end_row, end_col) =
            if anchor_row > cursor_row || (anchor_row == cursor_row && anchor_col > cursor_col) {
                (cursor_row, cursor_col, anchor_row, anchor_col)
            } else {
                (anchor_row, anchor_col, cursor_row, cursor_col)
            };

        // Convert from scrollback-relative coordinates to viewport coordinates
        // scrollback_len() + rows covers the entire terminal height (scrollback + visible grid)
        // Row 0 = current screen bottom (latest output), negative rows = scrollback
        let scrollback_len = tab.app.scrollback_len() as isize;
        let abs_start_row = (scrollback_len + start_row) as usize;
        let abs_end_row = (scrollback_len + end_row) as usize;

        // Add selection highlights for all rows in range
        if abs_start_row < window_end && abs_end_row >= window_start {
            let vis_start_row = abs_start_row.saturating_sub(window_start);
            let vis_end_row = (abs_end_row + 1).min(window_end) - window_start;

            if vis_start_row == vis_end_row {
                // Single-row selection
                highlights.push((vis_start_row, start_col, end_col));
            } else {
                // Multi-row selection: highlight entire rows in between
                if abs_start_row >= window_start {
                    highlights.push((vis_start_row, start_col, 200)); // Start row: from start_col to EOL
                }
                for row in (abs_start_row + 1)..abs_end_row {
                    if row >= window_start && row < window_end {
                        let vis_row = row - window_start;
                        highlights.push((vis_row, 0, 200)); // Full row width
                    }
                }
                if abs_end_row < window_end {
                    let vis_row = abs_end_row - window_start;
                    highlights.push((vis_row, 0, end_col)); // End row: from 0 to end_col
                }
            }
        }
    }

    // Build cursor position (viewport coordinates)
    let scrollback_len = tab.app.scrollback_len() as isize;
    let abs_cursor_row = (scrollback_len + tab.copy_mode.cursor_row) as usize;
    let cursor_pos = if abs_cursor_row >= window_start && abs_cursor_row < window_end {
        Some((abs_cursor_row - window_start, tab.copy_mode.cursor_col))
    } else {
        None
    };

    (highlights, cursor_pos)
}

/// Build generic context menu from state, if one is open.
#[allow(dead_code)]
pub(crate) fn context_menu(state: &GpuRuntimeState) -> Option<ContextMenu> {
    state.overlays.context_menu.as_ref().map(|m| ContextMenu {
        x_px: m.x_px as f32,
        y_px: m.y_px as f32,
        items: m.items.clone(),
        enabled_items: m.enabled_items.clone(),
        hovered_item: m.hovered_item,
    })
}

/// Build command palette snapshot for rendering, if palette is open.
#[allow(dead_code)]
pub(crate) fn command_palette(state: &GpuRuntimeState) -> Option<CommandPalette> {
    state.command_palette.as_ref().map(|cp| {
        let sub_prompt_label = cp.sub_prompt.as_ref().map(|sp| match sp {
            crate::state::SubPrompt::Ssh => "SSH → New connection (user@host):".to_owned(),
            crate::state::SubPrompt::SnippetPlaceholders {
                placeholders,
                current_placeholder_idx,
                options,
                current_option_idx,
                ..
            } => {
                if *current_placeholder_idx < placeholders.len() {
                    let placeholder_name = &placeholders[*current_placeholder_idx];
                    let available_options = options.get(*current_placeholder_idx);
                    if let Some(opts) = available_options {
                        if opts.is_empty() {
                            // No options available: accept free-form input
                            format!(
                                "Enter {} (type value or press Enter to skip):",
                                placeholder_name
                            )
                        } else {
                            // Show dropdown options
                            let mut label =
                                format!("Select {} (↑/↓ to navigate):\n", placeholder_name);
                            for (idx, opt) in opts.iter().enumerate() {
                                if idx == *current_option_idx {
                                    label.push_str(&format!("  > {}\n", opt));
                                } else {
                                    label.push_str(&format!("    {}\n", opt));
                                }
                            }
                            label
                        }
                    } else {
                        format!("Select {} (loading options...):", placeholder_name)
                    }
                } else {
                    "Executing snippet...".to_owned()
                }
            }
        });
        let items: Vec<String> = if sub_prompt_label.is_some() {
            vec![]
        } else {
            cp.filtered
                .iter()
                .map(|&i| cp.all_items[i].label.clone())
                .collect()
        };
        let cursor_char = cp.query[..cp.cursor_byte.min(cp.query.len())]
            .chars()
            .count();
        render_model::CommandPalette {
            query: cp.query.clone(),
            cursor_char,
            items,
            selected: cp.selected,
            scroll_offset: cp.scroll_offset,
            sub_prompt_label,
        }
    })
}
