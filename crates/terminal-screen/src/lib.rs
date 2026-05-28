mod cell;
mod color;
mod grid;
mod screen;

pub use cell::{Cell, CellStyle};
pub use color::{ansi_indexed_to_rgb, ansi_color_to_rgb_with_palette};
pub use screen::{DamageRegion, Screen, ScreenSnapshot};

/// Per-character styled data: (character, foreground RGB, background RGB).
/// `None` means the cell uses the renderer's default color.
pub type StyledChars = Vec<(char, Option<[f32; 3]>, Option<[f32; 3]>)>;
