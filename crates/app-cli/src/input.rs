mod keyboard;
mod pointer;

use crate::GpuRuntimeState;
use render_wgpu::AppWindowEvent;

pub(crate) fn handle_event(state: &mut GpuRuntimeState, event: AppWindowEvent) {
    if pointer::handle_event(state, &event) {
        return;
    }
    keyboard::handle_event(state, event);
}
