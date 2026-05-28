use crate::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiColor {
    Default,
    Indexed(u8),
    TrueColor(u8, u8, u8),
}

/// Like `ansi_color_to_rgb` but checks `palette` (indices 0-15) before the
/// built-in xterm table. Passing `None` gives the same result as the plain
/// variant.
pub fn ansi_color_to_rgb_with_palette(
    c: AnsiColor,
    palette: Option<&[[f32; 3]; 16]>,
) -> Option<[f32; 3]> {
    match c {
        AnsiColor::Default => None,
        AnsiColor::Indexed(n) => {
            if let (Some(pal), 0..=15) = (palette, n) {
                Some(pal[n as usize])
            } else {
                Some(ansi_indexed_to_rgb(n))
            }
        }
        AnsiColor::TrueColor(r, g, b) => {
            Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
        }
    }
}

/// Resolves a cell into `(char, fg_rgb, bg_rgb)` while applying an optional
/// theme palette override for indexed colors.
pub fn ansi_cell_tuple_with_palette(
    cell: &Cell,
    palette: Option<&[[f32; 3]; 16]>,
) -> (char, Option<[f32; 3]>, Option<[f32; 3]>) {
    (
        cell.ch,
        ansi_color_to_rgb_with_palette(cell.style.fg, palette),
        ansi_color_to_rgb_with_palette(cell.style.bg, palette),
    )
}

/// * 0–15: the 16 standard ANSI/xterm colors
/// * 16–231: the 6×6×6 embedded color cube
/// * 232–255: the 24-step embedded grayscale ramp
pub fn ansi_indexed_to_rgb(idx: u8) -> [f32; 3] {
    match idx {
        0  => [0.000, 0.000, 0.000],
        1  => [0.502, 0.000, 0.000],
        2  => [0.000, 0.502, 0.000],
        3  => [0.502, 0.502, 0.000],
        4  => [0.000, 0.000, 0.502],
        5  => [0.502, 0.000, 0.502],
        6  => [0.000, 0.502, 0.502],
        7  => [0.753, 0.753, 0.753],
        8  => [0.502, 0.502, 0.502],
        9  => [1.000, 0.333, 0.333],
        10 => [0.333, 1.000, 0.333],
        11 => [1.000, 1.000, 0.333],
        12 => [0.333, 0.333, 1.000],
        13 => [1.000, 0.333, 1.000],
        14 => [0.333, 1.000, 1.000],
        15 => [1.000, 1.000, 1.000],
        16..=231 => {
            let i = idx - 16;
            let r = if i / 36 == 0 { 0.0 } else { (i / 36) as f32 / 5.0 };
            let g = if (i / 6).is_multiple_of(6) { 0.0 } else { ((i / 6) % 6) as f32 / 5.0 };
            let b = if i.is_multiple_of(6) { 0.0 } else { (i % 6) as f32 / 5.0 };
            [r, g, b]
        }
        232..=255 => {
            let l = (idx - 232) as f32 / 23.0;
            [l, l, l]
        }
    }
}
