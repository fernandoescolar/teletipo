use std::ops::Range;

use crate::types::{HighlightRange, IncrementalSnapshot};

/// Pluggable syntax / token highlighter for an editor buffer.
///
/// `highlight` returns a fresh full-buffer highlight set; implementors that
/// can be incremental should override [`LanguageHighlighter::highlight_incremental`]
/// to skip unchanged regions.
pub trait LanguageHighlighter {
    /// Compute highlights for the entire buffer text.
    fn highlight(&self, text: &str) -> Vec<HighlightRange>;

    /// Compute an incremental snapshot relative to a previous one.
    ///
    /// Default implementation just re-highlights everything.
    fn highlight_incremental(
        &self,
        text: &str,
        previous: Option<&IncrementalSnapshot>,
    ) -> IncrementalSnapshot {
        let highlights = self.highlight(text);
        let changed_ranges = compute_changed_ranges(text, previous);
        IncrementalSnapshot {
            version: previous.map_or(1, |p| p.version.saturating_add(1)),
            highlights,
            changed_ranges,
        }
    }
}

#[derive(Default)]
pub struct NoopHighlighter;

impl LanguageHighlighter for NoopHighlighter {
    fn highlight(&self, _text: &str) -> Vec<HighlightRange> {
        Vec::new()
    }
}

#[derive(Default)]
pub struct ShellLikeHighlighter;

impl LanguageHighlighter for ShellLikeHighlighter {
    fn highlight(&self, text: &str) -> Vec<HighlightRange> {
        let mut ranges = Vec::new();
        let mut first_word_done = false;
        let mut idx = 0usize;

        for token in text.split_whitespace() {
            if let Some(rel) = text[idx..].find(token) {
                let start = idx + rel;
                let end = start + token.len();
                let t = if !first_word_done {
                    first_word_done = true;
                    "command"
                } else if token.starts_with('-') {
                    "flag"
                } else {
                    "arg"
                };
                ranges.push(HighlightRange {
                    range: start..end,
                    token: t,
                });
                idx = end;
            }
        }

        ranges
    }
}

#[allow(clippy::single_range_in_vec_init)]
pub(crate) fn compute_changed_ranges(
    text: &str,
    _previous: Option<&IncrementalSnapshot>,
) -> Vec<Range<usize>> {
    // Full re-highlight until incremental diffing is implemented.
    vec![0..text.len()]
}

#[cfg(test)]
mod tests {
    use super::{LanguageHighlighter, NoopHighlighter, ShellLikeHighlighter};

    #[test]
    fn noop_returns_no_ranges() {
        let h = NoopHighlighter;
        assert!(h.highlight("let x = 1;").is_empty());
    }

    #[test]
    fn shell_highlighter_marks_command_and_flags() {
        let h = ShellLikeHighlighter;
        let ranges = h.highlight("git status --short");
        assert_eq!(ranges[0].token, "command");
        assert_eq!(ranges[2].token, "flag");
    }

    #[test]
    fn incremental_snapshot_advances_version() {
        let h = ShellLikeHighlighter;
        let s1 = h.highlight_incremental("echo hello", None);
        let s2 = h.highlight_incremental("echo hello world", Some(&s1));
        assert!(s2.version > s1.version);
        assert!(!s2.changed_ranges.is_empty());
    }

    #[test]
    fn shell_highlighter_empty_input_yields_no_ranges() {
        assert!(ShellLikeHighlighter.highlight("").is_empty());
    }

    #[test]
    fn shell_highlighter_only_flags_after_command() {
        let ranges = ShellLikeHighlighter.highlight("ls -la -h");
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0].token, "command");
        assert_eq!(ranges[1].token, "flag");
        assert_eq!(ranges[2].token, "flag");
    }

    #[test]
    fn shell_highlighter_args_are_args() {
        let ranges = ShellLikeHighlighter.highlight("cp src dst");
        assert_eq!(ranges[0].token, "command");
        assert_eq!(ranges[1].token, "arg");
        assert_eq!(ranges[2].token, "arg");
    }

    #[test]
    fn shell_highlighter_ranges_match_substrings() {
        let text = "git push origin";
        let ranges = ShellLikeHighlighter.highlight(text);
        for r in &ranges {
            // The slice at the reported range must be the whitespace-delimited token.
            assert!(!text[r.range.clone()].contains(char::is_whitespace));
        }
    }

    #[test]
    fn incremental_snapshot_starts_at_version_one() {
        let s = ShellLikeHighlighter.highlight_incremental("ls", None);
        assert_eq!(s.version, 1);
    }
}
