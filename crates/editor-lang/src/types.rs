use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightRange {
    pub range: Range<usize>,
    pub token: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncrementalSnapshot {
    pub version: u64,
    pub highlights: Vec<HighlightRange>,
    pub changed_ranges: Vec<Range<usize>>,
}
