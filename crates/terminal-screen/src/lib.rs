mod cell;
mod color;
mod grid;
mod screen;

pub use cell::{Cell, CellStyle};
pub use color::{ansi_indexed_to_rgb, ansi_color_to_rgb_with_palette};
pub use screen::{DamageRegion, Screen, ScreenSnapshot};

/// Per-character styled data: (character, foreground RGB, background RGB, style bits).
/// `None` on the color fields means the cell uses the renderer's default color.
/// Style bits: bit 0 = bold, bit 1 = italic, bit 2 = strikethrough.
pub type StyledChars = Vec<(char, Option<[f32; 3]>, Option<[f32; 3]>, u8)>;

/// Style bit flags used in the 4th element of `StyledChars`.
pub const STYLE_BOLD: u8          = 0b001;
pub const STYLE_ITALIC: u8        = 0b010;
pub const STYLE_STRIKETHROUGH: u8 = 0b100;
