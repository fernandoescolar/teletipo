#![doc = "Core editor buffer, cursor, selection, and command-domain types."]
#![warn(missing_docs)]
#![allow(missing_docs)]

mod buffer;
mod gap_buffer;
mod history;
mod types;

pub use buffer::EditorBuffer;
pub use gap_buffer::GapBuffer;
pub use types::{BufferEngineKind, Cursor, Selection, SemanticCommand};
