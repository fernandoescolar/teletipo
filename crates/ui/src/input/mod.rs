mod coord_mapper;
mod keyboard;
mod pointer;

pub use coord_mapper::{
    clamp_editor_scroll_offset, cursor_to_terminal_cell, current_line_prefix, cursor_at_line_end,
    detect_terminal_links, editor_cursor_row_col, editor_row_col_to_offset, extract_selection,
    line_leading_spaces,
};

use crate::actions::UiAction;
use crate::state::UiState;
use render_wgpu::AppWindowEvent;

pub struct InputRouter;

impl InputRouter {
    pub fn process(state: &UiState, event: &AppWindowEvent) -> Vec<UiAction> {
        let mut actions = pointer::map_pointer_event(state, event);
        if actions.is_empty() {
            actions = keyboard::map_keyboard_event(state, event);
        }
        actions
    }
}
