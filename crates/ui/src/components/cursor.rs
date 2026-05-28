//! Cursor blink, bell flash, modifier-key, and window-metrics state.

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorBlink {
    pub last_toggle: Instant,
    pub phase: bool,
}

impl Default for CursorBlink {
    fn default() -> Self {
        Self {
            last_toggle: Instant::now(),
            phase: true,
        }
    }
}

impl CursorBlink {
    pub fn tick(&mut self) {
        const BLINK_HALF_MS: u128 = 500;
        if self.last_toggle.elapsed().as_millis() >= BLINK_HALF_MS {
            self.phase = !self.phase;
            self.last_toggle = Instant::now();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BellState {
    pub flash_until: Option<Instant>,
}

impl BellState {
    pub fn is_active(&self) -> bool {
        self.flash_until
            .map(|deadline| Instant::now() < deadline)
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifierState {
    pub ctrl: bool,
    pub super_key: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowMetrics {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale_factor: f64,
    pub cell_w: f32,
    pub cell_h: f32,
    pub cursor_x: f64,
    pub cursor_y: f64,
}

impl Default for WindowMetrics {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            x: 0,
            y: 0,
            scale_factor: 1.0,
            cell_w: 9.0,
            cell_h: 18.0,
            cursor_x: 0.0,
            cursor_y: 0.0,
        }
    }
}
