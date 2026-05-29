#![doc = "Language highlighting abstractions and incremental highlight snapshots."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod highlighter;
mod types;

pub use highlighter::{LanguageHighlighter, NoopHighlighter, ShellLikeHighlighter};
pub use types::{HighlightRange, IncrementalSnapshot};
