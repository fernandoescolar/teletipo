mod highlighter;
mod types;

pub use highlighter::{LanguageHighlighter, NoopHighlighter, ShellLikeHighlighter};
pub use types::{HighlightRange, IncrementalSnapshot};
