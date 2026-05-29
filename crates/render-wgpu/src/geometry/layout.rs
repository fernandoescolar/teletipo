use winit::dpi::{PhysicalPosition, PhysicalSize};

use crate::types::{PaneLayout, RenderSnapshot};

/// Shared size and padding calculations for geometry helpers.
#[derive(Clone, Copy)]
pub(crate) struct LayoutContext<'a> {
    size: PhysicalSize<u32>,
    snapshot: &'a RenderSnapshot,
    cell_w_px: f32,
    cell_h_px: f32,
}

impl<'a> LayoutContext<'a> {
    pub(crate) fn new(
        size: PhysicalSize<u32>,
        snapshot: &'a RenderSnapshot,
        cell_w_px: f32,
        cell_h_px: f32,
    ) -> Self {
        Self {
            size,
            snapshot,
            cell_w_px,
            cell_h_px,
        }
    }

    pub(crate) fn has_grid(self) -> bool {
        self.size.width > 0 && self.size.height > 0 && self.cell_w_px > 0.0 && self.cell_h_px > 0.0
    }

    pub(crate) fn px_x(self) -> f32 {
        if self.size.width > 0 {
            2.0 / self.size.width as f32
        } else {
            0.0
        }
    }

    pub(crate) fn px_y(self) -> f32 {
        if self.size.height > 0 {
            2.0 / self.size.height as f32
        } else {
            0.0
        }
    }

    pub(crate) fn window_width(self) -> f32 {
        self.size.width as f32
    }

    pub(crate) fn window_height(self) -> f32 {
        self.size.height as f32
    }

    pub(crate) fn tab_bar_h(self) -> f32 {
        if self.snapshot.tab_labels.is_empty() {
            0.0
        } else {
            self.cell_h_px
        }
    }

    pub(crate) fn available_h(self) -> f32 {
        self.window_height() - self.tab_bar_h()
    }

    pub(crate) fn edit_top_px(self) -> f32 {
        (self.tab_bar_h() + self.snapshot.split_ratio * self.available_h() + 2.0).round()
    }
}

pub fn snapshot_to_ime_area(
    snapshot: &RenderSnapshot,
    window_size: PhysicalSize<u32>,
) -> (PhysicalPosition<f64>, PhysicalSize<f64>) {
    let layout = PaneLayout {
        split_ratio: snapshot.split_ratio,
    };
    let (edit_y_top, edit_y_bottom) = layout.editor_bounds();

    let cols: usize = 80;
    let text = &snapshot.editor_text;
    let clamped = snapshot.editor_cursor_offset.min(text.len());
    let before = &text[..clamped];
    let row = before.chars().filter(|&c| c == '\n').count();
    let col = before
        .rfind('\n')
        .map(|i| clamped - i - 1)
        .unwrap_or(clamped);

    let lines = text.lines().count().max(1) as f64;
    let cell_w_ndc = 2.0_f64 / cols as f64;
    let cell_h_ndc = (edit_y_top - edit_y_bottom) as f64 / lines;

    let ndc_x = -1.0 + col as f64 * cell_w_ndc;
    let ndc_y = edit_y_top as f64 - (row as f64 + 1.0) * cell_h_ndc;

    let w = window_size.width as f64;
    let h = window_size.height as f64;
    let screen_x = (ndc_x + 1.0) / 2.0 * w;
    let screen_y = (1.0 - ndc_y) / 2.0 * h;
    let char_w = cell_w_ndc / 2.0 * w;
    let char_h = cell_h_ndc / 2.0 * h;

    (
        PhysicalPosition::new(screen_x, screen_y),
        PhysicalSize::new(char_w.max(1.0), char_h.max(1.0)),
    )
}
