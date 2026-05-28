use std::ops::Range;

use crate::types::{HighlightRange, IncrementalSnapshot};

pub trait LanguageHighlighter {
    fn highlight(&self, text: &str) -> Vec<HighlightRange>;

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
}
