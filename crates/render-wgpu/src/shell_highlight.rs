//! Minimal shell-syntax highlighter used by the editor pane.
//!
//! Returns a per-character colour palette so the renderer can emit coloured
//! glyphs without round-tripping through `editor-lang`. Intentionally tiny:
//! enough to make pipelines, flags, quoted strings, comments and `$variables`
//! visually distinct in the rendered editor buffer.

/// Returns a per-character syntax colour for a shell command string.
/// `None` means "use the renderer default colour".
/// Handles: keywords (purple), commands (cyan), flags (yellow),
/// quoted strings (amber), variable references (green), comments (dim gray).
#[allow(clippy::too_many_lines)] // long table-style match for shell token coloring
pub fn highlight_shell(text: &str) -> Vec<Option<[f32; 3]>> {
    const KEYWORD: [f32; 3] = [0.78, 0.55, 0.96]; // soft purple
    const COMMAND: [f32; 3] = [0.40, 0.88, 1.00]; // cyan
    const FLAG: [f32; 3] = [0.97, 0.90, 0.40]; // yellow
    const STRING: [f32; 3] = [1.00, 0.72, 0.30]; // amber
    const COMMENT: [f32; 3] = [0.55, 0.57, 0.60]; // dim gray
    const VAR: [f32; 3] = [0.56, 0.93, 0.56]; // soft green

    const SH_KEYWORDS: &[&str] = &[
        "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac",
        "in", "function", "return", "break", "continue",
    ];

    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out: Vec<Option<[f32; 3]>> = vec![None; n];
    let mut i = 0usize;
    // true before the first real (non-whitespace, non-newline) word on each line
    let mut first_word = true;

    while i < n {
        let ch = chars[i];
        match ch {
            '\n' => {
                i += 1;
                first_word = true;
            }
            ' ' | '\t' => {
                i += 1;
            }
            '#' => {
                while i < n && chars[i] != '\n' {
                    out[i] = Some(COMMENT);
                    i += 1;
                }
            }
            '"' | '\'' => {
                let quote = ch;
                out[i] = Some(STRING);
                i += 1;
                while i < n {
                    if chars[i] == '\\' && quote == '"' && i + 1 < n {
                        out[i] = Some(STRING);
                        i += 1;
                        out[i] = Some(STRING);
                        i += 1;
                    } else if chars[i] == quote {
                        out[i] = Some(STRING);
                        i += 1;
                        break;
                    } else {
                        out[i] = Some(STRING);
                        i += 1;
                    }
                }
                first_word = false;
            }
            '$' => {
                let start = i;
                i += 1;
                if i < n && chars[i] == '{' {
                    i += 1;
                    while i < n && chars[i] != '}' {
                        i += 1;
                    }
                    if i < n {
                        i += 1;
                    } // consume '}'
                } else {
                    while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    // special vars: $@, $*, $#, $?, $!, $0-$9
                    if i == start + 1 && i < n && "@*#?!0123456789".contains(chars[i]) {
                        i += 1;
                    }
                }
                for item in out[start..i].iter_mut() {
                    *item = Some(VAR);
                }
            }
            ';' => {
                i += 1;
                if i < n && chars[i] == ';' {
                    i += 1;
                } // ;;
                first_word = true;
            }
            '|' | '&' => {
                out[i] = None;
                i += 1;
                if i < n && (chars[i] == '|' || chars[i] == '&') {
                    i += 1; // || or &&
                }
                first_word = true;
            }
            ch if ch.is_alphanumeric()
                || matches!(ch, '_' | '-' | '.' | '/' | '~' | '@' | ':' | '=') =>
            {
                let word_start = i;
                while i < n {
                    let wch = chars[i];
                    if wch.is_whitespace()
                        || matches!(
                            wch,
                            '"' | '\'' | '$' | '#' | ';' | '|' | '&' | '(' | ')' | '<' | '>' | '`'
                        )
                    {
                        break;
                    }
                    i += 1;
                }
                let word: String = chars[word_start..i].iter().collect();
                let color = if word.starts_with('-') {
                    Some(FLAG)
                } else if SH_KEYWORDS.contains(&word.as_str()) {
                    Some(KEYWORD)
                } else if first_word {
                    Some(COMMAND)
                } else {
                    None
                };
                for item in out[word_start..i].iter_mut() {
                    *item = color;
                }
                first_word = false;
            }
            _ => {
                i += 1;
            }
        }
    }

    out
}
