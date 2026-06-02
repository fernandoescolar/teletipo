use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub(crate) fn hash_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Returns `true` for characters that are typically icon/symbol glyphs from
/// Nerd Fonts or Unicode symbol blocks (Private Use Areas, arrows, etc.).
pub(crate) fn is_icon_like(ch: char) -> bool {
    let cp = ch as u32;
    (0xE000..=0xF8FF).contains(&cp)
        || (0xF0000..=0xFFFFD).contains(&cp)
        || (0x100000..=0x10FFFD).contains(&cp)
        || (0x2190..=0x21FF).contains(&cp)
        || (0x2300..=0x23FF).contains(&cp)
        || (0x2500..=0x257F).contains(&cp)
        || (0x2580..=0x259F).contains(&cp)
        || (0x2600..=0x26FF).contains(&cp)
        || (0x2700..=0x27BF).contains(&cp)
}

pub(crate) fn normalize_rect_selection(
    r0: usize,
    c0: usize,
    r1: usize,
    c1: usize,
) -> (usize, usize, usize, usize) {
    if (r0, c0) <= (r1, c1) {
        (r0, c0, r1, c1)
    } else {
        (r1, c1, r0, c0)
    }
}

/// Returns the number of terminal columns occupied by `ch` (1 for narrow, 2
/// for wide — emoji, CJK, etc.).  Mirrors the same function in render-wgpu.
pub(crate) fn char_col_width(ch: char) -> usize {
    let cp = ch as u32;
    if matches!(cp,
        0x1100..=0x115F   // Hangul Jamo
        | 0x2E80..=0x303E // CJK Radicals + Symbols
        | 0x3041..=0x33FF // Japanese, Korean
        | 0x3400..=0x9FFF // CJK Unified Ideographs
        | 0xAC00..=0xD7FF // Hangul Syllables
        | 0xF900..=0xFAFF // CJK Compatibility
        | 0xFE30..=0xFE6F // CJK Compatibility Forms
        | 0xFF01..=0xFF60 // Fullwidth ASCII variants
        | 0xFFE0..=0xFFE6 // Fullwidth Signs
        | 0x1F000..=0x1FAFF // Emoji (main blocks)
    ) {
        2
    } else {
        1
    }
}

pub(crate) fn editor_offset_to_row_col(text: &str, cursor_offset: usize) -> (usize, usize) {
    let clamped = cursor_offset.min(text.len());
    let before = &text[..clamped];
    let row = before.chars().filter(|&c| c == '\n').count();
    // Visual column: sum of display widths of chars on the current line.
    let col: usize = match before.rfind('\n') {
        Some(pos) => before[pos + 1..].chars().map(char_col_width).sum(),
        None => before.chars().map(char_col_width).sum(),
    };
    (row, col)
}
