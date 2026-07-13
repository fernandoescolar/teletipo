//! Text rendering with per-character styling support.

use gpui::{App, Pixels, SharedString, TextRun, Window, point, px};
use render_model::TextCommand;

use crate::color::text_color;

/// Paint a text command with per-character styling.
pub fn paint_text(command: &TextCommand, window: &mut Window, cx: &mut App, font_size: Pixels) {
    if command.text.is_empty() {
        return;
    }

    let style = window.text_style();
    let text_str = &command.text;

    // Build TextRun objects with per-character styling if available
    let runs = build_text_runs(
        text_str,
        style.font(),
        command.color,
        &command.char_colors,
        &command.char_styles,
    );

    if runs.is_empty() {
        return;
    }

    let line = window.text_system().shape_line(
        SharedString::from(text_str.clone()),
        font_size,
        &runs,
        None,
    );
    let _ = line.paint(
        point(px(command.x), px(command.y)),
        window.line_height(),
        window,
        cx,
    );
}

/// Build TextRun objects with per-character styling from render-model TextStyle.
pub(crate) fn build_text_runs(
    text: &str,
    base_font: gpui::Font,
    base_color: [f32; 4],
    char_colors: &Option<Vec<[f32; 4]>>,
    char_styles: &Option<Vec<render_model::TextStyle>>,
) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();

    for (char_idx, ch) in text.chars().enumerate() {
        let char_len = ch.len_utf8();

        // Get styling for this character
        let color = char_colors
            .as_ref()
            .and_then(|colors| colors.get(char_idx))
            .copied()
            .unwrap_or(base_color);

        let text_style = char_styles
            .as_ref()
            .and_then(|styles| styles.get(char_idx))
            .copied()
            .unwrap_or_default();

        // Create font with proper weight and style
        let font = gpui::Font {
            weight: if text_style.bold {
                gpui::FontWeight::BOLD
            } else {
                gpui::FontWeight::NORMAL
            },
            style: if text_style.italic {
                gpui::FontStyle::Italic
            } else {
                gpui::FontStyle::Normal
            },
            ..base_font.clone()
        };

        // Create strikethrough if needed
        let strikethrough = if text_style.strike {
            Some(gpui::StrikethroughStyle {
                color: Some(text_color(color, 1.0)),
                thickness: px(1.0),
            })
        } else {
            None
        };

        // TODO: Add "dim" support by reducing brightness of color
        // For now, we skip the dim attribute as GPUI doesn't have a direct opacity/brightness modifier

        let run = TextRun {
            len: char_len,
            font,
            color: text_color(color, 1.0),
            background_color: None,
            underline: None,
            strikethrough,
        };

        // Combine with previous run if styling is identical
        if let Some(last_run) = runs.last_mut()
            && last_run.font == run.font
            && last_run.color == run.color
            && last_run.strikethrough == run.strikethrough
        {
            last_run.len += char_len;
            continue;
        }

        runs.push(run);
    }

    runs
}
