use crate::actions::UiAction;
use crate::state::UiState;
use render_wgpu::AppWindowEvent;
use winit::event::{ElementState, MouseButton};

pub(super) fn map_pointer_event(state: &UiState, event: &AppWindowEvent) -> Vec<UiAction> {
    match event {
        AppWindowEvent::WindowMoved { x, y } => {
            vec![UiAction::CursorMoved {
                x: *x as f64,
                y: *y as f64,
            }]
        }
        AppWindowEvent::Resized {
            width,
            height,
            scale_factor,
            cell_w,
            cell_h,
        } => vec![UiAction::Resized {
            width: *width,
            height: *height,
            scale: *scale_factor,
            cell_w: *cell_w,
            cell_h: *cell_h,
        }],
        AppWindowEvent::CursorMoved { x, y } => vec![UiAction::CursorMoved { x: *x, y: *y }],
        AppWindowEvent::MouseWheel { delta_lines } => vec![UiAction::MouseWheel(*delta_lines)],
        AppWindowEvent::MouseInput {
            state: ElementState::Pressed,
            button: MouseButton::Left,
        } => {
            let row = (state.window.cursor_y / state.window.cell_h as f64).max(0.0) as usize;
            let col = (state.window.cursor_x / state.window.cell_w as f64).max(0.0) as usize;
            vec![UiAction::SelectionBegin { row, col }]
        }
        AppWindowEvent::MouseInput {
            state: ElementState::Released,
            button: MouseButton::Left,
        } => vec![UiAction::SelectionEnd],
        AppWindowEvent::ModifiersChanged(mods) => vec![UiAction::ModifiersChanged(
            crate::components::ModifierState {
                ctrl: mods.control_key(),
                super_key: mods.super_key(),
                shift: mods.shift_key(),
                alt: mods.alt_key(),
            },
        )],
        _ => Vec::new(),
    }
}
